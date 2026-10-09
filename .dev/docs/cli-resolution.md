---
title: CLI 名称、PATH 与工作目录
description: 解释 Windows 命令入口的查找契约，以及终端可运行而常驻应用找不到时的排查顺序。
scope:
  - /src-tauri/src/config/paths.rs
  - /src-tauri/src/execution/mod.rs
  - /src-tauri/src/execution/process.rs
  - /src-tauri/tests/windows_cli.rs
updated: 2026-10-09
---

# 配置与执行路径

用户配置的 `program` 可以是 pi、uv、python 等名称。`PreparedAction.program` 是提交时准备好的启动值，类型 String 不表示必须配置绝对路径。查找与路径语义由 [/src-tauri/src/config/paths.rs](/src-tauri/src/config/paths.rs) 的 `resolve_program` 拥有；[/src-tauri/src/execution/process.rs](/src-tauri/src/execution/process.rs) 负责参数、管道与进程生命周期。

| 配置值 | Windows 启动值如何取得 |
| --- | --- |
| `pi` | 在有效 PATH 中按目录顺序查找，再按 PATHEXT 的受支持扩展名顺序定位，如 `pi.cmd` |
| `pi.cmd` | 在有效 PATH 中查找该完整文件名 |
| `./tools/pi` | 以动作 cwd 定位，并尝试受支持的扩展名；不搜索 PATH |
| 绝对路径 | 在该位置定位；未写扩展名时尝试受支持扩展名 |

解析出的绝对路径不写回 config.json。cwd 只决定该任务的工作目录、显式相对程序路径和相对 PATH 项，不把裸名称直接拼到 cwd 下。空 PATH 项跳过，禁止依赖启动器目录或驱动器的隐式当前目录。

# Windows 查找契约

Rust [Command::new 文档](https://doc.rust-lang.org/std/process/struct.Command.html#method.new) 说明省略扩展名时只自动补 .exe；[npm bin 文档](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/#bin) 说明 Windows 会创建 .cmd 入口。因此仅调用 Command::new("pi") 无法覆盖常见 npm 安装方式。

bullet 先读取动作 env 中的 PATH/PATHEXT，未配置时读取应用继承的环境变量。Windows 变量名不区分大小写，查找使用与 Command::envs 相同的覆盖顺序。PATHEXT 只保留 .com/.exe/.bat/.cmd，忽略大小写并去重；未设置时默认 .COM;.EXE;.BAT;.CMD。不自动启用 .ps1/.py 等文件关联或 PowerShell profile。

每个目录内按扩展名顺序查找；第一个目录的 cmd 可以优先于后一个目录的 exe。npm 同时创建的无扩展名 Unix shim 不会遮住 Windows 的 cmd 入口。带扩展名且存在的完整路径优先原样使用；其他脚本类型需要显式选择解释器及其参数。

找到 cmd/bat 后继续交给 Command，并通过 args 数组传参，由 Rust 标准库适配批处理。不得把用户文本拼进原始 cmd /c 字符串。Windows 批处理对参数字符仍有自身限制；多行文本优先用 stdin，写完后关闭管道。

# 排查程序找不到

在 PowerShell 执行 `where.exe pi` 查看实际文件位置，再用 `Get-Command pi -All` 区分文件入口、alias 和函数。终端查找结果不能证明已经运行的 bullet 拥有相同 PATH。

如果只有终端可用，检查是否刚安装程序、终端 profile 修改了 PATH、应用从更新前的环境启动，或动作 env 覆盖了 PATH。重启 bullet 并确认动作 PATH 包含程序入口及它依赖的运行时目录。PATH 覆盖是完整替换，不自动追加系统 PATH。

# 验证边界

平台无关单元测试模拟 PATH 中的 cmd/npm shim、目录与扩展名优先级、已指定扩展名及目录排除。[/src-tauri/tests/windows_cli.rs](/src-tauri/tests/windows_cli.rs) 仅在 Windows 执行，使用临时 cmd/bat 入口和系统 PowerShell，覆盖动作 Path 覆盖、多行 UTF-8 stdin、参数边界和相对目录，不依赖真实 pi 或 npm 安装。运行结果以 [validation.md](validation.md) 为准。
