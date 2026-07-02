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

    // SSE parser. Accumulate bytes, split on `\n\n` event boundaries,
    // then within each event look for `data: ` lines and parse JSON.
    let mut stream = res.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("stream error: {e}"))?;
        let text = std::str::from_utf8(&chunk)
            .map_err(|e| format!("utf8 decode error: {e}"))?;
        buffer.push_str(text);

        while let Some(idx) = buffer.find("\n\n") {
            let event_block: String = buffer.drain(..idx + 2).collect();

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
                            on_chunk(ChunkEvent {
                                delta: text.to_string(),
                                stop: false,
                            });
                        }
                    }
                    Some("message_stop") => {
                        on_chunk(ChunkEvent {
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
    }

    Ok(())
}
