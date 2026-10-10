//! 当次运行的执行 trace：追加式、有界的步骤记录。
//! 本模块只承载两件本地策略：字节上限（UTF-8 安全截断）与敏感值脱敏；
//! 不做通用日志服务，也不记录脚本内部逐行输出。

use super::{now_ms, RunSnapshot};
use serde::{Deserialize, Serialize};

/// 单个 trace 记录的完好数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEntry {
    pub at: u64,
    pub step: String,
    pub detail: String,
}

/// 每次 run 的 trace 总字节预算（step + detail 的 UTF-8 字节数）。
const MAX_TRACE_BYTES: usize = 131_072;
/// 预算末尾为终态记录预留的空间，保证失败原因有位置可写。
const TERMINAL_RESERVE_BYTES: usize = 4_096;
/// 已知偏大的单条内容（脚本冻结源码、HTTP 请求体、参数数组）在写入前单独限幅。
pub const SCRIPT_SOURCE_LIMIT: usize = 8_192;
pub const HTTP_BODY_LIMIT: usize = 8_192;
pub const ARGS_LIMIT: usize = 8_192;
const TRUNCATION_SUFFIX: &str = "…（超出 trace 容量，已在此截断）";

/// 普通记录：超过“总预算 − 终态预留”后整条跳过并标记 truncated；
/// 后续读取与执行不受影响，runner 继续排空管道。
pub fn record(snapshot: &mut RunSnapshot, step: &str, detail: String) {
    if used_bytes(&snapshot.trace) + step.len() + detail.len()
        > MAX_TRACE_BYTES - TERMINAL_RESERVE_BYTES
    {
        snapshot.trace_truncated = true;
        return;
    }
    push(snapshot, step, detail);
}

/// 终态记录：动用预留空间，永远写入；detail 超出预留时 UTF-8 安全截断，
/// 记录后 trace 总字节（含终态）严格 ≤ MAX_TRACE_BYTES。
pub fn record_terminal(snapshot: &mut RunSnapshot, step: &str, detail: String) {
    let budget = TERMINAL_RESERVE_BYTES.saturating_sub(step.len() + 8);
    let (bounded, cut) = cut_utf8(&detail, budget);
    if cut {
        snapshot.trace_truncated = true;
    }
    push(snapshot, step, bounded);
}

fn push(snapshot: &mut RunSnapshot, step: &str, detail: String) {
    snapshot.trace.push(TraceEntry {
        at: now_ms(),
        step: step.into(),
        detail,
    });
}

fn used_bytes(entries: &[TraceEntry]) -> usize {
    entries
        .iter()
        .map(|entry| entry.step.len() + entry.detail.len())
        .sum()
}

/// UTF-8 安全截断，返回（结果，是否截断）。
/// 截断发生时输出总字节（含后缀）严格 ≤ limit；limit 装不下后缀时返回空串。
pub fn cut_utf8(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.into(), false);
    }
    let suffix_bytes = TRUNCATION_SUFFIX.len();
    if limit <= suffix_bytes {
        return (String::new(), true);
    }
    let mut end = limit - suffix_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}{}", &text[..end], TRUNCATION_SUFFIX), true)
}

/// 敏感名称判定：只看配置里写的名字，不扫描实际值。
const SENSITIVE_NAME_MARKS: [&str; 9] = [
    "auth",
    "token",
    "api-key",
    "apikey",
    "api_key",
    "secret",
    "password",
    "cookie",
    "credential",
];

pub fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_NAME_MARKS.iter().any(|mark| lower.contains(mark))
}

/// HTTP 头的 trace 摘要。敏感名一律隐藏；非敏感头中出现 `${env:` 引用时
/// 隐藏展开后的值——展开得到的密钥不会进入 trace 或任何持久化。
pub fn header_summary(name: &str, resolved: Option<&str>, template: &str) -> String {
    if is_sensitive_name(name) {
        format!("{name}: （已隐藏）")
    } else if template.contains("${env:") {
        format!("{name}: （环境变量值已隐藏）")
    } else {
        format!("{name}: {}", resolved.unwrap_or("（展开失败）"))
    }
}

