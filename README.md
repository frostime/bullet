# bullet

一个 Windows 优先的轻量文本动作入口。点击置顶气泡，输入文本，发送给配置好的 CLI、HTTP 请求或托管脚本。Svelte 5 + Tauri 2，瑞士国际主义风格。

## 快速开始

开发需要 Node.js 22.12+、Rust，以及 [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)（Visual Studio C++ Build Tools / Windows SDK / WebView2）。

```powershell
npm ci
npm run desktop
```

构建安装包：

```powershell
.\scripts\build-windows.ps1
```

结果在 `src-tauri/target/release/bundle/nsis/`。ZIP 提供源码与浏览器预览，不是已原生验收的 Windows 安装包。

只看 UI：`npm run dev`，打开终端显示的 localhost 地址。浏览器显著标注模拟模式，不执行本机 CLI 或真实请求。完整桌面功能必须使用 `npm run desktop`。

## 使用

- 点击气泡展开/收起，拖动移动。
- Enter 换行；Ctrl+Enter 发送；Esc 收起。
- 发送旁的动作入口选择已配置动作。
- “后台运行”分离当前任务，可继续发送另一条。
- 任务页查看后台进度和最近结果；完成后可复制。
- 设置页编辑动作、cwd、参数、HTTP 请求、脚本源码；完整 JSON 支持高级字段。

配置在 Windows 用户主目录的 `.config/bullet/config.json`，例如 `C:\Users\you\.config\bullet\config.json`。默认工作目录为用户主目录，每个动作可以覆盖。程序通过 PATH 或完整路径定位。默认的整理文本动作需要 Python。

Windows 的运行程序可以直接填写 `pi`、`uv`、`python` 等名称。bullet 按有效 PATH 和 PATHEXT 的 .com/.exe/.bat/.cmd 项查找，包括 npm 的 `pi.cmd` 入口，无需在配置中填写绝对路径。动作环境变量可以覆盖 PATH/PATHEXT；PATH 覆盖需要提供完整搜索路径。修改系统 PATH 后重启 bullet。终端中的 PowerShell alias/函数不会自动加载；程序参数单独填写在参数数组中。

托管脚本在 `.config/bullet/scripts/<action-id>/main.<extension>`，运行时使用不可变版本。源码编辑由 bullet 管理；不需要把脚本散落到各个工作目录。

## 配置示例

[examples/actions.json](examples/actions.json) 提供翻译（非流式/流式）、思源写入、pi print 和 uv 托管脚本动作。将所需动作复制到配置 `actions` 中，补齐地址、模型、文档 ID、提示词路径和运行环境后启用。示例默认禁用。

输入模板只有 `{{input}}`；Script 参数中独立的 `{{script}}` 由 bullet 替换为版本文件绝对路径。HTTP 结果路径使用 JSON Pointer。请求头可通过 `${env:NAME}` 引用环境变量，修改环境变量后重启 bullet。

运行中的任务在应用内存中；退出应用会停止它们。重启仅恢复最近完成的结果。写入任务失败或取消时不会自动重试。

## 开发

```powershell
npm run check
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
```

浏览器交互回归：先执行 `npx playwright install chromium`，再执行 `npm run test:ui`。该脚本检查多行输入、发送、detach、并发、脚本编辑、任务切换、收起复位与滚动，截图写入 `screenshots/`。它验证模拟界面，不替代 Windows 原生验收。Linux 子进程集成测试需要 `python3`。

文档入口： [.dev/docs/index.md](.dev/docs/index.md)。验证记录： [.dev/docs/validation.md](.dev/docs/validation.md)。仓库维护入口：[AGENTS.md](AGENTS.md)。
