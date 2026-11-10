//! Owner request of 9 November 2026: ask the CEO to troubleshoot a failed turn,
//! and let the CEO offer to open a GitHub issue.
//!
//! `manager.troubleshoot { sessionId, turnId? }` writes a report from facts only
//! (the employee, the coding agent, the failure class, the public error text, the
//! last tool names and states, the app version) and sends it as the first message of
//! a NEW conversation with the CEO. It carries no message text and no tool output,
//! and everything passes through `issue_report::clean`.
//!
//! `open_issue { title, body }` (CEO only) never posts. It makes an approval card
//! (a `task` proposal with `action: "issue"`, so no storage change is needed); the
//! post happens only when the owner approves, through the same GitHub CLI path as
//! `diagnostics.issuePost`, or the browser address is returned when the CLI is not
//! signed in.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use serde_json::{json, Value};

const MAX_ISSUE_TITLE: usize = 200;
const MAX_ISSUE_BODY: usize = 8_000;
const MAX_TOOLS: usize = 3;

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// The turn to report on: the one asked for, else the session's latest failed one.
pub(crate) fn pick_turn(session: &Value, wanted: Option<&str>) -> Result<Value, DispatchError> {
    let turns = session["turns"].as_array().cloned().unwrap_or_default();
    let found = match wanted {
        Some(id) => turns.into_iter().find(|turn| turn["id"].as_str() == Some(id)),
        None => turns.into_iter().rev().find(|turn| turn["state"].as_str() == Some("error")),
    };
    found.ok_or_else(|| derr("not_found", "There is no failed turn to look into in that conversation.", StatusCode::NOT_FOUND))
}

/// Facts only: no message text, no tool output. Every line is cleaned again at the end.
pub(crate) async fn report(st: &AppState, session: &Value, turn: &Value) -> String {
    let turn_id = turn["id"].as_str().unwrap_or("");
    let agent = session["agentId"].as_str().and_then(|id| st.db.agent_get(id).ok()).unwrap_or(Value::Null);
    let name = agent["name"].as_str().unwrap_or("an employee");
    let role = agent["role"].as_str().filter(|role| !role.trim().is_empty());
    let runtimes = st.runtimes.read().await.clone();
    let runtime = session["runtimeId"].as_str().and_then(|id| runtimes.iter().find(|row| row["id"].as_str() == Some(id)));
    let provider = session["provider"].as_str().or_else(|| runtime.and_then(|row| row["provider"].as_str())).unwrap_or("unknown");
    let version = runtime.and_then(|row| row["version"].as_str()).filter(|version| !version.is_empty()).unwrap_or("unknown");
    let model = session["contextModel"].as_str().or_else(|| agent["model"].as_str()).filter(|model| !model.is_empty()).unwrap_or("default");
    let project = session["projectId"].as_str().and_then(|id| st.db.project_get(id).ok()).and_then(|row| row["name"].as_str().map(str::to_owned))
        .or_else(|| session["workspaceLabel"].as_str().map(str::to_owned)).unwrap_or_else(|| "none".into());
    let class = turn["failureClass"].as_str().unwrap_or("unknown");
    let data = st.db.turn_data(turn_id).unwrap_or(Value::Null);
    let mut lines: Vec<String> = vec!["A turn failed in this conversation. Facts only; no message text or tool output is included.".into(), String::new()];
    lines.push(format!("- Employee: {name}{}", role.map(|role| format!(" ({role})")).unwrap_or_default()));
    lines.push(format!("- Coding agent: {provider} {version}"));
    lines.push(format!("- Model: {model}"));
    lines.push(format!("- Project: {project}"));
    lines.push(format!("- Conversation id: {}", session["id"].as_str().unwrap_or("")));
    lines.push(format!("- Turn id: {turn_id}"));
    lines.push(format!("- Failure class: {class}"));
    if let Some(message) = turn["failureMessage"].as_str().filter(|text| !text.is_empty()) { lines.push(format!("- Error shown to the owner: {}", one_line(message))); }
    let detail = match &turn["detail"] { Value::String(text) => Some(text.clone()), Value::Null => None, other => Some(other.to_string()) };
    if let Some(detail) = detail.filter(|text| !text.trim().is_empty()) { lines.push(format!("- Detail: {}", clip(&one_line(&detail), 240))); }
    if let Some(kind) = data["hint"]["kind"].as_str() {
        match data["hint"]["retryAfterSecs"].as_i64() {
            Some(seconds) => lines.push(format!("- Hint: {kind}, retry after {seconds} s")),
            None => lines.push(format!("- Hint: {kind}")),
        }
    }
    if let Some(decision) = data["retryDecision"]["decision"].as_str() {
        match data["retryDecision"]["reason"].as_str() {
            Some(reason) => lines.push(format!("- Automatic retry: {decision} ({reason})")),
            None => lines.push(format!("- Automatic retry: {decision}")),
        }
    }
    if let Some(at) = turn["completedAt"].as_str().or_else(|| turn["createdAt"].as_str()) { lines.push(format!("- When: {at}")); }
    lines.push(format!("- Retinue version: {}", env!("CARGO_PKG_VERSION")));
    let tools: Vec<&Value> = session["tools"].as_array().map(|tools| tools.iter().filter(|tool| tool["turnId"].as_str() == Some(turn_id)).collect()).unwrap_or_default();
    if !tools.is_empty() {
        lines.push(String::new());
        lines.push("Last tool calls in that turn (name and state only):".into());
        let skip = tools.len().saturating_sub(MAX_TOOLS);
        for tool in tools.into_iter().skip(skip) {
            let label = tool["title"].as_str().filter(|title| !title.trim().is_empty()).or_else(|| tool["kind"].as_str()).unwrap_or("tool");
            lines.push(format!("- {}: {}", clip(&one_line(label), 60), tool["state"].as_str().unwrap_or("unknown")));
        }
    }
    lines.push(String::new());
    lines.push("Please investigate the cause. You may read the failing conversation with your tools, and read the logs and diagnostics you can reach. Explain what went wrong in plain words. Propose a fix, or make one only with my approval. If it looks like a bug in Retinue itself, offer to open a GitHub issue with `open_issue`; I will approve the exact text first.".into());
    crate::issue_report::clean(&lines.join("\n"))
}

