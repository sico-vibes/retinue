//! Pi's RPC records: one file that knows how to write a command and read a
//! record, so the adapter itself never guesses at a shape.
//!
//! Source of every shape here: `@earendil-works/pi-coding-agent` 1.5.2 (MIT),
//! `docs/rpc.md`, `docs/rpc-commands.md` and `docs/json.md`, read on
//! 16 November 2026. Framing is strict JSONL: one JSON object per line,
//! terminated with LF, and records are split on LF only (a generic line reader
//! that also splits on U+2028 would break a valid record).
//!
//! These functions are pure: they take a parsed record and return what the
//! adapter should do with it. Every one that can meet a shape it does not know
//! answers `None` or an empty list rather than guessing.

use retinue_agent_core::{ModelInfo, ServiceTier};
use serde_json::{json, Value};

/// A record longer than this is not a protocol record worth parsing: a provider
/// diagnostic or a runaway tool result never becomes memory pressure.
pub const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;

/// The thinking levels offered for a model that reports `reasoning: true`.
///
/// Pi's own picker can offer `xhigh` and `max`, but the model object in
/// `get_available_models` only says whether the model reasons at all, so this
/// list stops at `high`: understating what a model takes is honest, and Pi
/// clamps anything above what it supports anyway.
pub const THINKING_LEVELS: [&str; 5] = ["off", "minimal", "low", "medium", "high"];

/// The one command record. `extra` carries the command's own fields; `id` is
/// echoed by the matching response.
pub fn command(id: &str, kind: &str, extra: Value) -> Value {
    let mut record = json!({"id": id, "type": kind});
    if let (Some(record), Value::Object(extra)) = (record.as_object_mut(), extra) {
        for (key, value) in extra {
            record.insert(key, value);
        }
    }
    record
}

/// A command's answer. `error` is provider text and is sanitised before it ever
/// leaves the adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub id: Option<String>,
    pub command: String,
    pub success: bool,
    pub data: Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Response(Response),
    /// A session event, or anything else on stdout that is not a response.
    Event(Value),
}

/// Read one stdout line as a record. `None` means the line was not JSON at all
/// (a provider diagnostic written to the wrong stream) and must be ignored.
pub fn parse_record(line: &str) -> Option<Record> {
    if line.is_empty() || line.len() > MAX_RECORD_BYTES {
        return None;
    }
    let value = serde_json::from_str::<Value>(line).ok()?;
    let object = value.as_object()?;
    if object.get("type").and_then(Value::as_str) != Some("response") {
        return Some(Record::Event(value));
    }
    let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(Record::Response(Response {
        id: text("id"),
        command: text("command").unwrap_or_default(),
        success: object.get("success").and_then(Value::as_bool).unwrap_or(false),
        data: object.get("data").cloned().unwrap_or(Value::Null),
        error: text("error"),
    }))
}

/// An assistant text delta from `message_update`.
pub fn text_delta(event: &Value) -> Option<&str> {
    update_of(event, "text_delta")?.get("delta")?.as_str()
}

/// A thinking delta from `message_update`.
pub fn thinking_delta(event: &Value) -> Option<&str> {
    update_of(event, "thinking_delta")?.get("delta")?.as_str()
}

/// The provider stream's own error text from `message_update`.
pub fn stream_error(event: &Value) -> Option<String> {
    let update = update_of(event, "error")?;
    update
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| update.get("reason").and_then(Value::as_str))
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

fn update_of<'a>(event: &'a Value, kind: &str) -> Option<&'a Value> {
    if event["type"].as_str() != Some("message_update") {
        return None;
    }
    let update = &event["assistantMessageEvent"];
    (update["type"].as_str() == Some(kind)).then_some(update)
}

/// `(tool call id, tool name, arguments)` of a started tool.
pub fn tool_start(event: &Value) -> Option<(&str, &str, &Value)> {
    if event["type"].as_str() != Some("tool_execution_start") {
        return None;
    }
    let id = event["toolCallId"].as_str().filter(|id| !id.is_empty())?;
    let name = event["toolName"].as_str().unwrap_or("tool");
    Some((id, name, &event["args"]))
}

/// `(tool call id, partial result)` of a running tool.
pub fn tool_update(event: &Value) -> Option<(&str, &Value)> {
    if event["type"].as_str() != Some("tool_execution_update") {
        return None;
    }
    Some((event["toolCallId"].as_str().filter(|id| !id.is_empty())?, &event["partialResult"]))
}

