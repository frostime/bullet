pub mod http;
pub mod output;
pub mod process;
pub mod trace;

pub use trace::TraceEntry;

use crate::config::{
    atomic_write,
    model::{Action, ActionKind, NotificationMode, OutputMode},
    paths, ConfigStore,
};
use output::append_bounded;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

pub const MAX_ACTIVE_RUNS: usize = 16;
const MAX_INPUT_BYTES: usize = 1_048_576;
const MAX_LOG_BYTES: usize = 262_144;

const INPUT_PLACEHOLDER_PREFIX: &str = "{{input";
const INPUT_PLACEHOLDER: &str = "{{input}}";
const INPUT_INLINE_PLACEHOLDER: &str = "{{input-inline}}";

/// Substitute input placeholders in one scan; inserted text is never rescanned,
/// so input containing literal placeholder text cannot recurse.
/// `{{input}}` inserts the raw text; `{{input-inline}}` inserts its JSON string
/// body without quote marks, intended for a string that is itself JSON.
pub fn substitute_input(text: &str, input: &str) -> String {
    if !text.contains(INPUT_PLACEHOLDER_PREFIX) {
        return text.to_owned();
    }
    let inline = escape_json_inline(input);
    let mut result = String::with_capacity(text.len() + input.len() * 4);
    let mut rest = text;
    while let Some((start, token)) = next_input_placeholder(rest) {
        result.push_str(&rest[..start]);
        if token == INPUT_PLACEHOLDER {
            result.push_str(input);
        } else {
            result.push_str(&inline);
        }
        rest = &rest[start + token.len()..];
    }
    result.push_str(rest);
    result
}

/// Input text as a JSON string body without the surrounding quotes.
fn escape_json_inline(input: &str) -> String {
    let mut encoded = serde_json::to_string(input).expect("字符串总能序列化为 JSON");
    // serde_json 的首尾引号均为 ASCII 字节，remove 不会切开多字符边界
    encoded.remove(encoded.len() - 1);
    encoded.remove(0);
    encoded
}

/// Scan the shared prefix so an absent token does not trigger repeated searches
/// of the entire remaining template.
fn next_input_placeholder(text: &str) -> Option<(usize, &'static str)> {
    let mut offset = 0;
    while let Some(start) = text[offset..].find(INPUT_PLACEHOLDER_PREFIX) {
        let start = offset + start;
        let candidate = &text[start..];
        if candidate.starts_with(INPUT_PLACEHOLDER) {
            return Some((start, INPUT_PLACEHOLDER));
        }
        if candidate.starts_with(INPUT_INLINE_PLACEHOLDER) {
            return Some((start, INPUT_INLINE_PLACEHOLDER));
        }
        offset = start + INPUT_PLACEHOLDER_PREFIX.len();
    }
    None
}

#[cfg(test)]
mod input_substitution_tests {
    use super::*;

    #[test]
    fn plain_placeholder_repeats_for_every_occurrence() {
        assert_eq!(substitute_input("[{{input}}]", "abc"), "[abc]");
        assert_eq!(substitute_input("a{{input}}b{{input}}c", "x"), "axbxc");
        assert_eq!(substitute_input("untouched", "x"), "untouched");
    }

    #[test]
    fn inline_placeholder_uses_json_string_escaping() {
        assert_eq!(
            substitute_input("{{input-inline}}", "a\"b\nc"),
            "a\\\"b\\nc"
        );
        assert_eq!(substitute_input("{{input-inline}}", ""), "");
        assert_eq!(substitute_input("{{input-inline}}", "中文"), "中文");
    }

    #[test]
    fn mixed_placeholders_keep_their_own_forms() {
        assert_eq!(
            substitute_input("{{input}}//{{input-inline}}", "a\""),
            "a\"//a\\\""
        );
    }

    #[test]
    fn input_content_is_never_rescanned() {
        assert_eq!(
            substitute_input("{{input}}", "{{input-inline}}"),
            "{{input-inline}}"
        );
        assert_eq!(
            substitute_input("{{input-inline}}", "x{{input}}"),
            "x{{input}}"
        );
    }

