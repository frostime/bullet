use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub settings: Settings,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub default_cwd: String,
    pub notifications: NotificationMode,
    pub history_limit: usize,
    pub max_output_bytes: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_cwd: "~".into(),
            notifications: NotificationMode::Background,
            history_limit: 20,
            max_output_bytes: 1_048_576,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NotificationMode {
    #[default]
    Background,
    All,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub kind: ActionKind,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub input: InputMode,
    #[serde(default)]
    pub timeout_sec: Option<u64>,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default = "python_extension")]
    pub script_extension: String,
    #[serde(default)]
    pub http: Option<HttpConfig>,
}

fn yes() -> bool {
    true
}
fn python_extension() -> String {
    "py".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    Cli,
    Script,
    Http,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InputMode {
    #[default]
    Stdin,
    Args,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub encoding: String,
}
impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            mode: OutputMode::Live,
            encoding: "utf-8".into(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    #[default]
    Live,
    Buffered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpConfig {
    pub url: String,
    #[serde(default = "post")]
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<Value>,
    #[serde(default)]
    pub response: ResponseConfig,
}
fn post() -> String {
    "POST".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ResponseConfig {
    pub format: ResponseFormat,
    pub pointer: Option<String>,
    pub done_marker: Option<String>,
    pub success: Option<SuccessRule>,
    pub success_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    #[default]
    Text,
    Json,
    Sse,
    Jsonl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuccessRule {
    pub pointer: String,
    pub equals: Value,
}

impl Config {
    /// Only cwd whitespace is normalized; program and argument data stay untouched.
    pub fn normalize(&mut self) {
        self.settings.default_cwd = self.settings.default_cwd.trim().into();
        for action in &mut self.actions {
            action.cwd = action.cwd.as_ref().and_then(|cwd| {
                let cwd = cwd.trim();
                (!cwd.is_empty()).then(|| cwd.to_owned())
            });
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("仅支持配置 version: 1".into());
        }
        super::paths::validate_cwd(&self.settings.default_cwd, false)?;
        if !(1..=200).contains(&self.settings.history_limit) {
            return Err("historyLimit 必须为 1–200".into());
        }
        if !(4096..=8_388_608).contains(&self.settings.max_output_bytes) {
            return Err("maxOutputBytes 必须为 4096–8388608".into());
        }
        let mut ids = std::collections::HashSet::new();
        for action in &self.actions {
            let label = format!("动作 {} ({})", action.name, action.id);
            validate_id(&action.id).map_err(|error| format!("{label}：{error}"))?;
            if !ids.insert(&action.id) {
                return Err(format!("{label}：动作 ID 重复"));
            }
            action
                .validate()
                .map_err(|error| format!("{label}：{error}"))?;
        }
        Ok(())
    }
}

impl Action {
    fn validate(&self) -> Result<(), String> {
        let action = self;
        if action.name.trim().is_empty() {
            return Err("动作名称不能为空".into());
        }
        if let Some(cwd) = action.cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()) {
            super::paths::validate_cwd(cwd, true)?;
        }
        if action.kind == ActionKind::Script {
            if action.script_extension.is_empty()
                || action.script_extension.len() > 10
                || !action
                    .script_extension
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric())
            {
                return Err("脚本扩展名只能包含英文字母和数字".into());
            }
        }
        if !action.enabled {
            return Ok(());
        }
        if action.timeout_sec == Some(0) {
            return Err("timeoutSec 必须为正数或 null".into());
        }
        if encoding_rs::Encoding::for_label(action.output.encoding.as_bytes()).is_none() {
            return Err(format!("未知编码：{}", action.output.encoding));
        }
        if action.kind == ActionKind::Script && !action.args.iter().any(|arg| arg == "{{script}}") {
            return Err("Script 参数必须包含独立的 {{script}} 参数".into());
        }
        if action.kind != ActionKind::Http {
            if action.program.trim().is_empty() {
                return Err(format!("{}：程序不能为空", action.name));
            }
            let takes_input = ["{{input}}", "{{input-inline}}"];
            if action.input == InputMode::Args
                && !action
                    .args
                    .iter()
                    .any(|arg| takes_input.iter().any(|token| arg.contains(token)))
            {
                return Err(format!(
                    "{}：参数输入模式需要 {{{{input}}}} 或 {{{{input-inline}}}}",
                    action.name
                ));
            }
        } else {
            let http = action.http.as_ref().ok_or("HTTP 动作缺少 http 配置")?;
            let url = reqwest::Url::parse(&http.url).map_err(|_| "HTTP URL 无效")?;
            if !["http", "https"].contains(&url.scheme()) {
                return Err("只支持 HTTP/HTTPS URL".into());
            }
            reqwest::Method::from_bytes(http.method.as_bytes()).map_err(|_| "HTTP method 无效")?;
            for pointer in [
                http.response.pointer.as_ref(),
                http.response.success.as_ref().map(|r| &r.pointer),
            ]
            .into_iter()
            .flatten()
            {
                if !pointer.is_empty() && !pointer.starts_with('/') {
                    return Err("结果路径使用 JSON Pointer，例如 /choices/0/message/content".into());
                }
            }
            if http.response.success.is_some() && http.response.format != ResponseFormat::Json {
                return Err("success 条件仅支持 JSON 返回".into());
            }
        }
        Ok(())
    }
}

pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("动作 ID 使用英文字母、数字、连字符或下划线，长度 1–80".into());
    }
    Ok(())
}