/// `(tool call id, result, is_error)` of a finished tool.
pub fn tool_end(event: &Value) -> Option<(&str, &Value, bool)> {
    if event["type"].as_str() != Some("tool_execution_end") {
        return None;
    }
    let id = event["toolCallId"].as_str().filter(|id| !id.is_empty())?;
    Some((id, &event["result"], event["isError"].as_bool() == Some(true)))
}

/// A finished assistant message: its authoritative text, its stop reason, and
/// the provider's error text when the run failed.
#[derive(Debug, Clone, PartialEq)]
pub struct FinishedMessage {
    pub text: String,
    pub stop_reason: Option<String>,
    pub error: Option<String>,
}

/// `message_end` for an assistant message. A user or tool message answers `None`.
pub fn finished_message(event: &Value) -> Option<FinishedMessage> {
    if event["type"].as_str() != Some("message_end") {
        return None;
    }
    let message = &event["message"];
    if message["role"].as_str() != Some("assistant") {
        return None;
    }
    Some(FinishedMessage {
        text: content_text(&message["content"]),
        stop_reason: message["stopReason"].as_str().map(str::to_owned),
        error: message["errorMessage"]
            .as_str()
            .or_else(|| message["error"].as_str())
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned),
    })
}

/// The text of a message content value: a plain string, or the `text` blocks of
/// a block array. Thinking and tool-call blocks are not reply text.
pub fn content_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    let Some(blocks) = content.as_array() else { return String::new() };
    let mut text = String::new();
    for block in blocks {
        if block["type"].as_str() == Some("text") {
            if let Some(part) = block["text"].as_str() {
                text.push_str(part);
            }
        }
    }
    text
}

/// Whether `agent_settled` arrived, and whether it was an abort. This is the one
/// record that means "Pi will not continue on its own", so it ends the turn.
pub fn settled(event: &Value) -> Option<bool> {
    (event["type"].as_str() == Some("agent_settled")).then(|| event["aborted"].as_bool() == Some(true))
}

/// Whether a successful compaction finished during this turn.
pub fn compaction_finished(event: &Value) -> bool {
    event["type"].as_str() == Some("compaction_end") && event["result"].is_object()
}

/// Cumulative provider-reported usage for one assistant response, from either
/// `message_update.usage` or `get_session_stats.tokens`. Every bucket stays
/// `None` when Pi did not report it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub total: Option<u64>,
    /// Pi's own price estimate in US dollars, for the whole session.
    pub cost_usd: Option<f64>,
}

pub fn usage(value: &Value) -> Option<Usage> {
    if !value.is_object() {
        return None;
    }
    let usage = Usage {
        input: value["input"].as_u64(),
        output: value["output"].as_u64(),
        cache_read: value["cacheRead"].as_u64(),
        cache_write: value["cacheWrite"].as_u64(),
        total: value["totalTokens"].as_u64(),
        // `message_update.usage.cost` is an object; `get_session_stats.cost` is
        // one number for the whole session. Both are Pi's own USD estimate.
        cost_usd: value["cost"]["total"]
            .as_f64()
            .or_else(|| value["cost"].as_f64())
            .filter(|cost| cost.is_finite() && *cost >= 0.0),
    };
    (usage != Usage::default()).then_some(usage)
}

/// Context use and the window it is measured against, from
/// `get_session_stats.contextUsage`.
pub fn context_usage(value: &Value) -> (Option<u64>, Option<u64>) {
    let context = &value["contextUsage"];
    (context["tokens"].as_u64(), context["contextWindow"].as_u64())
}

/// The current session's id and how many messages it holds, from `get_state`.
pub fn state_of(data: &Value) -> (Option<String>, Option<u64>, Option<String>) {
    (
        data["sessionId"].as_str().map(str::to_owned),
        data["messageCount"].as_u64(),
        data["model"]["id"].as_str().map(str::to_owned),
    )
}

/// A model id as Retinue stores it for Pi: `provider/model`. Pi's own
/// `set_model` needs the two halves apart, and its `--model` accepts this form.
pub fn model_id(provider: &str, id: &str) -> String {
    format!("{provider}/{id}")
}

