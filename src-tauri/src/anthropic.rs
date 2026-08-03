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
use tokio::sync::oneshot;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODELS_URL: &str = "https://api.anthropic.com/v1/models?limit=100";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Effort levels in the order the UI should offer them, weakest first.
const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Ceiling on a single reply, whatever a model says it can produce.
///
/// Two things make this matter more than it looks. Current models advertise up
/// to 128K output tokens — nobody reads a 128K-token answer in a 380px window,
/// but they would wait for it and pay for it. And on models where thinking is
/// on by default, `max_tokens` caps thinking *and* visible text together, so a
/// number chosen for the answer alone gets eaten by the reasoning before a
/// single word reaches the screen. 32K leaves generous room for both while
/// bounding the worst case. Unused headroom costs nothing — this is a cap, not
/// a reservation.
const MAX_RESPONSE_TOKENS: u32 = 32_768;

/// Clamp whatever the caller asked for into something sane.
///
/// The request carries the selected model's own reported ceiling (see
/// [`ModelInfo::max_tokens`]), so this is a guard rail rather than a policy:
/// it stops a missing or absurd value from either truncating replies or
/// authorizing an enormous one.
fn resolve_max_tokens(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(MAX_RESPONSE_TOKENS)
        .clamp(1024, MAX_RESPONSE_TOKENS)
}

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

/// Automatic prompt caching. Every turn re-sends the entire conversation, so
/// by turn N the same prefix has been paid for at full input price N times
/// over. This marks the last cacheable block; the next turn reads that prefix
/// back at roughly a tenth of the cost. Conversations shorter than the model's
/// minimum cacheable length simply don't cache — no error, no behavior change,
/// which is why it can be unconditional.
#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
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
    cache_control: CacheControl,
}

/// Everything one turn needs. A struct rather than a sixth positional
/// argument on `stream_chat`.
pub struct ChatRequest {
    pub api_key: String,
    pub model: String,
    pub messages: Vec<Message>,
    pub effort: Option<String>,
    /// The selected model's own output ceiling, as reported by `/v1/models`.
    pub max_tokens: Option<u32>,
}

/// A single streaming event we surface back to the caller.
#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChunkEvent {
    /// The newly-arrived text token. Empty on `stop` or non-text events.
    pub delta: String,
    /// True when the stream has finished and the message is complete.
    pub stop: bool,
    /// Set on the final event when the turn ended for a reason the user needs
    /// to know about — an error mid-response, the length cap, a refusal, a
    /// cancellation. `None` on an ordinary completion. User-facing text.
    pub notice: Option<String>,
}

impl ChunkEvent {
    fn text(delta: &str) -> Self {
        Self { delta: delta.to_string(), stop: false, notice: None }
    }

    fn stop() -> Self {
        Self { delta: String::new(), stop: true, notice: None }
    }

    fn stop_with(notice: impl Into<String>) -> Self {
        Self { delta: String::new(), stop: true, notice: Some(notice.into()) }
    }
}

/// SSE parser for the Anthropic streaming wire format. Accumulates bytes,
/// splits on `\n\n` event boundaries, then within each event looks for
/// `data: ` lines and parses JSON. Network-free — takes raw text chunks in,
/// yields parsed `ChunkEvent`s out. Extracted from `stream_chat` so the
/// buffer/boundary logic can be unit tested without a live connection.
pub(crate) struct SseParser {
    buffer: String,
    /// Bytes at the tail of the last chunk that didn't form a whole UTF-8
    /// character yet. See `push_bytes`.
    pending: Vec<u8>,
    /// The most recent `stop_reason`. The API reports why a turn ended on the
    /// `message_delta` *before* `message_stop`, so it has to be carried across
    /// events rather than read at the point it's needed.
    stop_reason: Option<String>,
    saw_stop: bool,
}

impl SseParser {
    pub(crate) fn new() -> Self {
        Self {
            buffer: String::new(),
            pending: Vec::new(),
            stop_reason: None,
            saw_stop: false,
        }
    }

