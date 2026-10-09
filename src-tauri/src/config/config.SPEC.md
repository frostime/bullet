# 配置、路径与托管脚本契约

- 配置目录是用户主目录下 `.config/bullet`，Windows 使用原生用户主目录，不使用 AppData 或 WSL HOME。
- config.json 中的 `version` 是格式版本，当前为 1。类型和字段以 [model.rs](model.rs) 为准；未知字段必须报错，避免静默误配置。
- 首次运行写入默认配置和文本整理 Python 脚本。无效的初始配置阻止启动，不覆盖用户文件；运行中的无效外部修改保留最后有效配置并返回 warning。
- 每次打开面板、载入配置和启动任务都重新检查文件。没有后台文件监视器，手工修改不是即时推送。
- 保存必须比较调用方 revision 与当前文件内容哈希；冲突不能无条件覆盖。原子替换保证本进程写入不暴露半个 JSON；无法消除不遵守协议的外部编辑器在检查和替换之间的竞态。
- Script 源码在 scripts/<actionId>/main.<extension>，ID 与扩展名经过路径安全校验。动作改名不改路径，移除动作保留源码。
- 执行使用 scripts/<actionId>/versions/<hash>/main.<extension> 的不可变版本。`__file__`/模块位置在版本目录；cwd 与此无关。单文件脚本不自动复制相邻资产。版本保留，不自动删除正在或曾经运行的源码。
- 脚本源和动作配置分别保存，各自有 revision；两者不是跨文件事务。试运行先保存配置和脚本再执行，错误必须可见。
- defaultCwd 为绝对路径或 ~/...。动作 cwd 空值继承默认值，动作相对 cwd 以 defaultCwd 为基准，~/... 以用户主目录为基准。
- 目录不存在时拒绝启动，保留用户输入；不创建目录、不回退到其他目录、不修改全局 cwd。
- 程序名允许直接配置 pi、uv、python。Unix 的裸名称由 Command 使用子进程 PATH 查找；Windows 按有效 PATH 的目录顺序和 PATHEXT 中 .com/.exe/.bat/.cmd 的顺序定位入口，缺少 PATHEXT 时使用该默认列表。动作 env 中的 PATH/PATHEXT 覆盖应用继承值，Windows 变量名不区分大小写。
- Windows 不隐式搜索启动器目录或当前工作目录。相对 PATH 项以动作 cwd 为基准，空项跳过；驱动器相对项如 C:bin 不参与查找。带路径的程序相对 cwd 解析，驱动器相对程序路径 C:foo 必须拒绝。已指定扩展名的存在文件优先使用原路径。
- npm 的无扩展名 Unix shim 不作为 Windows 裸命令入口，查找 pi 可以命中 pi.cmd。启动时取得的绝对路径仅属于本次执行，不回写用户配置。修改系统 PATH 后需要重启常驻应用；不载入 PowerShell profile、alias 或函数。
- 普通参数按数组传给 Command；Windows .cmd/.bat 由 Rust 标准库处理批处理转义，不自行拼接 cmd /c 命令字符串。仅参数输入模式替换 {{input}}；Script 的独立 {{script}} 参数替换为源码快照绝对路径。
- stdin 输入完成后必须关闭，包括空输入。脚本是配置好的代码，用户输入是数据。
- HTTP JSON 中的 {{input}} 只替换字符串值再序列化；环境变量引用 ${env:NAME} 只用于 HTTP 请求头。不会在任意字段执行模板代码。
