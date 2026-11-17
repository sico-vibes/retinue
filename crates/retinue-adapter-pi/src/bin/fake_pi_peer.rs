//! Test-only Pi RPC peer. Not a production entrypoint.
//!
//! It answers the commands `retinue-adapter-pi` sends and streams the records a
//! real `pi --mode rpc` process sends, so the adapter's turns, tool rows, usage
//! and cancellation are tested without Pi installed. The shapes it prints are
//! the ones in `@earendil-works/pi-coding-agent` 1.5.2's `docs/rpc-commands.md`
//! and `docs/json.md`.
//!
//! Scenarios (`--scenario <name>`): `stream` (default), `tool`, `error`,
//! `refuse`, `hang`, `exit`, `emptyhistory`.
//! `--record <path>` appends every command line it receives; `--record-env
//! <path>` writes one line per variable it was started with, as `NAME=set:len`
//! or `NAME=absent` for the key variables a test checks, so no value is ever
//! written to disk.

use serde_json::{json, Value};
use std::{
    env,
    fs::OpenOptions,
    io::{BufRead, Write},
    path::PathBuf,
};

const KEY_VARIABLES: [&str; 3] = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "RETINUE_MANAGED"];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version") {
        println!("1.5.2");
        return;
    }
    if args.iter().any(|arg| arg == "--help") {
        println!("Usage: pi [options] [messages...]");
        println!("  --mode <mode>                  Select text, json or rpc output");
        println!("  --session-id <id>              Open or create a session with this id");
        println!("  --append-system-prompt <path>  Append to the system prompt");
        return;
    }
    let flag = |name: &str| args.iter().position(|arg| arg == name).and_then(|index| args.get(index + 1)).cloned();
    let session_id = flag("--session-id").unwrap_or_else(|| "fake-session".into());
    let scenario = flag("--scenario").unwrap_or_else(|| "stream".into());
    let record = flag("--record").map(PathBuf::from);
    if let Some(path) = flag("--record-env").map(PathBuf::from) {
        let mut lines = format!("ARGS={}\n", args.join(" "));
        for name in KEY_VARIABLES {
            match env::var(name) {
                Ok(value) => lines.push_str(&format!("{name}=set:{}\n", value.len())),
                Err(_) => lines.push_str(&format!("{name}=absent\n")),
            }
        }
        let _ = std::fs::write(path, lines);
    }

    let stdin = std::io::stdin();
    let mut abort_pending = false;
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if let Some(path) = &record {
            let _ = OpenOptions::new().create(true).append(true).open(path).and_then(|mut file| writeln!(file, "{line}"));
        }
        let Ok(command) = serde_json::from_str::<Value>(&line) else { continue };
        let id = command["id"].as_str().unwrap_or("").to_owned();
        let kind = command["type"].as_str().unwrap_or("");
        match kind {
            "get_state" => respond(
                &id,
                "get_state",
                json!({
                    "sessionId": session_id,
                    "thinkingLevel": "medium",
                    "isStreaming": false,
                    "messageCount": if scenario == "emptyhistory" { 0 } else { 1 },
                    "model": {"id": "claude-sonnet-4", "provider": "anthropic", "name": "Claude Sonnet 4"}
                }),
            ),
            "get_available_models" => respond(
                &id,
                "get_available_models",
                json!({"models": [
                    {"id": "claude-sonnet-4", "name": "Claude Sonnet 4", "provider": "anthropic", "reasoning": true, "input": ["text", "image"], "contextWindow": 200000},
                    {"id": "glm-5.2", "name": "GLM 5.2", "provider": "opencode-go", "reasoning": false, "input": ["text"], "contextWindow": 128000}
                ]}),
            ),
            "set_model" => respond(
                &id,
                "set_model",
                json!({
                    "id": command["modelId"],
                    "provider": command["provider"],
                    "name": command["modelId"],
                    "reasoning": true,
                    "input": ["text"],
                    "contextWindow": 200000
                }),
            ),
            "set_thinking_level" => respond(&id, "set_thinking_level", json!({})),
            "get_session_stats" => respond(
                &id,
                "get_session_stats",
                json!({
                    "tokens": {"input": 100, "output": 10, "cacheRead": 0, "cacheWrite": 5, "total": 115},
                    "cost": 0.0012,
                    "contextUsage": {"tokens": 60000, "contextWindow": 200000, "percent": 30}
                }),
            ),
            "steer" => respond(&id, "steer", json!({"disposition": "queued"})),
            "abort" => {
                respond(&id, "abort", json!({}));
                if abort_pending {
                    abort_pending = false;
                    event(json!({"type": "agent_settled", "aborted": true}));
                }
            }
            "prompt" => {
                if scenario == "refuse" {
                    response(&id, "prompt", false, Value::Null, Some("Pi is not signed in to that provider."));
                    continue;
                }
                respond(&id, "prompt", json!({"disposition": "started"}));
                match scenario.as_str() {
                    "hang" => {
                        abort_pending = true;
                    }
                    "exit" => return,
                    "error" => {
                        event(json!({"type": "message_start", "message": {"role": "assistant", "content": []}}));
                        event(json!({"type": "message_update", "assistantMessageEvent": {"type": "error", "reason": "401", "error": "Provider returned 401 for key sk-test-0123456789abcdefghijklmnopqrstuvwxyz"}}));
                        event(json!({"type": "message_end", "message": {"role": "assistant", "content": "I could not start.", "stopReason": "error", "errorMessage": "Provider returned 401 for key sk-test-0123456789abcdefghijklmnopqrstuvwxyz"}}));
                        event(json!({"type": "agent_end", "messages": [], "willRetry": false}));
                        event(json!({"type": "agent_settled", "aborted": false}));
                    }
                    "tool" => {
                        stream_turn(true);
                    }
                    _ => {
                        stream_turn(false);
                    }
                }
            }
            _ => response(&id, kind, false, Value::Null, Some("Unknown command")),
        }
    }
}