    /// Feed raw bytes straight off the wire.
    ///
    /// The transport splits the body on byte counts, not character
    /// boundaries, so a chunk can end halfway through a multi-byte UTF-8
    /// character — an em dash, a curly quote, an emoji, an accented name, all
    /// of which Claude emits constantly. Decoding each chunk in isolation
    /// therefore fails at random on perfectly valid output, and because that
    /// failure used to abort the whole request, the partial reply on screen
    /// was replaced by a decode error. Anything incomplete at the tail is held
    /// back and prepended to the next chunk instead.
    pub(crate) fn push_bytes(&mut self, bytes: &[u8]) -> Vec<ChunkEvent> {
        self.pending.extend_from_slice(bytes);

        let mut text = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(s) => {
                    text.push_str(s);
                    self.pending.clear();
                    break;
                }
                Err(e) => {
                    let good = e.valid_up_to();
                    if let Ok(s) = std::str::from_utf8(&self.pending[..good]) {
                        text.push_str(s);
                    }
                    match e.error_len() {
                        // Ran out of input mid-character: keep the tail for the
                        // next chunk. This is the split we're here for.
                        None => {
                            self.pending.drain(..good);
                            break;
                        }
                        // Genuinely invalid bytes. Skip them and keep going, so
                        // one bad byte can't wedge the rest of the stream.
                        Some(n) => {
                            self.pending.drain(..good + n);
                        }
                    }
                }
            }
        }

        self.push(&text)
    }

    /// Whether a terminal event has been seen. A stream that ends without one
    /// needs a synthetic stop from the caller — see `stream_chat`.
    pub(crate) fn saw_stop(&self) -> bool {
        self.saw_stop
    }

    /// Feed a chunk of decoded SSE text; returns any complete events it
    /// produced, in arrival order. Text that doesn't complete an event
    /// boundary stays buffered for the next call.
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
                            events.push(ChunkEvent::text(text));
                        }
                    }
                    // Carries `stop_reason` one event ahead of the stop itself.
                    Some("message_delta") => {
                        if let Some(reason) = value
                            .pointer("/delta/stop_reason")
                            .and_then(|r| r.as_str())
                        {
                            self.stop_reason = Some(reason.to_string());
                        }
                    }
                    // An error raised *after* content started streaming
                    // (overloaded, upstream failure). Dropping these — which
                    // this parser used to do, along with every other unknown
                    // type — meant the stream simply ended: no `message_stop`,
                    // so the caller never learned the turn was over, the bubble
                    // kept its streaming caret forever, and the text already on
                    // screen never made it into the conversation history.
                    Some("error") => {
                        let detail = value
                            .pointer("/error/message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("the connection to Anthropic failed");
                        self.saw_stop = true;
                        events.push(ChunkEvent::stop_with(format!(
                            "The response stopped early — {detail}"
                        )));
                    }
                    Some("message_stop") => {
                        self.saw_stop = true;
                        events.push(match self.stop_reason.as_deref() {
                            // Cut off at the token cap rather than finished.
                            // Without saying so, this is indistinguishable
                            // from a reply that just ends mid-sentence.
                            Some("max_tokens") => ChunkEvent::stop_with(
                                "Response hit the length limit and was cut off — ask Claude to continue.",
                            ),
                            // Safety classifiers declined. Content is empty or
                            // partial; an empty bubble with no explanation is
                            // the worst possible way to present that.
                            Some("refusal") => ChunkEvent::stop_with(
                                "Claude declined to answer this one.",
                            ),
                            _ => ChunkEvent::stop(),
                        });
                    }
                    // message_start / content_block_start / content_block_stop
                    // / ping carry nothing we act on yet. They become
                    // interesting once we surface token counts.
                    _ => {}
                }
            }
        }

        events
    }
}