    #[test]
    fn unrecognized_prefixes_do_not_hide_later_placeholders() {
        assert_eq!(
            substitute_input(
                "中文{{input-x}}/{{input{{input-inline}}/{{input}}/{{input",
                "a\"b"
            ),
            "中文{{input-x}}/{{inputa\\\"b/a\"b/{{input"
        );
    }

    #[test]
    fn partial_brace_sequences_stay_untouched() {
        assert_eq!(
            substitute_input("{{input-inl}} {{input-x}}", "y"),
            "{{input-inl}} {{input-x}}"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSnapshot {
    pub id: String,
    pub action_id: String,
    pub action_name: String,
    pub input: String,
    pub status: RunStatus,
    pub stage: String,
    pub stdout: String,
    pub stderr: String,
    pub cwd: Option<String>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
    pub http_status: Option<u16>,
    pub truncated: bool,
    pub submission_unknown: bool,
    pub unread: bool,
    pub output_mode: OutputMode,
    pub config_revision: String,
    pub script_revision: Option<String>,
    pub seq: u64,
    /// 当次执行的步骤 trace；旧历史记录反序列化时缺省为空。只增不改。
    #[serde(default)]
    pub trace: Vec<TraceEntry>,
    /// trace 触界后跳过过记录；执行与输出排空不受影响。
    #[serde(default)]
    pub trace_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: String,
    pub action_name: String,
    pub status: RunStatus,
    pub stage: String,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub unread: bool,
    pub preview: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RunEvent {
    Snapshot {
        run: RunSnapshot,
    },
    #[serde(rename_all = "camelCase")]
    Update {
        run_id: String,
        seq: u64,
        stdout_delta: String,
        stderr_delta: String,
        status: RunStatus,
        stage: String,
        error: Option<String>,
        exit_code: Option<i32>,
        http_status: Option<u16>,
        finished_at: Option<u64>,
        truncated: bool,
        submission_unknown: bool,
        trace_delta: Vec<TraceEntry>,
        trace_truncated: bool,
    },
}

pub type EventSink = Arc<dyn Fn(RunEvent) -> bool + Send + Sync>;
pub type CompletionSink = Arc<dyn Fn(RunSnapshot, NotificationMode) + Send + Sync>;

struct RunEntry {
    snapshot: RunSnapshot,
    cancel: CancellationToken,
    subscriber: Option<EventSink>,
    sent_stdout: usize,
    sent_stderr: usize,
    sent_trace: usize,
    dirty: bool,
}

pub struct RunManager {
    entries: Mutex<HashMap<String, RunEntry>>,
    foreground: Mutex<Option<String>>,
    config: Arc<ConfigStore>,
    history_write: Mutex<()>,
    completed: CompletionSink,
}

pub struct PreparedAction {
    pub action: Action,
    pub input: String,
    pub cwd: Option<PathBuf>,
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug)]
pub struct ExecutionFailure {
    pub message: String,
    pub submission_unknown: bool,
}
impl From<String> for ExecutionFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            submission_unknown: false,
        }
    }
}