/// The two halves of a Pi model id. A value without a `/` cannot be sent to Pi,
/// so it answers `None` instead of guessing a provider.
pub fn split_model(value: &str) -> Option<(&str, &str)> {
    let (provider, id) = value.split_once('/')?;
    (!provider.is_empty() && !id.is_empty()).then_some((provider, id))
}

/// `get_available_models` as Retinue's catalogue. Models that do not take text
/// input are left out: Pi's catalogue also holds image, speech and classifier
/// models, and a coding agent conversation cannot run on those.
pub fn parse_models(data: &Value) -> Vec<ModelInfo> {
    let mut models = Vec::new();
    for model in data["models"].as_array().into_iter().flatten() {
        let (Some(provider), Some(id)) = (model["provider"].as_str(), model["id"].as_str()) else { continue };
        if provider.is_empty() || id.is_empty() || !takes_text(model) {
            continue;
        }
        let full = model_id(provider, id);
        if models.iter().any(|existing: &ModelInfo| existing.id == full) {
            continue;
        }
        let name = model["name"].as_str().filter(|name| !name.is_empty()).unwrap_or(id);
        let reasoning = model["reasoning"].as_bool() == Some(true);
        models.push(ModelInfo {
            id: full,
            display_name: name.to_owned(),
            provider_id: Some(provider.to_owned()),
            supported_thinking: if reasoning { THINKING_LEVELS.iter().map(|level| (*level).to_owned()).collect() } else { vec!["off".to_owned()] },
            default_thinking: None,
            service_tiers: Vec::<ServiceTier>::new(),
            default_service_tier: None,
            variants: None,
            host_dependent: true,
            is_default: None,
            group: Some(provider.to_owned()),
            availability: None,
            release_date: None,
            context_window: model["contextWindow"].as_u64(),
        });
    }
    // A stable order keeps `runtime.models.changed` about the models, not about
    // the order Pi happened to answer in.
    models.sort_by(|a, b| a.provider_id.cmp(&b.provider_id).then_with(|| a.display_name.cmp(&b.display_name)));
    models
}

/// A model with an `input` list that leaves text out is not a coding model.
fn takes_text(model: &Value) -> bool {
    match model["input"].as_array() {
        Some(input) => input.iter().any(|kind| kind.as_str() == Some("text")),
        None => true,
    }
}

