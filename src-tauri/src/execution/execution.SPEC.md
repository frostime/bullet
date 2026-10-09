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