/// 逐项解析产物并写入 trace；任何一步失败都以记录 + 错误返回，成为失败终态。
/// 不在这里触碰应用全局目录，也不拼接 shell 字符串。
fn prepare(
    manager: &Arc<RunManager>,
    id: &str,
    default_cwd: &str,
    action: &Action,
    input: &str,
) -> Result<PreparedAction, ExecutionFailure> {
    if action.kind == ActionKind::Http {
        return Ok(PreparedAction {
            action: action.clone(),
            input: input.into(),
            cwd: None,
            program: String::new(),
            args: Vec::new(),
        });
    }
    let cwd = match paths::resolve_cwd(default_cwd, action.cwd.as_deref(), &manager.config.home) {
        Ok(cwd) => {
            manager.trace(id, "cwd", format!("解析为 {}", cwd.display()));
            // start 未返回前就把 cwd 写进快照，首次订阅的快照即拿到完整元数据。
            if let Some(entry) = manager.entries.lock().unwrap().get_mut(id) {
                entry.snapshot.cwd = Some(cwd.to_string_lossy().into_owned());
            }
            cwd
        }
        Err(error) => {
            manager.trace(id, "cwd", format!("解析失败：{error}"));
            return Err(error.into());
        }
    };
    let program =
        match paths::resolve_program(&action.program, &cwd, &manager.config.home, &action.env) {
            Ok(program) => {
                manager.trace(id, "program", format!("解析为 {program}"));
                program
            }
            Err(error) => {
                manager.trace(id, "program", format!("解析失败：{error}"));
                return Err(error.into());
            }
        };
    let script = match action.kind {
        ActionKind::Script => match manager
            .config
            .freeze_script(&action.id, &action.script_extension)
        {
            Ok(path) => {
                // 记录磁盘上的冻结版本，不是编辑器里的最新稿。
                let source = std::fs::read_to_string(&path)
                    .map_err(|error| format!("动作 {}：冻结脚本不可读：{error}", action.id))?;
                let version = path
                    .parent()
                    .and_then(|directory| directory.file_name())
                    .map(|name| name.to_string_lossy().into_owned());
                manager.trace_bounded(
                    id,
                    "script",
                    &format!(
                        "版本 {} · 路径 {}\n{}",
                        version.as_deref().unwrap_or("未知"),
                        path.display(),
                        source
                    ),
                    trace::SCRIPT_SOURCE_LIMIT,
                );
                if let Some(entry) = manager.entries.lock().unwrap().get_mut(id) {
                    entry.snapshot.script_revision = version;
                }
                Some(path)
            }
            Err(error) => {
                manager.trace(id, "script", format!("冻结失败：{error}"));
                return Err(error.into());
            }
        },
        _ => None,
    };
    let args: Vec<String> = action
        .args
        .iter()
        .map(|arg| {
            if arg == "{{script}}" {
                script
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else if action.input == crate::config::model::InputMode::Args {
                substitute_input(arg, input)
            } else {
                arg.clone()
            }
        })
        .collect();
    manager.trace_bounded(
        id,
        "args",
        &format!(
            "原始 {} · 插值 {}",
            serde_json::to_string(&action.args).unwrap_or_default(),
            serde_json::to_string(&args).unwrap_or_default()
        ),
        trace::ARGS_LIMIT,
    );
    if action.env.is_empty() {
        manager.trace(id, "env", "未配置环境变量覆盖".into());
    } else {
        let overrides: Vec<(&str, &str)> = action
            .env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        manager.trace(id, "env", trace::env_summary(&overrides));
    }
    Ok(PreparedAction {
        action: action.clone(),
        input: input.into(),
        cwd: Some(cwd),
        program,
        args,
    })
}

pub struct ExecutionResult {
    pub exit_code: Option<i32>,
    pub http_status: Option<u16>,
}

impl RunManager {
    pub fn new(config: Arc<ConfigStore>, completed: CompletionSink) -> Arc<Self> {
        let mut entries = HashMap::new();
        if let Ok(bytes) =
            crate::config::read_bounded(&config.root.join("history.json"), 128 * 1024 * 1024)
        {
            if let Ok(history) = serde_json::from_slice::<Vec<RunSnapshot>>(&bytes) {
                for snapshot in history
                    .into_iter()
                    .filter(|run| run.status != RunStatus::Running)
                    .take(config.load().config.settings.history_limit)
                {
                    entries.insert(
                        snapshot.id.clone(),
                        RunEntry {
                            snapshot,
                            cancel: CancellationToken::new(),
                            subscriber: None,
                            sent_stdout: 0,
                            sent_stderr: 0,
                            sent_trace: 0,
                            dirty: false,
                        },
                    );
                }
            }
        }
        Arc::new(Self {
            entries: Mutex::new(entries),
            foreground: Mutex::new(None),
            config,
            history_write: Mutex::new(()),
            completed,
        })
    }

    pub fn start(self: &Arc<Self>, action_id: &str, input: String) -> Result<String, String> {
        if input.len() > MAX_INPUT_BYTES {
            return Err("输入超过 1 MiB".into());
        }
        let config = self.config.load();
        let action = config
            .config
            .actions
            .iter()
            .find(|action| action.id == action_id && action.enabled)
            .cloned()
            .ok_or("动作不存在或已禁用")?;
        let may_write = action.http.as_ref().is_some_and(|http| {
            !matches!(http.method.to_ascii_uppercase().as_str(), "GET" | "HEAD")
        });
        let id = uuid::Uuid::new_v4().to_string();
        let mut snapshot = RunSnapshot {
            id: id.clone(),
            action_id: action.id.clone(),
            action_name: action.name.clone(),
            input: input.clone(),
            status: RunStatus::Running,
            stage: "准备执行".into(),
            stdout: String::new(),
            stderr: String::new(),
            // cwd/程序/脚本版本在 start 返回前同步解析（见下方 prepare），
            // 首次订阅快照就能拿到完整元数据；scripts/config 慢些编辑不丢失冻结版本。
            cwd: None,
            started_at: now_ms(),
            finished_at: None,
            error: None,
            exit_code: None,
            http_status: None,
            truncated: false,
            submission_unknown: false,
            unread: false,
            output_mode: action.output.mode,
            config_revision: config.revision.clone(),
            script_revision: None,
            seq: 0,
            trace: Vec::new(),
            trace_truncated: false,
        };
        // 接受即占位：产物解析失败也必须落在一次真实失败的 run 里，而不是丢失提交。
        trace::record(
            &mut snapshot,
            "accepted",
            format!(
                "配置版本 {} · 动作 {}（{:?}）· 输入模式 {:?} · 输入 {} 字节",
                config.revision,
                action.id,
                action.kind,
                action.input,
                input.len()
            ),
        );
        let cancel = CancellationToken::new();
        {
            let mut entries = self.entries.lock().unwrap();
            if entries
                .values()
                .filter(|entry| entry.snapshot.status == RunStatus::Running)
                .count()
                >= MAX_ACTIVE_RUNS
            {
                return Err("已有 16 个运行中任务，请先停止或等待完成".into());
            }
            entries.insert(
                id.clone(),
                RunEntry {
                    snapshot,
                    cancel: cancel.clone(),
                    subscriber: None,
                    sent_stdout: 0,
                    sent_stderr: 0,
                    sent_trace: 0,
                    dirty: false,
                },
            );
        }
        *self.foreground.lock().unwrap() = Some(id.clone());
        // 解析同步进行：冻结脚本、cwd、脚本版本在 start 返回前定格，之后立即编辑也不会改变本次 run。
        // 准备失败仍通过 finish 异步发布，保持真实失败 run 的生命周期。
        let prepared = prepare(
            self,
            &id,
            &config.config.settings.default_cwd,
            &action,
            &input,
        );
        let manager = self.clone();
        let run_id = id.clone();
        tokio::spawn(async move {
            let prepared = match prepared {
                Ok(prepared) => prepared,
                Err(failure) => {
                    manager.finish(
                        &run_id,
                        RunStatus::Failed,
                        Err(failure),
                        config.config.settings.notifications,
                        config.config.settings.history_limit,
                    );
                    return;
                }
            };
            let execution = async {
                if action.kind == ActionKind::Http {
                    http::execute(
                        &prepared,
                        &manager,
                        &run_id,
                        config.config.settings.max_output_bytes,
                    )
                    .await
                } else {
                    process::execute(
                        &prepared,
                        &manager,
                        &run_id,
                        config.config.settings.max_output_bytes,
                    )
                    .await
                }
            };
            let mut execution = Box::pin(execution);
            let timeout = async {
                if let Some(seconds) = action.timeout_sec {
                    tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            tokio::pin!(timeout);
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
            let (status, result) = loop {
                tokio::select! {
                    biased;
                    outcome = &mut execution => {
                        let status = if outcome.is_ok() { RunStatus::Succeeded } else { RunStatus::Failed };
                        break (status, outcome);
                    }
                    _ = cancel.cancelled() => break (RunStatus::Cancelled, Err(ExecutionFailure {
                        message: "任务已停止；已发生的操作不会撤销".into(), submission_unknown: may_write && manager.was_submitted(&run_id),
                    })),
                    _ = &mut timeout => break (RunStatus::Failed, Err(ExecutionFailure {
                        message: "执行超时；未自动重试".into(), submission_unknown: may_write && manager.was_submitted(&run_id),
                    })),
                    _ = tick.tick() => manager.flush(&run_id),
                }
            };
            // Drop the runner before publishing completion; its process guard stops descendants.
            drop(execution);
            manager.finish(
                &run_id,
                status,
                result,
                config.config.settings.notifications,
                config.config.settings.history_limit,
            );
        });
        Ok(id)
    }

    pub fn subscribe(&self, id: &str, sink: EventSink) -> Result<(), String> {
        let mut entries = self.entries.lock().unwrap();
        let entry = entries.get_mut(id).ok_or("任务不存在")?;
        // 订阅只负责续传 delta 偏移，不得隐式清除未读：订阅可能发生在看不到结果的界面。
        // Snapshot and subscription share a lock: no increment can land between them.
        if !sink(RunEvent::Snapshot {
            run: entry.snapshot.clone(),
        }) {
            return Err("界面连接已关闭".into());
        }
        entry.sent_stdout = entry.snapshot.stdout.len();
        entry.sent_stderr = entry.snapshot.stderr.len();
        entry.sent_trace = entry.snapshot.trace.len();
        entry.subscriber = Some(sink);
        Ok(())
    }

    pub fn detach(&self, id: &str) {
        let mut foreground = self.foreground.lock().unwrap();
        if foreground.as_deref() == Some(id) {
            *foreground = None;
        }
        drop(foreground);
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            entry.subscriber = None;
        }
    }
    pub fn set_foreground(&self, id: Option<String>) {
        *self.foreground.lock().unwrap() = id;
    }
    pub fn foreground(&self) -> Option<String> {
        self.foreground.lock().unwrap().clone()
    }
    /// 只对终态任务生效的显式已读确认；持久化在 history.json，重启不复活未读。
    /// 不推送任何事件，也不打断序号：界面自己更新本地副本。
    /// 持久化失败返回错误并恢复未读，界面可以报告并显式重试。
    pub fn mark_read(&self, id: &str) -> Result<(), String> {
        let changed = {
            let mut entries = self.entries.lock().unwrap();
            let Some(entry) = entries.get_mut(id) else {
                return Ok(());
            };
            if entry.snapshot.status == RunStatus::Running || !entry.snapshot.unread {
                return Ok(());
            }
            entry.snapshot.unread = false;
            true
        };
        if !changed {
            return Ok(());
        }
        // persist_history 自己也要(entries)锁，必须在释放后再调用。
        let read_failed = self.persist_history();
        match read_failed {
            Ok(()) => Ok(()),
            Err(error) => {
                // 撤销内存中的已读，保留未读提醒；稍后可以再次显式确认。
                if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
                    entry.snapshot.unread = true;
                }
                Err(format!("已读确认未能保存：{error}"))
            }
        }
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let entries = self.entries.lock().unwrap();
        let entry = entries.get(id).ok_or("任务不存在")?;
        if entry.snapshot.status == RunStatus::Running {
            entry.cancel.cancel();
        }
        Ok(())
    }
    pub fn cancel_all(&self) {
        for entry in self.entries.lock().unwrap().values() {
            entry.cancel.cancel();
        }
    }
    pub fn list(&self) -> Vec<RunSummary> {
        let mut runs: Vec<_> = self
            .entries
            .lock()
            .unwrap()
            .values()
            .map(|entry| RunSummary {
                id: entry.snapshot.id.clone(),
                action_name: entry.snapshot.action_name.clone(),
                status: entry.snapshot.status,
                stage: entry.snapshot.stage.clone(),
                started_at: entry.snapshot.started_at,
                finished_at: entry.snapshot.finished_at,
                unread: entry.snapshot.unread,
                preview: entry.snapshot.input.chars().take(64).collect(),
            })
            .collect();
        runs.sort_by_key(|run| std::cmp::Reverse(run.started_at));
        runs
    }
    pub fn clear_history(&self) -> Result<(), String> {
        self.entries
            .lock()
            .unwrap()
            .retain(|_, entry| entry.snapshot.status == RunStatus::Running);
        self.persist_history()
    }
    pub fn stage(&self, id: &str, stage: &str) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            entry.snapshot.stage = stage.into();
            entry.dirty = true;
        }
    }
    /// 追加一条 trace 记录；触界跳过不报错，执行与排空不受影响。
    pub fn trace(&self, id: &str, step: &str, detail: String) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            trace::record(&mut entry.snapshot, step, detail);
            entry.dirty = true;
        }
    }
    /// 已知可能超限的文本：就地限幅后记录，截断反映到 traceTruncated。
    pub fn trace_bounded(&self, id: &str, step: &str, text: &str, limit: usize) {
        let (detail, clipped) = trace::cut_utf8(text, limit);
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            trace::record(&mut entry.snapshot, step, detail);
            entry.snapshot.trace_truncated |= clipped;
            entry.dirty = true;
        }
    }
    pub fn append(&self, id: &str, stderr: bool, text: &str, limit: usize) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            let clipped = if stderr {
                append_bounded(&mut entry.snapshot.stderr, text, MAX_LOG_BYTES)
            } else {
                append_bounded(&mut entry.snapshot.stdout, text, limit)
            };
            entry.snapshot.truncated |= clipped;
            entry.dirty = true;
        }
    }
    pub fn record_exit(&self, id: &str, code: Option<i32>) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            entry.snapshot.exit_code = code;
            let detail = code
                .map(|code| format!("退出码 {code}"))
                .unwrap_or_else(|| "被信号终止或被强制停止".into());
            trace::record(&mut entry.snapshot, "exit", detail);
        }
    }
    pub fn record_http_status(&self, id: &str, status: u16) {
        if let Some(entry) = self.entries.lock().unwrap().get_mut(id) {
            entry.snapshot.http_status = Some(status);
            trace::record(
                &mut entry.snapshot,
                "http_response",
                format!("HTTP {status}"),
            );
        }
    }
    fn was_submitted(&self, id: &str) -> bool {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|entry| entry.snapshot.stage != "准备执行")
    }
    fn flush(&self, id: &str) {
        let mut entries = self.entries.lock().unwrap();
        let Some(entry) = entries.get_mut(id) else {
            return;
        };
        if !entry.dirty {
            return;
        }
        entry.snapshot.seq += 1;
        entry.dirty = false;
        let event = RunEvent::Update {
            run_id: id.into(),
            seq: entry.snapshot.seq,
            stdout_delta: entry.snapshot.stdout[entry.sent_stdout..].into(),
            stderr_delta: entry.snapshot.stderr[entry.sent_stderr..].into(),
            status: entry.snapshot.status,
            stage: entry.snapshot.stage.clone(),
            error: entry.snapshot.error.clone(),
            exit_code: entry.snapshot.exit_code,
            http_status: entry.snapshot.http_status,
            finished_at: entry.snapshot.finished_at,
            truncated: entry.snapshot.truncated,
            submission_unknown: entry.snapshot.submission_unknown,
            trace_delta: entry.snapshot.trace[entry.sent_trace..].to_vec(),
            trace_truncated: entry.snapshot.trace_truncated,
        };
        entry.sent_stdout = entry.snapshot.stdout.len();
        entry.sent_stderr = entry.snapshot.stderr.len();
        entry.sent_trace = entry.snapshot.trace.len();
        if let Some(sink) = &entry.subscriber {
            if !sink(event) {
                entry.subscriber = None;
            }
        }
    }
    fn finish(
        &self,
        id: &str,
        status: RunStatus,
        result: Result<ExecutionResult, ExecutionFailure>,
        notifications: NotificationMode,
        limit: usize,
    ) {
        let snapshot = {
            let mut entries = self.entries.lock().unwrap();
            let entry = entries.get_mut(id).unwrap();
            entry.snapshot.status = status;
            entry.snapshot.finished_at = Some(now_ms());
            // 只有成功/失败终态点亮未读；取消不产生提醒，保持运行期间的假。
            entry.snapshot.unread = matches!(status, RunStatus::Succeeded | RunStatus::Failed);
            entry.snapshot.stage = match status {
                RunStatus::Succeeded => "已完成",
                RunStatus::Failed => "失败",
                RunStatus::Cancelled => "已停止",
                _ => "运行中",
            }
            .into();
            let label = match status {
                RunStatus::Succeeded => "完成",
                RunStatus::Failed => "失败",
                RunStatus::Cancelled => "已停止",
                RunStatus::Running => "运行中",
            };
            let terminal_core = match &result {
                Ok(result) => {
                    if let Some(code) = result.exit_code {
                        format!("退出码 {code}")
                    } else if let Some(status) = result.http_status {
                        format!("HTTP {status}")
                    } else {
                        "完成".into()
                    }
                }
                Err(error) => {
                    let mut detail = error.message.clone();
                    if error.submission_unknown {
                        detail.push_str("；提交状态不明");
                    }
                    detail
                }
            };
            match result {
                Ok(result) => {
                    entry.snapshot.exit_code = result.exit_code;
                    entry.snapshot.http_status = result.http_status;
                }
                Err(error) => {
                    entry.snapshot.error = Some(error.message);
                    entry.snapshot.submission_unknown = error.submission_unknown;
                }
            }
            // 终态记录使用预留空间，即使 trace 已触界也保留失败原因的位置。
            trace::record_terminal(
                &mut entry.snapshot,
                "terminal",
                format!("{label}：{terminal_core}"),
            );
            entry.dirty = true;
            entry.snapshot.clone()
        };
        self.flush(id);
        (self.completed)(snapshot, notifications);
        {
            let foreground = self.foreground();
            let mut entries = self.entries.lock().unwrap();
            let mut terminal: Vec<_> = entries
                .values()
                .filter(|entry| entry.snapshot.status != RunStatus::Running)
                .map(|entry| (entry.snapshot.started_at, entry.snapshot.id.clone()))
                .collect();
            terminal.sort_by_key(|item| std::cmp::Reverse(item.0));
            for (_, old_id) in terminal.into_iter().skip(limit) {
                if foreground.as_deref() != Some(&old_id) {
                    entries.remove(&old_id);
                }
            }
        }
        if let Err(error) = self.persist_history() {
            self.append(id, true, &format!("\n历史保存失败：{error}"), MAX_LOG_BYTES);
            self.flush(id);
        }
    }
    fn persist_history(&self) -> Result<(), String> {
        let _write = self.history_write.lock().unwrap();
        let mut history: Vec<_> = self
            .entries
            .lock()
            .unwrap()
            .values()
            .filter(|entry| entry.snapshot.status != RunStatus::Running)
            .map(|entry| entry.snapshot.clone())
            .collect();
        history.sort_by_key(|run| std::cmp::Reverse(run.started_at));
        history.truncate(self.config.load().config.settings.history_limit);
        atomic_write(
            &self.config.root.join("history.json"),
            &serde_json::to_vec(&history).map_err(|error| error.to_string())?,
        )
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
