---
title: 交付验证记录
description: 列出本次交付已获得的证据，以及仍需要 Windows 原生验收的行为。
scope:
  - /src/**
  - /src-tauri/**
  - /scripts/**
updated: 2026-10-09
---

# 已执行的验证

本次环境为 Linux，Node.js 24.19、Rust 1.99。没有运行 Windows 应用或生成 NSIS 安装包。

CLI 查找补丁重新执行核心测试和 Rust 格式检查；前端文件未修改，前端检查沿用原交付记录。Windows 路径模块单独进行了目标类型检查，未重跑完整桌面目标。

| 检查 | 结果 | 能确认的范围 |
| --- | --- | --- |
| `npm run check` | 0 errors、0 warnings | Svelte 和 TypeScript 类型、模板检查 |
| `npm test` | 2 tests passed | IPC 序号缺口、迟到更新与不同 runId 的处理 |
| `npm run build` | 通过 | 前端生产构建、打包资源 |
| `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --locked` | 13 tests passed | 原有 9 项及新增 cmd/npm shim 查找、PATH/PATHEXT 优先级、文件排除、Unix 名称保留 |
| `rustc --edition=2021 --crate-type lib --target x86_64-pc-windows-gnu --emit metadata src-tauri/src/config/paths.rs` | 通过 | 本补丁 Windows 路径模块的目标类型检查；未运行 Windows 程序 |
| `cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-gnu --locked` | 原交付版本通过；本补丁未重跑 | 原交付桌面目标证据，不替代补丁后的完整 Windows 构建 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | 通过 | Rust 格式 |
| `npm run test:ui` | 通过 | Chromium 中的模拟界面交互，无页面异常 |

Rust 集成测试在 Linux 启动真实 Python 子进程，检查 stdin EOF、配置 cwd、并行执行、detach 后完成、历史持久化与取消后保留部分输出。SSE 测试使用本地 TCP HTTP 服务，将中文和事件拆成小块后验证提取结果。测试不调用真实 AI 或思源服务，不确认 Windows 子进程树清理。

新增 [/src-tauri/tests/windows_cli.rs](/src-tauri/tests/windows_cli.rs) 的 3 项测试仅在 Windows 启用，本次未运行。测试主体另用 Linux 核心依赖进行了类型检查；这不能确认 PowerShell、cmd/bat 或 Windows Job Object 的运行行为。原 Windows CI 的 cargo test 命令会自动包含这些测试。

浏览器回归覆盖 Enter 换行、Ctrl+Enter 发送、detach 后另发任务、查看结果、脚本保存与重新载入、任务路由、完成后收起再展开空输入、30 个动作的菜单滚动、长结果滚动、较小管理窗口的滚动。执行由 browser bridge 模拟，不能证明原生窗口尺寸或真实 CLI/HTTP 行为。

截图位于 [/screenshots/](/screenshots/)。中文截图使用测试环境可用的字体；Windows 界面使用系统无衬线字体。

# 尚未验证

Windows MSVC 构建与 NSIS 打包、安装后系统通知、托盘点击、多显示器与 DPI、中文输入法、真实 PowerShell/Python/Node/PI/uv 以及 Windows Job Object 的子进程取消。完整验收步骤见 [windows-validation.md](windows-validation.md)。

[/.github/workflows/windows.yml](/.github/workflows/windows.yml) 和 [/scripts/build-windows.ps1](/scripts/build-windows.ps1) 已提供 Windows 构建路径；本次未执行该 CI，不能把 workflow 配置视为构建通过证据。
