# 执行契约

- 任务存在于 Rust，收起/detach/管理窗口切换不得取消任务。每个任务有独立 runId、cwd、输入和提交时配置。
- 并行任务上限 16；达到上限拒绝提交，界面保留输入。没有等待队列。
- 状态只能 running → succeeded/failed/cancelled。重复停止终态任务为无操作；失败任务不得自动重试。
- 成功以 CLI 的零退出码、HTTP 2xx 和配置的业务条件共同判定；stderr 非空不等于失败。
- stdout 与 stderr 同时读取，不能先写完 stdin 再排空输出。按块读取且增量解码，保留跨块字符；底层脚本需要主动 flush 才能实时输出。
- 超过配置的结果限制或固定日志限制时继续排空，设置 truncated。完整 JSON 解析另限 8 MiB，单个 SSE/JSONL 记录限 1 MiB。
- SSE 只在空行后派发事件，合并多条 data，兼容 CR/LF/CRLF；网络 chunk 不等于记录。配置 doneMarker 时缺少标记须失败并保留部分结果。
- buffered 不改变执行、读取或持久化，只延后 UI 呈现。失败/取消也允许查看部分结果。
- stdout、stderr 是两条独立管道，不承诺真实跨管道时间顺序。
- Linux 进程组与 Windows Job Object 负责子进程树停止。Windows 创建后立即分配 Job，极短的分配前竞态须在原生测试中关注；无法分配时拒绝继续执行。
- 取消/超时必须先 drop runner 再发布终态。退出应用不支持跨进程续跑。
- 通知不是成功判定条件；通知或历史保存错误不得把已执行成功的外部操作重跑。
- 持久化只包含完成任务，按 historyLimit 保留。重启不恢复执行中的任务，原来的输入不会自动重发。
- 接受的动作先分配 runId 再解析产物；cwd/程序/冻结脚本/参数插值失败都发布为真实失败的 run。输入超大、动作不存在或禁用、容量拒绝仍是启动错误。
- 每次运行在快照内携带追加式 trace（`TraceEntry { at, step, detail }`）：配置版本、cwd、解析出的程序、原始/插值参数数组、脚本冻结版本与有界源码、输入模式、配置的 env 覆盖、spawn/PID/退出；HTTP 方法/URL、渲染 body、脱敏头摘要、响应状态、解析与条件结果；终态（含取消、超时、提交状态不明）必有 terminal 记录。
- 脱敏策略在 `trace.rs`：敏感名（auth/token/api-key/secret/password/cookie/credential）与任何 `${env:}` 展开的头值全部隐藏；env 只记录用户配置的覆盖，不枚举继承环境；解析后的密钥不落 trace、不落历史。不逐行记录 stdout/stderr 与脚本内部行。
- trace 总字节有界（含终态预留），已知偏大内容就地 UTF-8 安全截断；触界跳过普通记录并置 traceTruncated，执行与管道排空不中断。trace 经快照与 Update 增量（traceDelta）发送，重订阅重设增量偏移，无独立 IPC。
- 未读只在成功/失败终态置真；取消不提醒；subscribe 不隐式清除未读。mark_read 只对终态任务生效，并把已读写回历史持久化，重启不复活未读；持久化失败时返回错误并恢复未读，可显式重试。