/// Stream a chat completion. Calls `on_chunk` for each delta, then exactly
/// once with `stop: true` — including when the turn ends badly, so the caller
/// never has to guess whether more is coming.
///
/// `cancel` fires when the user hits stop. Dropping the response stream closes
/// the connection, which is what actually halts generation upstream.
pub async fn stream_chat<F>(
    req: ChatRequest,
    mut cancel: oneshot::Receiver<()>,
    on_chunk: F,
) -> Result<(), String>
where
    F: Fn(ChunkEvent) + Send + 'static,
{
    let body = RequestBody {
        model: &req.model,
        max_tokens: resolve_max_tokens(req.max_tokens),
        messages: &req.messages,
        stream: true,
        output_config: req.effort.as_deref().map(|e| OutputConfig { effort: e }),
        cache_control: CacheControl { kind: "ephemeral" },
    };

    let client = reqwest::Client::new();
    let res = client
        .post(API_URL)
        .header("x-api-key", &req.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(friendly_api_error(status.as_u16(), &text));
    }

    // SSE parsing (buffer/boundary/event logic) lives in SseParser — see
    // its doc comment. Here we just feed it network chunks and forward
    // whatever events it produces.
    let mut stream = res.bytes_stream();
    let mut parser = SseParser::new();
    let mut cancelled = false;

    loop {
        tokio::select! {
            // `biased` so a pending cancel is checked before an already-ready
            // chunk. Without it a fast stream can starve the cancel branch and
            // the stop button does nothing until the reply finishes on its own.
            biased;

            _ = &mut cancel => {
                cancelled = true;
                break;
            }

            chunk = stream.next() => {
                let Some(chunk) = chunk else { break };
                let chunk = chunk.map_err(|e| format!("stream error: {e}"))?;
                for ev in parser.push_bytes(&chunk) {
                    on_chunk(ev);
                }
                if parser.saw_stop() {
                    break;
                }
            }
        }
    }

    if cancelled {
        on_chunk(ChunkEvent::stop_with("Stopped."));
    } else if !parser.saw_stop() {
        // The body ended without a terminal event — a dropped connection, a
        // proxy timeout. The caller would otherwise wait forever for a stop
        // that is never coming, leaving the partial reply stranded outside the
        // conversation history.
        on_chunk(ChunkEvent::stop_with(
            "The response ended unexpectedly — the connection may have dropped.",
        ));
    }

    Ok(())
}

/// Turn an API error body into something a person can act on.
///
/// The raw JSON was previously rendered straight into the chat bubble, so the
/// two failures a user can actually fix — no credit, conversation too long —
/// arrived as a wall of escaped JSON.
fn friendly_api_error(status: u16, body: &str) -> String {
    let lower = body.to_lowercase();

    if lower.contains("credit balance") {
        return "Your Anthropic API credit balance is too low. Top it up at \
                console.anthropic.com — note that API credits are separate from a \
                Claude.ai subscription."
            .to_string();
    }
    if status == 401 || status == 403 {
        return "Anthropic rejected the API key. Open Settings → API key to check \
                or replace it."
            .to_string();
    }
    if status == 429 {
        return "Rate limited by Anthropic. Wait a moment and try again.".to_string();
    }
    if lower.contains("context") && (lower.contains("long") || lower.contains("exceed")) {
        return "This conversation has grown past what the model can read in one \
                go. Start a new chat (➕) to continue."
            .to_string();
    }
    if status >= 500 {
        return format!("Anthropic's API is having trouble (error {status}). Try again shortly.");
    }

    format!("api error {status}: {body}")
}

/// One model as the picker needs it. `efforts` holds canonical lowercase
/// API values ("low", "xhigh", ...) — presentation casing is the
/// frontend's business.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub efforts: Vec<String>,
    /// The model's own output-token ceiling, straight from `/v1/models`.
    ///
    /// Read rather than hardcoded for the same reason the model list is: one
    /// baked-in number ages badly across a lineup. The previous fixed 4096
    /// silently truncated long replies on models that can produce 128K, and
    /// on models that think by default it could be spent entirely on
    /// reasoning before any text appeared.
    pub max_tokens: Option<u32>,
}

/// Fetch the account's available models from `/v1/models`.
///
/// This exists so the picker never goes stale: the endpoint reports both
/// the current model IDs and each model's exact effort capabilities, which
/// is otherwise a hardcoded table that rots every few weeks (it already
/// rotted twice — Opus 4.7 -> 4.8 -> 5). Retired models drop off this list
/// automatically; new ones appear without an app update.
pub async fn list_models(api_key: String) -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::new();
    let res = client
        .get(MODELS_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("api error {status}: {text}"));
    }

    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("models parse error: {e}"))?;

    Ok(parse_models(&body))
}