/// 已配置环境变量覆盖的 trace 摘要：只覆盖用户配置的键，不枚举继承环境；
/// 敏感名隐藏值，普通名保留配置值（它是配置原文，不是展开结果）。
pub fn env_summary(entries: &[(&str, &str)]) -> String {
    entries
        .iter()
        .map(|(name, value)| {
            if is_sensitive_name(name) {
                format!("{name}: （已隐藏）")
            } else {
                format!("{name}: {value}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> RunSnapshot {
        RunSnapshot {
            id: "run".into(),
            action_id: "a".into(),
            action_name: "a".into(),
            input: String::new(),
            status: super::super::RunStatus::Running,
            stage: "准备执行".into(),
            stdout: String::new(),
            stderr: String::new(),
            cwd: None,
            started_at: 0,
            finished_at: None,
            error: None,
            exit_code: None,
            http_status: None,
            truncated: false,
            submission_unknown: false,
            unread: false,
            output_mode: crate::config::model::OutputMode::Live,
            config_revision: String::new(),
            script_revision: None,
            seq: 0,
            trace: Vec::new(),
            trace_truncated: false,
        }
    }

    fn step_text(index: usize) -> String {
        format!("第 {index} 步的细节")
    }

    #[test]
    fn normal_records_stop_before_budget_and_terminal_is_retained() {
        let mut snapshot = snapshot();
        for index in 0..40_000 {
            record(&mut snapshot, "step", step_text(index));
            if snapshot.trace_truncated {
                break;
            }
        }
        assert!(snapshot.trace_truncated);
        record_terminal(&mut snapshot, "terminal", "失败：最终原因".into());
        let last = snapshot.trace.last().unwrap();
        assert_eq!(last.step, "terminal");
        assert_eq!(last.detail, "失败：最终原因");
        assert!(used_bytes(&snapshot.trace) <= MAX_TRACE_BYTES);
    }

    /// 恰好填满普通预算（严格等号不算触界）后再写终态，总量也只能 ≤ 上限。
    #[test]
    fn terminal_fits_after_exactly_filled_prefix() {
        let mut snapshot = snapshot();
        record(&mut snapshot, "step", "x".repeat(1_000));
        let target = MAX_TRACE_BYTES - TERMINAL_RESERVE_BYTES;
        let room = target - used_bytes(&snapshot.trace);
        // step "fill" 四字节 + detail 补齐剩余 room，总量恰好等于预算（等号不触界）。
        record(&mut snapshot, "fill", "x".repeat(room - "fill".len()));
        assert_eq!(used_bytes(&snapshot.trace), target);
        assert!(!snapshot.trace_truncated);
        record_terminal(&mut snapshot, "terminal", "失败：最终原因".into());
        assert!(used_bytes(&snapshot.trace) <= MAX_TRACE_BYTES);
        assert!(snapshot
            .trace
            .last()
            .unwrap()
            .detail
            .contains("失败：最终原因"));
    }

    #[test]
    fn cut_utf8_keeps_character_boundaries_and_hard_byte_bound() {
        let (text, cut) = cut_utf8("你好", 6);
        assert!(!cut);
        assert_eq!(text, "你好");
        // 100 字节限：前缀回退到字符边界，加上后缀后总字节恰好不超限。
        let huge = "汉字块".repeat(50); // 450 字节
        let (text, cut) = cut_utf8(&huge, 100);
        assert!(cut);
        assert_eq!(text.len(), 100);
        assert!(text.ends_with(TRUNCATION_SUFFIX));
        assert!(huge.starts_with(text.trim_end_matches(TRUNCATION_SUFFIX)));
        // 限幅装不下后缀：输出空串而不是越界。
        let (text, cut) = cut_utf8("你好", 4);
        assert!(cut);
        assert_eq!(text, "");
    }

    #[test]
    fn sensitive_names_are_hidden_but_plain_config_values_are_kept() {
        assert!(is_sensitive_name("X-API-KEY"));
        assert!(is_sensitive_name("AUTHORIZATION"));
        assert!(is_sensitive_name("GITHUB_TOKEN"));
        assert!(!is_sensitive_name("Path"));
        assert_eq!(
            header_summary("Authorization", Some("Bearer s3cret"), "Bearer s3cret"),
            "Authorization: （已隐藏）"
        );
        assert_eq!(
            header_summary("X-Tenant", Some("tenant-1"), "${env:TENANT}"),
            "X-Tenant: （环境变量值已隐藏）"
        );
        assert_eq!(
            header_summary("X-Tenant", Some("tenant-1"), "tenant-1"),
            "X-Tenant: tenant-1"
        );
        assert_eq!(
            env_summary(&[("API_KEY", "s3cret"), ("PYTHONIOENCODING", "utf-8")]),
            "API_KEY: （已隐藏）\nPYTHONIOENCODING: utf-8"
        );
    }
}
