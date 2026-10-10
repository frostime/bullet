use super::{
    output::{extract_json, RecordParser, TextDecoder},
    substitute_input, ExecutionFailure, ExecutionResult, PreparedAction, RunManager,
};
use crate::config::model::ResponseFormat;
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;

pub async fn execute(
    prepared: &PreparedAction,
    manager: &Arc<RunManager>,
    id: &str,
    limit: usize,
) -> Result<ExecutionResult, ExecutionFailure> {
    let config = prepared
        .action
        .http
        .as_ref()
        .ok_or_else(|| ExecutionFailure::from("HTTP 配置缺失".to_string()))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| ExecutionFailure::from(error.to_string()))?;
    let method = reqwest::Method::from_bytes(config.method.as_bytes())
        .map_err(|error| ExecutionFailure::from(error.to_string()))?;
    let mut request = client.request(method.clone(), &config.url);
    for (name, value) in &config.headers {
        request = request.header(
            name,
            expand_environment(value).map_err(ExecutionFailure::from)?,
        );
    }
    if let Some(body) = &config.body {
        request = request.json(&render_body(body, &prepared.input));
    }
    manager.stage(id, "请求已发起");
    let response = request.send().await.map_err(|error| ExecutionFailure {
        message: "网络请求失败，请检查地址、鉴权和网络；未自动重试".into(),
        submission_unknown: !error.is_connect()
            && method != reqwest::Method::GET
            && method != reqwest::Method::HEAD,
    })?;
    let status = response.status();
    manager.record_http_status(id, status.as_u16());
    if !status.is_success() {
        return Err(format!("HTTP {}：服务器返回错误；未自动重试", status.as_u16()).into());
    }
    manager.stage(id, "读取响应");
    let mut stream = response.bytes_stream();
    let mut decoder =
        TextDecoder::new(&prepared.action.output.encoding).map_err(ExecutionFailure::from)?;
    let mut document = String::new();
    let mut parser = RecordParser::new(config.response.format);
    let mut saw_done = false;
    loop {
        let chunk = stream.next().await;
        let eof = chunk.is_none();
        let text = match chunk {
            Some(Ok(bytes)) => decoder.decode(&bytes, false),
            Some(Err(_)) => {
                return Err(ExecutionFailure {
                    message: "响应中断；未自动重试".into(),
                    submission_unknown: method != reqwest::Method::GET
                        && method != reqwest::Method::HEAD,
                })
            }
            None => decoder.decode(&[], true),
        };
        match config.response.format {
            ResponseFormat::Text => manager.append(id, false, &text, limit),
            ResponseFormat::Json => {
                if document.len() + text.len() > 8_388_608 {
                    return Err("JSON 响应超过 8 MiB".to_string().into());
                }
                document.push_str(&text);
            }
            ResponseFormat::Sse | ResponseFormat::Jsonl => {
                for record in parser.push(&text, eof).map_err(ExecutionFailure::from)? {
                    if config.response.done_marker.as_deref() == Some(record.as_str()) {
                        saw_done = true;
                        break;
                    }
                    if let Some(pointer) = &config.response.pointer {
                        let value: Value = serde_json::from_str(&record).map_err(|error| {
                            ExecutionFailure::from(format!("流式 JSON 解析失败：{error}"))
                        })?;
                        if let Some(output) =
                            extract_json(&value, Some(pointer)).map_err(ExecutionFailure::from)?
                        {
                            manager.append(id, false, &output, limit);
                        }
                    } else {
                        manager.append(id, false, &format!("{record}\n"), limit);
                    }
                }
            }
        }
        if eof || saw_done {
            break;
        }
    }
    if config.response.format == ResponseFormat::Json {
        let value: Value = serde_json::from_str(&document)
            .map_err(|error| ExecutionFailure::from(format!("JSON 解析失败：{error}")))?;
        if let Some(rule) = &config.response.success {
            if value.pointer(&rule.pointer) != Some(&rule.equals) {
                manager.append(id, false, &document, limit);
                return Err("服务器业务成功条件不满足，请检查返回结果"
                    .to_string()
                    .into());
            }
        }
        let result = if let Some(text) = &config.response.success_text {
            Some(text.clone())
        } else {
            extract_json(&value, config.response.pointer.as_deref())
                .map_err(ExecutionFailure::from)?
        };
        let result = result
            .ok_or_else(|| ExecutionFailure::from("配置的结果路径不存在或为空".to_string()))?;
        manager.append(id, false, &result, limit);
    }
    if config.response.done_marker.is_some()
        && matches!(
            config.response.format,
            ResponseFormat::Sse | ResponseFormat::Jsonl
        )
        && !saw_done
    {
        return Err("流式响应结束但缺少完成标记，已保留部分结果"
            .to_string()
            .into());
    }
    Ok(ExecutionResult {
        exit_code: None,
        http_status: Some(status.as_u16()),
    })
}

/// Substitute input placeholders inside parsed JSON values, never by concatenating
/// a raw JSON document.
pub fn render_body(value: &Value, input: &str) -> Value {
    match value {
        Value::String(text) => Value::String(substitute_input(text, input)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| render_body(item, input)).collect())
        }
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), render_body(value, input)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn expand_environment(value: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut tail = value;
    while let Some(start) = tail.find("${env:") {
        result.push_str(&tail[..start]);
        let variable = &tail[start + 6..];
        let end = variable.find('}').ok_or("环境变量引用缺少 }")?;
        let name = &variable[..end];
        let replacement = std::env::var(name).map_err(|_| format!("环境变量未设置：{name}"))?;
        result.push_str(&replacement);
        tail = &variable[end + 1..];
    }
    result.push_str(tail);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_is_json_data_even_with_quotes_and_newlines() {
        let body = serde_json::json!({"data":"{{input}}", "number": 1});
        let input = "\"\n中文\\path";
        let rendered = render_body(&body, input);
        let roundtrip: Value = serde_json::from_str(&rendered.to_string()).unwrap();
        assert_eq!(roundtrip["data"], input);
        assert_eq!(roundtrip["number"], 1);
    }

    /// `{{input-inline}}` lands inside a string that is itself JSON; after
    /// replacement the inner document must re-parse and keep the input intact.
    #[test]
    fn input_inline_keeps_inner_json_parseable() {
        let body = serde_json::json!({"val": "{\"a\": \"{{input-inline}}\"}"});
        let input = "he said \"hi\"\n中文";
        let rendered = render_body(&body, input);
        let inner: Value = serde_json::from_str(rendered["val"].as_str().unwrap()).unwrap();
        assert_eq!(inner["a"], input);
    }

    #[test]
    fn mixed_placeholders_render_each_in_its_own_form() {
        let body = serde_json::json!({"raw": "{{input}}", "inline": "{{input-inline}}"});
        let input = "a\"b";
        let rendered = render_body(&body, input);
        assert_eq!(rendered["raw"], input);
        assert_eq!(rendered["inline"], "a\\\"b");
    }

    /// Input containing a literal placeholder token must stay verbatim, never
    /// re-expanded by a scan of the inserted text.
    #[test]
    fn input_content_is_never_rescanned() {
        let body = serde_json::json!({"text": "before {{input}} after"});
        let rendered = render_body(&body, "{{input-inline}}");
        assert_eq!(rendered["text"], "before {{input-inline}} after");
    }
}