/// `manager.troubleshoot { sessionId, turnId? }` returns `{sessionId}` of the new CEO conversation.
pub(crate) async fn rpc(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let session_id = p["sessionId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("sessionId is required."))?;
    let wanted = match p.get("turnId") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("turnId must be a string."))?),
    };
    let manager = st.db.manager_agent().ok().flatten().filter(|agent| agent["archived"] != true)
        .ok_or_else(|| derr("conflict", "There is no CEO yet. Set up a CEO first, then ask them to troubleshoot.", StatusCode::CONFLICT))?;
    let session = st.db.session_detail(session_id).map_err(|_| derr("not_found", "That conversation is no longer available.", StatusCode::NOT_FOUND))?;
    let turn = pick_turn(&session, wanted)?;
    let text = report(st, &session, &turn).await;
    let employee = session["agentId"].as_str().and_then(|id| st.db.agent_get(id).ok()).and_then(|agent| agent["name"].as_str().map(str::to_owned)).unwrap_or_else(|| "employee".into());
    let short = turn["detail"].as_str().filter(|text| !text.trim().is_empty()).map(one_line).or_else(|| turn["failureMessage"].as_str().map(one_line)).unwrap_or_else(|| "error".into());
    let title = crate::issue_report::clean(&format!("Troubleshoot: {employee} {}", clip(&short, 40)));
    let mut params = json!({"agentId": manager["id"], "title": title.trim()});
    if let Some(project) = session["projectId"].as_str().filter(|id| !id.is_empty() && st.db.project_get(id).is_ok_and(|row| row["path"].as_str().is_some_and(|path| std::path::Path::new(path).is_dir()))) {
        params["projectId"] = json!(project);
    }
    let created = crate::new_session_with(st, params, None).await?;
    let new_id = created["id"].as_str().unwrap_or("").to_owned();
    crate::session_prompt(st, json!({"sessionId": &new_id, "text": text})).await?;
    Ok(json!({"sessionId": new_id}))
}

