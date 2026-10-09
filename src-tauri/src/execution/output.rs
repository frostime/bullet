use crate::config::model::ResponseFormat;
use encoding_rs::{Decoder, Encoding};
use serde_json::Value;

pub struct TextDecoder {
    decoder: Decoder,
}
impl TextDecoder {
    pub fn new(label: &str) -> Result<Self, String> {
        let encoding = Encoding::for_label(label.as_bytes()).ok_or("未知输出编码")?;
        Ok(Self {
            decoder: encoding.new_decoder_with_bom_removal(),
        })
    }
    pub fn decode(&mut self, bytes: &[u8], last: bool) -> String {
        let mut text = String::with_capacity(
            self.decoder
                .max_utf8_buffer_length(bytes.len())
                .unwrap_or(bytes.len() * 4 + 32),
        );
        let _ = self.decoder.decode_to_string(bytes, &mut text, last);
        text
    }
}

pub fn append_bounded(target: &mut String, text: &str, limit: usize) -> bool {
    let remaining = limit.saturating_sub(target.len());
    if text.len() <= remaining {
        target.push_str(text);
        return false;
    }
    let mut end = remaining.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    target.push_str(&text[..end]);
    true
}

pub fn extract_json(value: &Value, pointer: Option<&str>) -> Result<Option<String>, String> {
    let selected = match pointer {
        Some(path) => match value.pointer(path) {
            Some(value) => value,
            None => return Ok(None),
        },
        None => value,
    };
    Ok(match selected {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        _ => Some(serde_json::to_string_pretty(selected).map_err(|error| error.to_string())?),
    })
}

/// Network chunks are not SSE events. Keep incomplete lines and multiline data until a blank line.
pub struct RecordParser {
    format: ResponseFormat,
    line: String,
    event_data: Vec<String>,
    event_bytes: usize,
    previous_cr: bool,
    at_start: bool,
}
const MAX_RECORD_BYTES: usize = 1_048_576;
impl RecordParser {
    pub fn new(format: ResponseFormat) -> Self {
        Self {
            format,
            line: String::new(),
            event_data: Vec::new(),
            event_bytes: 0,
            previous_cr: false,
            at_start: true,
        }
    }
    pub fn push(&mut self, text: &str, eof: bool) -> Result<Vec<String>, String> {
        let mut records = Vec::new();
        for character in text.chars() {
            if self.at_start {
                self.at_start = false;
                if character == '\u{feff}' {
                    continue;
                }
            }
            if character == '\n' && self.previous_cr {
                self.previous_cr = false;
                continue;
            }
            self.previous_cr = character == '\r';
            if character == '\r' || character == '\n' {
                self.finish_line(&mut records)?;
            } else {
                self.line.push(character);
                if self.line.len() > MAX_RECORD_BYTES {
                    return Err("流式记录超过 1 MiB".into());
                }
            }
        }
        if eof && self.format == ResponseFormat::Jsonl && !self.line.trim().is_empty() {
            self.finish_line(&mut records)?;
        }
        // SSE requires a blank line; an unterminated final event is not dispatched.
        Ok(records)
    }
    fn finish_line(&mut self, records: &mut Vec<String>) -> Result<(), String> {
        let line = std::mem::take(&mut self.line);
        if self.format == ResponseFormat::Jsonl {
            if !line.trim().is_empty() {
                records.push(line);
            }
        } else if line.is_empty() {
            if !self.event_data.is_empty() {
                records.push(self.event_data.join("\n"));
            }
            self.event_data.clear();
            self.event_bytes = 0;
        } else if let Some(data) = line.strip_prefix("data:") {
            let data = data.strip_prefix(' ').unwrap_or(data);
            self.event_bytes += data.len() + 1;
            if self.event_bytes > MAX_RECORD_BYTES {
                return Err("SSE 事件超过 1 MiB".into());
            }
            self.event_data.push(data.into());
        } else if line == "data" {
            self.event_bytes += 1;
            if self.event_bytes > MAX_RECORD_BYTES {
                return Err("SSE 事件超过 1 MiB".into());
            }
            self.event_data.push(String::new());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_and_sse_survive_arbitrary_chunk_boundaries() {
        let mut decoder = TextDecoder::new("utf-8").unwrap();
        let bytes = "你好".as_bytes();
        assert_eq!(decoder.decode(&bytes[..2], false), "");
        assert_eq!(decoder.decode(&bytes[2..], true), "你好");
        let mut parser = RecordParser::new(ResponseFormat::Sse);
        assert!(parser.push("data: one\r", false).unwrap().is_empty());
        assert!(parser.push("\ndata: two\r", false).unwrap().is_empty());
        assert_eq!(parser.push("\n\r\n", false).unwrap(), vec!["one\ntwo"]);
    }
    #[test]
    fn bounded_output_preserves_character_boundaries() {
        let mut text = String::new();
        assert!(append_bounded(&mut text, "你好吗", 4));
        assert_eq!(text, "你");
        assert!(append_bounded(&mut text, "世界", 4));
    }
    #[test]
    fn jsonl_flushes_final_line_but_sse_does_not() {
        assert_eq!(
            RecordParser::new(ResponseFormat::Jsonl)
                .push("{\"x\":1}", true)
                .unwrap()
                .len(),
            1
        );
        assert!(RecordParser::new(ResponseFormat::Sse)
            .push("data: incomplete", true)
            .unwrap()
            .is_empty());
    }
}