/// A session id Pi accepts for `--session-id`: it must start and end with a
/// letter or number and may hold letters, numbers, `.`, `_` and `-`. Retinue's
/// own ids are made safe here once, so a resume names the same session.
pub fn pi_session_id(local: &str) -> String {
    let mut id = String::with_capacity(local.len().min(96));
    for ch in local.chars() {
        let kept = if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') { ch } else { '-' };
        if id.len() >= 96 {
            break;
        }
        id.push(kept);
    }
    let trimmed = id.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
    if trimmed.is_empty() {
        "retinue-session".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(value: Value) -> String {
        serde_json::to_string(&value).unwrap()
    }

    #[test]
    fn a_command_repeats_its_id_and_its_own_fields() {
        let record = command("req-1", "prompt", json!({"message": "Hello"}));
        assert_eq!(record["id"], "req-1");
        assert_eq!(record["type"], "prompt");
        assert_eq!(record["message"], "Hello");
    }

    #[test]
    fn a_response_and_an_event_are_told_apart_by_type() {
        let response = parse_record(&line(json!({"id": "1", "type": "response", "command": "get_state", "success": true, "data": {"sessionId": "s1"}}))).unwrap();
        match response {
            Record::Response(response) => {
                assert_eq!(response.id.as_deref(), Some("1"));
                assert_eq!(response.command, "get_state");
                assert!(response.success);
                assert_eq!(response.data["sessionId"], "s1");
                assert!(response.error.is_none());
            }
            other => panic!("expected a response, got {other:?}"),
        }
        let event = parse_record(&line(json!({"type": "agent_settled", "aborted": false}))).unwrap();
        assert!(matches!(event, Record::Event(_)));
    }

    #[test]
    fn a_failed_response_keeps_its_error_text_and_ignores_unknown_lines() {
        let failed = parse_record(&line(json!({"type": "response", "command": "set_model", "success": false, "error": "Model not found"}))).unwrap();
        match failed {
            Record::Response(response) => {
                assert!(response.id.is_none());
                assert!(!response.success);
                assert_eq!(response.error.as_deref(), Some("Model not found"));
            }
            other => panic!("expected a response, got {other:?}"),
        }
        assert!(parse_record("not json at all").is_none());
        assert!(parse_record("").is_none());
        assert!(parse_record("[1,2,3]").is_none(), "an array is not a record");
        assert!(parse_record(&format!("{{\"type\":\"{}\"}}", "x".repeat(MAX_RECORD_BYTES))).is_none());
    }

    #[test]
    fn deltas_come_only_from_the_matching_stream_update() {
        let text = json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "contentIndex": 0, "delta": "Hello"}});
        assert_eq!(text_delta(&text), Some("Hello"));
        assert_eq!(thinking_delta(&text), None);
        let thinking = json!({"type": "message_update", "assistantMessageEvent": {"type": "thinking_delta", "contentIndex": 0, "delta": "hm"}});
        assert_eq!(thinking_delta(&thinking), Some("hm"));
        assert_eq!(text_delta(&thinking), None);
        assert_eq!(text_delta(&json!({"type": "message_end"})), None);
    }

    #[test]
    fn a_provider_stream_error_is_read_from_the_error_update() {
        let event = json!({"type": "message_update", "assistantMessageEvent": {"type": "error", "reason": "overloaded", "error": "529 overloaded"}});
        assert_eq!(stream_error(&event).as_deref(), Some("529 overloaded"));
        let reason_only = json!({"type": "message_update", "assistantMessageEvent": {"type": "error", "reason": "aborted"}});
        assert_eq!(stream_error(&reason_only).as_deref(), Some("aborted"));
        assert!(stream_error(&json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "delta": "x"}})).is_none());
    }

    #[test]
    fn tool_lifecycle_records_keep_their_call_id() {
        let start = json!({"type": "tool_execution_start", "toolCallId": "call_1", "toolName": "bash", "args": {"command": "ls -la"}});
        let (id, name, args) = tool_start(&start).unwrap();
        assert_eq!((id, name), ("call_1", "bash"));
        assert_eq!(args["command"], "ls -la");
        assert!(tool_start(&json!({"type": "tool_execution_start", "toolName": "bash"})).is_none(), "a call with no id is not a row");

        let update = json!({"type": "tool_execution_update", "toolCallId": "call_1", "partialResult": {"content": []}});
        assert_eq!(tool_update(&update).unwrap().0, "call_1");

        let end = json!({"type": "tool_execution_end", "toolCallId": "call_1", "result": {"content": []}, "isError": true});
        let (id, _, failed) = tool_end(&end).unwrap();
        assert_eq!(id, "call_1");
        assert!(failed);
    }

    #[test]
    fn a_finished_assistant_message_is_its_authoritative_text() {
        let event = json!({"type": "message_end", "message": {"role": "assistant", "content": [{"type": "thinking", "thinking": "hm"}, {"type": "text", "text": "Done"}, {"type": "text", "text": "."}], "stopReason": "stop"}});
        let finished = finished_message(&event).unwrap();
        assert_eq!(finished.text, "Done.");
        assert_eq!(finished.stop_reason.as_deref(), Some("stop"));
        assert!(finished.error.is_none());

        let failed = json!({"type": "message_end", "message": {"role": "assistant", "content": "partial", "stopReason": "error", "errorMessage": "Provider returned 401"}});
        let finished = finished_message(&failed).unwrap();
        assert_eq!(finished.text, "partial");
        assert_eq!(finished.error.as_deref(), Some("Provider returned 401"));

        assert!(finished_message(&json!({"type": "message_end", "message": {"role": "user", "content": "hi"}})).is_none());
        assert!(finished_message(&json!({"type": "message_start", "message": {"role": "assistant"}})).is_none());
    }

    #[test]
    fn the_turn_ends_only_on_agent_settled() {
        assert_eq!(settled(&json!({"type": "agent_settled", "aborted": false})), Some(false));
        assert_eq!(settled(&json!({"type": "agent_settled", "aborted": true})), Some(true));
        assert_eq!(settled(&json!({"type": "agent_end"})), None, "agent_end is not the end: a retry can follow");
    }

    #[test]
    fn compaction_is_recognised_only_when_it_produced_a_result() {
        assert!(compaction_finished(&json!({"type": "compaction_end", "reason": "threshold", "result": {"summary": "s"}})));
        assert!(!compaction_finished(&json!({"type": "compaction_end", "reason": "threshold", "aborted": true})));
        assert!(!compaction_finished(&json!({"type": "compaction_start", "reason": "threshold"})));
    }

    #[test]
    fn usage_keeps_unknown_buckets_unknown() {
        let reported = json!({"input": 100, "output": 5, "cacheRead": 0, "totalTokens": 105, "cost": {"input": 0.001, "output": 0.002, "total": 0.003}});
        let first = usage(&reported).unwrap();
        assert_eq!(first.input, Some(100));
        assert_eq!(first.cache_write, None, "Pi did not report a cache write");
        assert_eq!(first.cost_usd, Some(0.003));
        assert!(usage(&json!({"input": 0})).is_some(), "one real bucket is a report");
        assert!(usage(&Value::Null).is_none());
        assert!(usage(&json!({"cost": {"total": -1.0}})).is_none(), "a negative price is not a price");
        // `get_session_stats` reports one number for the session's cost.
        assert_eq!(usage(&json!({"tokens": {"input": 1}, "cost": 0.45})).unwrap().cost_usd, Some(0.45));
        assert_eq!(usage(&json!({"input": 1, "cost": 0.45})).unwrap().input, Some(1));
    }

    #[test]
    fn state_context_and_models_are_read_without_guessing() {
        let state = json!({"sessionId": "abc", "messageCount": 4, "model": {"id": "claude-sonnet-4"}});
        let (id, count, model) = state_of(&state);
        assert_eq!((id.as_deref(), count, model.as_deref()), (Some("abc"), Some(4), Some("claude-sonnet-4")));

        let (used, window) = context_usage(&json!({"contextUsage": {"tokens": 60000, "contextWindow": 200000, "percent": 30}}));
        assert_eq!((used, window), (Some(60000), Some(200000)));
        assert_eq!(context_usage(&json!({})), (None, None), "no model means no context measurement");

        let data = json!({"models": [
            {"id": "claude-sonnet-4", "name": "Claude Sonnet 4", "provider": "anthropic", "reasoning": true, "input": ["text", "image"], "contextWindow": 200000},
            {"id": "claude-sonnet-4", "name": "Claude Sonnet 4 again", "provider": "anthropic", "input": ["text"]},
            {"id": "gpt-image-1", "name": "Image", "provider": "openai", "input": ["image"]},
            {"id": "glm-5.2", "name": "GLM", "provider": "opencode-go", "reasoning": false, "input": ["text"]},
            {"name": "no id", "provider": "anthropic"}
        ]});
        let models = parse_models(&data);
        assert_eq!(models.iter().map(|model| model.id.as_str()).collect::<Vec<_>>(), ["anthropic/claude-sonnet-4", "opencode-go/glm-5.2"]);
        assert_eq!(models[0].display_name, "Claude Sonnet 4", "the first entry wins, not the repeat");
        assert_eq!(models[0].context_window, Some(200000));
        assert!(models[0].supported_thinking.contains(&"high".to_owned()));
        assert_eq!(models[1].supported_thinking, ["off"], "a model that does not reason takes one level");
        assert!(models.iter().all(|model| model.service_tiers.is_empty()), "Pi reports no service tiers");
    }

    #[test]
    fn a_model_id_carries_the_provider_pi_needs_back() {
        assert_eq!(model_id("anthropic", "claude-sonnet-4"), "anthropic/claude-sonnet-4");
        assert_eq!(split_model("anthropic/claude-sonnet-4"), Some(("anthropic", "claude-sonnet-4")));
        assert_eq!(split_model("openrouter/anthropic/claude-4"), Some(("openrouter", "anthropic/claude-4")), "the first slash splits");
        assert_eq!(split_model("claude-sonnet-4"), None);
        assert_eq!(split_model("/model"), None);
        assert_eq!(split_model("provider/"), None);
    }

    #[test]
    fn a_session_id_is_safe_for_pi_and_names_the_same_session_again() {
        assert_eq!(pi_session_id("ses_ab-12"), "ses_ab-12");
        assert_eq!(pi_session_id("session/with:odd chars"), "session-with-odd-chars");
        assert_eq!(pi_session_id("--8c3f--"), "8c3f", "Pi needs a letter or number at both ends");
        assert_eq!(pi_session_id(""), "retinue-session");
        assert_eq!(pi_session_id("!!!"), "retinue-session");
        assert_eq!(pi_session_id(&"a".repeat(400)).len(), 96);
    }
}