fn stream_turn(tool: bool) {
    event(json!({"type": "agent_start"}));
    event(json!({"type": "message_start", "message": {"role": "assistant", "content": []}}));
    event(json!({"type": "message_update", "usage": {"input": 100, "output": 1, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 101, "cost": {"total": 0.0}}, "assistantMessageEvent": {"type": "thinking_delta", "contentIndex": 0, "delta": "Considering"}}));
    event(json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "contentIndex": 1, "delta": "Hello"}}));
    if tool {
        event(json!({"type": "tool_execution_start", "toolCallId": "call_1", "toolName": "bash", "args": {"command": "ls -la"}}));
        event(json!({"type": "tool_execution_update", "toolCallId": "call_1", "toolName": "bash", "args": {"command": "ls -la"}, "partialResult": {"content": []}}));
        event(json!({"type": "tool_execution_end", "toolCallId": "call_1", "toolName": "bash", "result": {"content": [{"type": "text", "text": "total 0"}]}, "isError": false, "durationMs": 12}));
    }
    event(json!({"type": "message_update", "usage": {"input": 100, "output": 4, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 104, "cost": {"total": 0.0021}}, "assistantMessageEvent": {"type": "text_delta", "contentIndex": 1, "delta": " there"}}));
    event(json!({"type": "message_end", "message": {"role": "assistant", "content": [{"type": "text", "text": "Hello there"}], "stopReason": "stop"}}));
    event(json!({"type": "agent_end", "messages": [], "willRetry": false}));
    event(json!({"type": "agent_settled", "aborted": false}));
}

fn respond(id: &str, command: &str, data: Value) {
    response(id, command, true, data, None);
}

fn response(id: &str, command: &str, success: bool, data: Value, error: Option<&str>) {
    let mut record = json!({"type": "response", "command": command, "success": success});
    if !id.is_empty() {
        record["id"] = json!(id);
    }
    if !data.is_null() {
        record["data"] = data;
    }
    if let Some(error) = error {
        record["error"] = json!(error);
    }
    emit(record);
}

fn event(record: Value) {
    emit(record);
}

fn emit(record: Value) {
    let text = serde_json::to_string(&record).expect("a record is JSON");
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{text}");
    let _ = stdout.flush();
}
