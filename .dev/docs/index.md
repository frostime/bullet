---
title: 开发文档入口
description: 根据修改或排查任务定位需要阅读的跨模块文档。
scope:
  - /.dev/docs/**
updated: 2026-10-09
---

- [architecture.md](architecture.md)：更改模块边界、新增动作能力、决定共享规则的归属时阅读。解释配置与执行为何分开、窗口拆分的代价。
- [task-lifecycle.md](task-lifecycle.md)：排查输出丢失、重复、detach、后台通知或取消顺序时阅读。涉及 Rust、IPC 和 Svelte 的共同约束。
- [cli-resolution.md](cli-resolution.md)：排查 Windows 中 pi/npm 等命令找不到、PATH 差异或 cwd 与程序路径混淆时阅读。
- [windows-validation.md](windows-validation.md)：搭建开发环境、构建安装包、验收多屏与通知时阅读。区分编译/自动化证据与必须在 Windows 检查的行为。
- [validation.md](validation.md)：交付时已执行的检查及其证据边界。

模块契约：[配置与托管脚本](../../src-tauri/src/config/config.SPEC.md)、[执行](../../src-tauri/src/execution/execution.SPEC.md)、[桌面窗口](../../src-tauri/src/desktop/desktop.SPEC.md)、[界面](../../src/src.SPEC.md)。
