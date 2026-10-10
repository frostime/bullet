---
title: 构建与 Windows 原生验收
description: 提供平台构建顺序和跨浏览器测试无法确认的验收项目。
scope:
  - /src-tauri/**
  - /scripts/**
  - /.github/workflows/**
updated: 2026-10-11
---

# 构建顺序

依据 [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)，安装 Visual Studio C++ Build Tools、Windows SDK、WebView2 和 Rust MSVC；项目 Node.js 使用 22.12+，Rust 下限以 Cargo.toml 为准。CLI 运行环境由用户安装，bullet 不下载 pi/uv/Python/Node。

从项目根目录执行 `npm ci`，然后 `npm run check`、`npm test`、`npm run build`。`npm run desktop` 启动真实桌面应用；`npm run desktop:build` 生成 NSIS 安装包。[/scripts/build-windows.ps1](/scripts/build-windows.ps1) 将检查、核心测试与打包串成一次构建。

核心测试 `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` 不依赖窗口。Windows CI 在 [/.github/workflows/windows.yml](/.github/workflows/windows.yml) 中配置实际桌面检查与构建，产物为 NSIS exe。workflow_dispatch 或推送可触发；该 workflow 尚未执行，源码 ZIP 不是安装包。

## 必须原生验证的项目

| 操作 | 验收标准 |
| --- | --- |
| 从托盘、自启动方式、不同启动目录打开 | 同一动作解析出相同 cwd；启动器目录不参与 |
| 双屏，左侧屏负坐标，100%/150%/200% DPI | 拖动后面板贴近气泡，菜单与结果不超工作区 |
| 跨屏拖动、改变显示器缩放、拔掉显示器后重启 | 气泡可恢复到有效位置；面板不跑到屏幕外 |
| 面板打开时点击其他软件 | 保持展开；焦点留在其他软件 |
| 多行、大量动作、长输出 | 输入/菜单/结果分别滚动，发送和收起可访问 |
| 脚本产生中文、多段输出、stderr 与子进程 | 中文不破碎、增量及时、取消清理子进程树 |
| Pi print、Python、Node、PowerShell 与 npm .cmd shim | 实际程序可启动；执行无额外控制台窗口 |
| detach 后另发任务、后台完成 | 原任务继续，新任务独立；完成不抢焦点 |
| 发任务后立即 Esc，等待成功或失败 | 运行中不提示数量；后台终态出现未读提示，隐藏订阅不清除 |
| 打开任务列表、查看运行中任务、查看最终结果 | 列表不清除，运行中不提前清除；查看最终结果才清除，重启不重复提示 |
| 前台任务完成，或运行时切到其他软件 | 前台看到终态不提醒；失去焦点后完成保留未读，重新查看才清除 |
| 找不到程序、cwd 无效、脚本非零退出、HTTP 失败 | 都能定位失败步骤；实际参数和冻结源码对应当次运行，鉴权头不显示明文 |
| 运行后编辑配置和源码，查看旧任务 | trace 不变；旧历史无 trace 时明确说明；详情在小窗口可滚动 |
| 安装 NSIS 后发送后台任务 | 通知名称/图标正确，任务结果可从任务入口查看 |
| 网络写入超时或取消 | 显示未确认提交，不自动重试 |

[通知插件](https://v2.tauri.app/plugin/notification/) 的 Windows 通知要求安装应用。Windows Job Object 与 npm shim 引导行为不能由 Linux 核心测试替代。

自动化证据以 [validation.md](validation.md) 为准；未执行的项目保持“未验证”。
