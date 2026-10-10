---
title: 交付验证记录
description: 列出本次交付已获得的证据，以及仍需要 Windows 原生验收的行为。
scope:
  - /src/**
  - /src-tauri/**
  - /scripts/**
updated: 2026-10-11
---

# 运行 trace 与结果未读验证（2026-10-11）

本轮环境仍为 Windows/MSYS（Node.js 24.14、Rust 1.95），Edge 无头交互。使用独立临时配置、冻结源码和本机回环 HTTP 服务；没有访问个人配置、鉴权或外部服务。

| 检查 | 结果 | 能确认的范围 |
| --- | --- | --- |
| `npm run check` | 0 errors、0 warnings | 类型与 Svelte 模板 |
| `npm test` | 16 tests passed | 既有配置会话、任务增量，以及 trace 顺序/重复/旧快照和未读终态规则 |
| `npm run test:ui` | 三个脚本通过 | 既有界面、配置恢复模拟 IPC；新 trace 展示、源码/参数/截断、隐藏订阅不确认、列表不算已读、前台确认、迟到确认不影响另一任务、模拟版本不泄露完整配置 |
| `npm run build` | 通过 | 前端生产构建 |
| `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` | 43 tests passed | 同步冻结与首个快照、准备失败历史、实际插值、HTTP 头脱敏、容量与 UTF-8、旧历史、只读/占用下已读失败及重试、原配置/执行测试 |
| `cargo check --manifest-path src-tauri/Cargo.toml` | 通过 | Windows 桌面入口、Channel、显式已读 IPC 的编译检查 |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` / `git diff --check` | 通过 | 格式与空白；被修改的执行文件同步格式化 |

前端与后端分别经过独立只读审查。发现的迟到已读响应、浏览器版本明文、准备阶段冻结时序、遗漏 cwd、截断标记及已读保存错误均已修复并补测试。浏览器原生 IPC 案例仍是模拟，不确认真实 WebView 隐藏/焦点事件；本轮 Rust 测试实际运行隔离的 cmd/PowerShell 子进程与回环 HTTP，但不代替安装后通知、进程树和窗口的手工验收。原 execution 测试的 Windows unused-import warning 保留。

Trace 示例截图位于忽略的 `tmp/run-trace/failed-trace.png`，使用合成数据。原生 Esc/焦点/重启验收步骤见 [windows-validation.md](windows-validation.md)。

# 配置编辑重构验证（2026-10-11，历史记录）

本轮环境为 Windows/MSYS，Node.js 24.14、Rust 1.95，浏览器交互使用本机 Edge 的无头模式。没有启动读取个人配置的桌面应用，没有生成安装包。

| 检查 | 结果 | 能确认的范围 |
| --- | --- | --- |
| `npm run check` | 0 errors、0 warnings | 前端类型和 Svelte 模板 |
| `npm test` | 11 tests passed | 编辑会话的 JSON/表单转换、草稿按动作与扩展名保留、重新载入取消与失败、不可用快照、异步串行、显式恢复及部分提交；原任务输出测试仍通过 |
| `npm run build` | 通过 | 前端生产构建 |
| `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features` | 32 tests passed | 配置恢复备份与冲突、校验与 cwd 一致性、Windows 只读/文件占用替换失败、原执行和 Windows CLI 测试 |
| `cargo check --manifest-path src-tauri/Cargo.toml` | 通过 | Windows 默认桌面 feature、新命令与权限配置编译；不代表运行验收 |
| `npm run test:ui` | 通过 | 原交互回归及配置草稿、JSON 覆盖、丢弃确认；模拟 IPC 下恢复参数、输入锁定及部分提交错误展示 |

变更涉及的 Rust 文件通过定向 rustfmt 检查。全仓库 `cargo fmt --all -- --check` 仍报告此前 `execution/mod.rs` 的格式差异，本轮未修改该执行模块。

新 Rust 文件错误测试只使用独立临时目录。模拟 IPC 的测试使用真实组件和 bridge，但不调用 Rust、系统弹窗或真实任务；不将它视为原生 WebView 验收。UI 截图与日志在忽略的 `tmp/` 下。现有 execution 测试在 Windows 有 unused-import warning，本轮未改动无关代码。

仍需人工验证原生管理窗口、系统确认弹窗、用户现场文件占用或权限，以及安装包。没有自动解除只读属性、提高权限或重试写入。配置 version 1 和脚本路径保持不变。

# 原交付与 CLI 查找补丁验证（历史记录）

以下是此前 Linux 环境的记录，不能替代上面的本轮结果。环境为 Linux，Node.js 24.19、Rust 1.99；未运行 Windows 应用或生成 NSIS 安装包。

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
