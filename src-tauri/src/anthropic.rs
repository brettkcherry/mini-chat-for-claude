// Anthropic streaming client.
//
// Wire format reference: https://docs.anthropic.com/en/api/messages-streaming
// We POST to /v1/messages with `stream: true`, then parse the SSE response:
// each event is a `data: { ... }\n\n` block. The deltas we care about live in
// `content_block_delta` events. `message_stop` signals end-of-turn.
//
// The streaming function takes a callback so the caller (commands::send_chat)
// can decide how to surface chunks — currently via Tauri events to the
// frontend, but the same function works for, say, writing to a file.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Wrapper for effort — the API shape is `output_config: { effort: "high" }`
/// (verified against platform.claude.com/docs/en/api/messages 2026-07-02;
/// a bare top-level `effort` field 400s with "Extra inputs are not permitted").
#[derive(Serialize)]
struct OutputConfig<'a> {
    effort: &'a str,
}

#[derive(Serialize)]
struct RequestBody<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: &'a [Message],
    stream: bool,
    /// Omitted entirely when no effort chosen — some models (Haiku 4.5)
    /// reject it, and absence = API default.
    #[serde(skip_serializing_if = "Option::is_none")]
    output_config: Option<OutputConfig<'a>>,
}

/// A single streaming event we surface back to the caller.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct ChunkEvent {
    /// The newly-arrived text token. Empty on `stop` or non-text events.
    pub delta: String,
    /// True when the stream has finished and the message is complete.
    pub stop: bool,
}

/// SSE parser for the Anthropic streaming wire format. Accumulates bytes,
/// splits on `\n\n` event boundaries, then within each event looks for
/// `data: ` lines and parses JSON. Network-free — takes raw text chunks in,
/// yields parsed `ChunkEvent`s out. Extracted from `stream_chat` so the
/// buffer/boundary logic can be unit tested without a live connection.
pub(crate) struct SseParser {
    buffer: String,
}

impl SseParser {
    pub(crate) fn new() -> Self {
        Self { buffer: String::new() }
    }

    /// Feed a chunk of raw SSE text; returns any complete events it produced,
    /// in arrival order. Text that doesn't complete an event boundary stays
    /// buffered for the next call.
    pub(crate) fn push(&mut self, text: &str) -> Vec<ChunkEvent> {
        self.buffer.push_str(text);
        let mut events = Vec::new();

        while let Some(idx) = self.buffer.find("\n\n") {
            let event_block: String = self.buffer.drain(..idx + 2).collect();

            for line in event_block.lines() {
                let Some(json_str) = line.strip_prefix("data: ") else {
                    continue;
                };
                let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) else {
                    continue;
                };

                match value.get("type").and_then(|t| t.as_str()) {
                    Some("content_block_delta") => {
                        if let Some(text) = value
                            .pointer("/delta/text")
                            .and_then(|t| t.as_str())
                        {
                            events.push(ChunkEvent {
                                delta: text.to_string(),
                                stop: false,
                            });
                        }
                    }
                    Some("message_stop") => {
                        events.push(ChunkEvent {
                            delta: String::new(),
                            stop: true,
                        });
                    }
                    // We ignore message_start / content_block_start /
                    // content_block_stop / message_delta / ping for v0.1.
                    // They become interesting once we surface token counts.
                    _ => {}
                }
            }
        }

        events
    }
}

/// Stream a chat completion. Calls `on_chunk` for each delta + once with
/// `stop: true` when the stream ends cleanly.
pub async fn stream_chat<F>(
    api_key: String,
    model: String,
    messages: Vec<Message>,
    effort: Option<String>,
    on_chunk: F,
) -> Result<(), String>
where
    F: Fn(ChunkEvent) + Send + 'static,
{
    let body = RequestBody {
        model: &model,
        max_tokens: 4096,
        messages: &messages,
        stream: true,
        output_config: effort.as_deref().map(|e| OutputConfig { effort: e }),
    };

    let client = reqwest::Client::new();
    let res = client
        .post(API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("api error {status}: {text}"));
    }

    // SSE parsing (buffer/boundary/event logic) lives in SseParser — see
    // its doc comment. Here we just feed it network chunks and forward
    // whatever events it produces.
    let mut stream = res.bytes_stream();
    let mut parser = SseParser::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("stream error: {e}"))?;
        let text = std::str::from_utf8(&chunk)
            .map_err(|e| format!("utf8 decode error: {e}"))?;
        for ev in parser.push(text) {
            on_chunk(ev);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_content_block_delta() {
        let mut parser = SseParser::new();
        let events = parser.push(
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hello\"}}\n\n",
        );
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].delta, "Hello");
        assert_eq!(events[0].stop, false);
    }

    #[test]
    fn message_stop_event() {
        let mut parser = SseParser::new();
        let events = parser.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].delta, "");
        assert_eq!(events[0].stop, true);
    }

    #[test]
    fn two_events_in_one_push() {
        let mut parser = SseParser::new();
        let input = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"A\"}}\n\n\
                     event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"B\"}}\n\n";
        let events = parser.push(input);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].delta, "A");
        assert_eq!(events[1].delta, "B");
    }

    #[test]
    fn event_split_across_two_pushes() {
        let mut parser = SseParser::new();
        let first = parser.push(
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hi\"}}",
        );
        assert!(first.is_empty());
        let second = parser.push("\n\n");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].delta, "Hi");
    }

    #[test]
    fn malformed_json_ignored() {
        let mut parser = SseParser::new();
        let events = parser.push("data: {not valid json\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn non_data_line_ignored() {
        let mut parser = SseParser::new();
        let events = parser.push("event: ping\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn ignored_event_type() {
        let mut parser = SseParser::new();
        let events = parser.push(
            "event: content_block_start\ndata: {\"type\":\"content_block_start\"}\n\n",
        );
        assert!(events.is_empty());
    }

    #[test]
    fn realistic_multi_event_stream() {
        let mut parser = SseParser::new();
        let input = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\"}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hel\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"lo, \"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"world!\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let events = parser.push(input);
        assert_eq!(events.len(), 4);
        let text: String = events
            .iter()
            .filter(|e| !e.stop)
            .map(|e| e.delta.clone())
            .collect();
        assert_eq!(text, "Hello, world!");
        assert!(events.last().unwrap().stop);
    }

    #[test]
    fn request_body_shape_with_effort() {
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let body = RequestBody {
            model: "claude-x",
            max_tokens: 4096,
            messages: &messages,
            stream: true,
            output_config: Some(OutputConfig { effort: "high" }),
        };
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["output_config"]["effort"], "high");
        assert_eq!(v["stream"], true);
        assert!(v["max_tokens"].is_number());
    }

    #[test]
    fn request_body_shape_without_effort() {
        let messages = vec![Message {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let body = RequestBody {
            model: "claude-x",
            max_tokens: 4096,
            messages: &messages,
            stream: true,
            output_config: None,
        };
        let v = serde_json::to_value(&body).unwrap();
        assert!(v.get("output_config").is_none());
        assert_eq!(v["stream"], true);
        assert!(v["max_tokens"].is_number());
    }
}