/// Split out from the request so it's unit-testable against a fixture.
fn parse_models(body: &serde_json::Value) -> Vec<ModelInfo> {
    let Some(data) = body.get("data").and_then(|d| d.as_array()) else {
        return Vec::new();
    };

    data.iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(|v| v.as_str())?;

            // Claude Mythos is invitation-only (Project Glasswing). If an
            // account can't use it, showing it in the picker is a trap.
            if id.contains("mythos") {
                return None;
            }

            let display = m.get("display_name").and_then(|v| v.as_str()).unwrap_or(id);
            // The badge is ~70px wide: "Claude Sonnet 5" -> "Sonnet 5".
            let label = display
                .strip_prefix("Claude ")
                .unwrap_or(display)
                .to_string();

            let effort = m.pointer("/capabilities/effort");
            let effort_supported = effort
                .and_then(|e| e.get("supported"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let efforts = if effort_supported {
                EFFORT_LEVELS
                    .iter()
                    .filter(|lvl| {
                        effort
                            .and_then(|e| e.get(*lvl))
                            .and_then(|l| l.get("supported"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                    })
                    .map(|lvl| lvl.to_string())
                    .collect()
            } else {
                Vec::new()
            };

            Some(ModelInfo {
                id: id.to_string(),
                label,
                efforts,
                max_tokens: m
                    .get("max_tokens")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32),
            })
        })
        .collect()
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

    /// The bug this whole byte path exists for: a multi-byte character
    /// straddling a chunk boundary. Decoding each chunk on its own returned an
    /// error, which aborted the request and wiped the partial reply off screen.
    #[test]
    fn multibyte_char_split_across_chunks() {
        let mut parser = SseParser::new();
        let full = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"a—b\"}}\n\n";
        let bytes = full.as_bytes();

        // Cut mid-em-dash. `—` is 3 bytes (E2 80 94); land between the first
        // and second so the first chunk ends on an incomplete character.
        let split = full.find('—').unwrap() + 1;
        assert!(std::str::from_utf8(&bytes[..split]).is_err(), "test setup: split must land mid-character");

        let first = parser.push_bytes(&bytes[..split]);
        assert!(first.is_empty(), "no complete event yet");

        let second = parser.push_bytes(&bytes[split..]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].delta, "a—b", "the character must survive the split intact");
    }

    /// Emoji are 4 bytes, and every one of the three interior split points has
    /// to behave.
    #[test]
    fn emoji_split_at_every_byte_boundary() {
        let full = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"hi 🚀!\"}}\n\n";
        let bytes = full.as_bytes();
        let rocket = full.find('🚀').unwrap();

        for offset in 1..4 {
            let mut parser = SseParser::new();
            let split = rocket + offset;
            assert!(parser.push_bytes(&bytes[..split]).is_empty());
            let events = parser.push_bytes(&bytes[split..]);
            assert_eq!(events.len(), 1, "split at +{offset}");
            assert_eq!(events[0].delta, "hi 🚀!", "split at +{offset}");
        }
    }

    /// Bytes that are not valid UTF-8 at all must be skipped rather than
    /// wedging the parser — the rest of the stream still has to arrive.
    #[test]
    fn invalid_bytes_are_skipped_not_fatal() {
        let mut parser = SseParser::new();
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend_from_slice(
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"ok\"}}\n\n"
                .as_bytes(),
        );
        let events = parser.push_bytes(&bytes);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].delta, "ok");
    }

    /// A mid-stream `error` event used to be dropped like any other unknown
    /// type, so the stream just stopped: no terminal event, a bubble stuck
    /// streaming forever, and the text already received never committed.
    #[test]
    fn mid_stream_error_terminates_with_a_notice() {
        let mut parser = SseParser::new();
        let input = concat!(
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"partial\"}}\n\n",
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        );
        let events = parser.push(input);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].delta, "partial");
        assert!(events[1].stop);
        assert!(events[1].notice.as_deref().unwrap().contains("Overloaded"));
        assert!(parser.saw_stop());
    }

    /// `stop_reason` arrives on the event *before* the stop, so it has to be
    /// carried across events.
    #[test]
    fn max_tokens_stop_reason_produces_a_truncation_notice() {
        let mut parser = SseParser::new();
        let input = concat!(
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"max_tokens\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let events = parser.push(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].stop);
        assert!(events[0].notice.as_deref().unwrap().contains("length limit"));
    }

    #[test]
    fn refusal_stop_reason_produces_a_notice() {
        let mut parser = SseParser::new();
        let input = concat!(
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let events = parser.push(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].stop);
        assert!(events[0].notice.is_some());
    }

    /// An ordinary completion must stay quiet — a notice on every turn would
    /// train people to ignore them.
    #[test]
    fn normal_end_turn_has_no_notice() {
        let mut parser = SseParser::new();
        let input = concat!(
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let events = parser.push(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].stop);
        assert!(events[0].notice.is_none());
    }

    #[test]
    fn max_tokens_is_clamped_into_range() {
        // A model that reports a 128K ceiling still gets capped.
        assert_eq!(resolve_max_tokens(Some(128_000)), MAX_RESPONSE_TOKENS);
        // Missing value means "as much as we allow", not the old tiny default.
        assert_eq!(resolve_max_tokens(None), MAX_RESPONSE_TOKENS);
        // A model with a genuinely smaller ceiling is respected.
        assert_eq!(resolve_max_tokens(Some(8_192)), 8_192);
        // Nonsense can't produce a one-token reply.
        assert_eq!(resolve_max_tokens(Some(0)), 1024);
    }

    #[test]
    fn api_errors_are_translated_for_humans() {
        let credit = friendly_api_error(400, r#"{"error":{"message":"Your credit balance is too low"}}"#);
        assert!(credit.contains("credit balance"));
        assert!(!credit.contains('{'), "raw JSON should not reach the user");

        assert!(friendly_api_error(401, "{}").contains("API key"));
        assert!(friendly_api_error(429, "{}").contains("Rate limited"));
        assert!(friendly_api_error(529, "{}").contains("trouble"));

        // Anything unrecognized still surfaces verbatim rather than being
        // swallowed — an opaque error is better than a wrong explanation.
        let unknown = friendly_api_error(418, "teapot");
        assert!(unknown.contains("teapot"));
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
            cache_control: CacheControl { kind: "ephemeral" },
        };
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["output_config"]["effort"], "high");
        assert_eq!(v["stream"], true);
        assert!(v["max_tokens"].is_number());
        assert_eq!(v["cache_control"]["type"], "ephemeral");
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
            cache_control: CacheControl { kind: "ephemeral" },
        };
        let v = serde_json::to_value(&body).unwrap();
        assert!(v.get("output_config").is_none());
        assert_eq!(v["stream"], true);
        assert!(v["max_tokens"].is_number());
    }

    /// Fixture mirrors the real /v1/models response shape.
    #[test]
    fn parse_models_extracts_labels_and_effort_levels() {
        let body = serde_json::json!({
            "data": [
                {
                    "type": "model",
                    "id": "claude-sonnet-5",
                    "display_name": "Claude Sonnet 5",
                    "max_input_tokens": 1000000,
                    "max_tokens": 128000,
                    "capabilities": {
                        "effort": {
                            "supported": true,
                            "low": { "supported": true },
                            "medium": { "supported": true },
                            "high": { "supported": true },
                            "xhigh": { "supported": false },
                            "max": { "supported": true }
                        }
                    }
                },
                {
                    "type": "model",
                    "id": "claude-haiku-4-5-20251001",
                    "display_name": "Claude Haiku 4.5",
                    "capabilities": { "effort": { "supported": false } }
                },
                {
                    "type": "model",
                    "id": "claude-mythos-5",
                    "display_name": "Claude Mythos 5",
                    "capabilities": { "effort": { "supported": true, "low": { "supported": true } } }
                }
            ],
            "has_more": false
        });

        let models = parse_models(&body);

        // Mythos filtered out (invitation-only).
        assert_eq!(models.len(), 2);

        // "Claude " prefix stripped for the narrow badge.
        assert_eq!(models[0].label, "Sonnet 5");
        // Unsupported level omitted, order preserved weakest-first.
        assert_eq!(models[0].efforts, vec!["low", "medium", "high", "max"]);

        // effort.supported == false yields no levels at all.
        assert_eq!(models[1].label, "Haiku 4.5");
        assert!(models[1].efforts.is_empty());

        // The output ceiling is read from the payload, not assumed...
        assert_eq!(models[0].max_tokens, Some(128_000));
        // ...and its absence is None rather than a fabricated number, so the
        // clamp decides instead of a guess.
        assert_eq!(models[1].max_tokens, None);
    }

    #[test]
    fn parse_models_tolerates_garbage() {
        assert!(parse_models(&serde_json::json!({})).is_empty());
        assert!(parse_models(&serde_json::json!({ "data": "nope" })).is_empty());
        // Entry with no id is skipped rather than panicking.
        assert!(parse_models(&serde_json::json!({ "data": [{ "x": 1 }] })).is_empty());
    }
}
