---
title: 任务、视图与输出一致性
description: 记录跨 Rust、IPC、Svelte 的任务生命周期与恢复顺序。
scope:
  - /src/components/Panel.svelte
  - /src/lib/types.ts
  - /src-tauri/src/execution/**
  - /src-tauri/src/desktop/**
updated: 2026-10-11
---

# 任务与视图

[/src-tauri/src/execution/mod.rs](/src-tauri/src/execution/mod.rs) → `RunManager` 拥有任务；[/src/components/Panel.svelte](/src/components/Panel.svelte) 拥有草稿与当前显示副本。

| 操作 | 任务变化 | 视图变化 |
| --- | --- | --- |
| 发送 | 建立 runId，冻结配置和 Script 源码版本 | 切到新任务；启动错误保留输入 |
| detach | 继续执行、移除面板订阅 | 空输入；可发送另一任务 |
| 收起 | 无 | 保留草稿/运行中视图；完成任务收起后清空 |
| 从任务页查看 | 不重跑；读取快照并订阅 | 显示原始输入和已累计输出 |
| 停止 | 请求取消 runner | 保留已收到输出；取消不撤销副作用 |
| 退出应用 | 所有任务收到取消信号 | 托盘退出；后台不作为独立服务继续 |

状态从 running 转到 succeeded、failed 或 cancelled 一次。foreground runId 是显示绑定，不是执行状态。任务页仅查询摘要，不创建另一个流式订阅者。

## 结果未读

运行中的任务不是未读结果。成功或失败时产生待查看结果，主动取消不产生提醒。输入面板和气泡只提示这种未读结果，不常驻展示任务总数；任务页仍保留运行状态和历史分组。

订阅与查看分开：隐藏面板可以继续接收输出，但不能据此清除未读。Panel 在终态已渲染、面板打开且获得焦点后，显式确认对应 runId 的已读状态。仅打开任务列表、查看运行中进度或收起面板都不提前确认未来结果。确认由 Rust 持久化，避免重启后同一结果重新提示。

## 输出时序

1. runner 连续排空 stdout/stderr 或响应流。
2. Rust 在有界缓冲中累计输出；每约 50 ms 发布增量。
3. `subscribe` 在同一把 entries 锁下发送快照并安装订阅，重设增量偏移。两步之间不允许写入。
4. 快照包含当前 seq，后续更新必须严格为 seq+1。
5. [/src/lib/types.ts](/src/lib/types.ts) → `applyRunEvent` 忽略旧序号和其他 runId，遇到缺口提示重新打开任务。
6. Panel 的 attachmentGeneration 排除先前订阅的迟到回调。

IPC 使用 [Tauri Channel](https://v2.tauri.app/develop/calling-frontend/#channels)。顺序保证不取代订阅边界上的互斥。界面隐藏、连接断开、输出容量达上限，都不能阻止 runner 继续读取管道。

buffered 与 live 只控制显示。buffered 仍累计、排空和保存输出；完成、失败或取消后可以查看已有结果。自动跟随底部只在用户仍位于底部时启用。

## 取消与通知顺序

RunManager 等待执行、取消或超时，同时驱动更新。在发布终态之前 drop runner：Process 的 kill_on_drop 和进程树 guard 负责收尾，然后才保存终态、通知、持久化历史。

通知策略由提交时的配置决定。background 通知要求完成时当前任务并未在有焦点的面板中查看；被其他软件夺走焦点也算后台。取消不通知。通知失败只记录日志，不改变已完成任务的成功状态。

HTTP 写请求超时、取消或响应中断可能已经产生副作用，界面标记提交状态不明；不自动重试。GET/HEAD 不应呈现写入状态不明。业务成功条件只能确认配置声明的结果，不承诺同步或索引完成。

## 排查入口

运行详情将 trace 与 stderr 分开。Rust 在实际准备与执行位置追加记录，包括路径解析、参数插值、冻结脚本、进程或请求启动、返回状态与失败位置。已接受动作的准备失败也保留失败任务，不再只显示一次性提交错误；输入超限、未知动作和并行容量拒绝仍属于提交失败。

Trace 记录当次配置版本、实际参数和冻结源码，后续编辑不会改写旧任务。它随完成历史保存，容量达到上限时标记截断，不能停止执行或排空输出。旧历史缺少 trace 时保持可读，不能从当前配置伪造记录。stdout/stderr 仍是独立管道，不承诺其跨管道顺序；trace 不是脚本内部逐行调试器。

已识别的鉴权字段与 HTTP 环境变量头值脱敏；不复制整个继承环境，不记录 HTTP 错误响应体。实际输入、普通参数、请求体和脚本源码仍可能含敏感文本，本地历史不是通用秘密扫描器。runId、configRevision、scriptRevision 和 seq 用于定位当次执行与 IPC 输出。

若后台返回但视图没更新：先查看任务摘要终态，再定位 Channel/attachmentGeneration；若任务始终 running：检查输入是否得到 EOF、脚本是否 flush、子进程是否持有输出管道。