// ---------------------------------------------------------------------------
// open_issue

pub fn tool_schema() -> Value {
    json!({"name":"open_issue","description":"Offer to open a GitHub issue for a bug in Retinue itself. It never posts: the owner sees a card with the exact title and text and decides. Write the body from facts you have (what happened, the version, the steps); never include secrets, personal data or message content.","inputSchema":{"type":"object","properties":{
        "title":{"type":"string","description":"A short issue title, 1 to 200 characters."},
        "body":{"type":"string","description":"The issue text in Markdown, up to 8000 characters."}
    },"required":["title","body"],"additionalProperties":false}})
}

pub(crate) async fn open_issue_tool(st: &AppState, session_id: &str, args: &Value) -> Result<String, String> {
    let manager = crate::proposals::manager_of_session(st, session_id)?;
    let map = args.as_object().ok_or("The issue must be an object.")?;
    if let Some(key) = map.keys().find(|key| !matches!(key.as_str(), "title" | "body")) { return Err(format!("The issue has an unsupported field: {key}.")); }
    let title = map.get("title").and_then(Value::as_str).map(|text| one_line(&crate::issue_report::clean(text))).filter(|text| !text.is_empty()).ok_or("title is required.")?;
    if title.chars().count() > MAX_ISSUE_TITLE { return Err(format!("title must be at most {MAX_ISSUE_TITLE} characters.")); }
    let body = map.get("body").and_then(Value::as_str).map(|text| crate::issue_report::clean(text.trim())).filter(|text| !text.is_empty()).ok_or("body is required.")?;
    if body.chars().count() > MAX_ISSUE_BODY { return Err(format!("body must be at most {MAX_ISSUE_BODY} characters.")); }
    let payload = json!({"action":"issue","title":title,"body":body});
    let id = crate::proposals::create_proposal(st, session_id, &manager, "task", payload, format!("Open a GitHub issue: {title}"), Some(body)).await?;
    Ok(format!("The issue card is waiting for the owner's approval (proposal {id}). Nothing is posted until they approve it; the outcome arrives here as a note. Tell the user the card is ready and end your turn."))
}

/// An approved issue: the GitHub CLI posts it, or the new-issue address is returned
/// for the owner's browser when the CLI is missing or not signed in.
pub(crate) async fn apply_issue(st: &AppState, payload: &Value) -> Result<(Value, String), DispatchError> {
    let map = payload.as_object().ok_or_else(|| invalid("The issue proposal is empty."))?;
    if let Some(key) = map.keys().find(|key| !matches!(key.as_str(), "action" | "title" | "body")) { return Err(invalid(&format!("The issue proposal has an unsupported field: {key}."))); }
    let title = payload["title"].as_str().map(str::trim).filter(|text| !text.is_empty() && text.chars().count() <= MAX_ISSUE_TITLE).ok_or_else(|| invalid("The title must be 1 to 200 characters."))?;
    let body = payload["body"].as_str().filter(|text| text.chars().count() <= MAX_ISSUE_BODY).ok_or_else(|| invalid("The issue text is longer than 8000 characters."))?;
    match crate::issue_report::post(st, &json!({"title": title, "body": body, "confirm": true})).await {
        Ok(result) => {
            let url = result["url"].as_str().unwrap_or("").to_owned();
            Ok((json!({"posted": true, "url": url}), format!("opened the GitHub issue {url}")))
        }
        Err(error) if error.1.code == "unsupported" => {
            let url = crate::issue_report::new_issue_url(&one_line(&crate::issue_report::clean(title)), &crate::issue_report::clean(body));
            Ok((json!({"posted": false, "url": &url}), format!("the GitHub CLI is not signed in, so nothing was posted; the owner can open this address to post it: {url}")))
        }
        Err(error) => Err(error),
    }
}
