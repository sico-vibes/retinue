//! A plan that outlives one turn (wave 2, nod idea 4).
//!
//! The bot's own to-do list already arrives as `AgentEvent::PlanUpdated` and is
//! stored on the turn (`turns.data.plan`), so it is visible while that turn is
//! open and gone once another turn is. Multi-turn work — plan, ask, continue —
//! is exactly when the list matters most, so this module keeps a copy on the
//! conversation (`sessions.data.plan`) and hands it back to the next turn.
//!
//! What it deliberately does not do:
//!
//! - It never invents progress. The list is always the bot's own, item for item.
//! - It never outlives a replacement: a new plan replaces it whole, an empty list
//!   clears it.
//! - It is never a claim about the machine. The window labels a list from an
//!   earlier turn with the turn it came from, and the transcript's own rows stay
//!   the evidence of what ran.

use crate::AppState;
use serde_json::{json, Value};

/// Most items carried into a prompt. The stored list keeps the adapter's own cap.
pub const CARRIED_ITEMS: usize = 20;
/// Character cap on the carried block.
pub const CARRIED_CHAR_CAP: usize = 800;

/// Writes the plan on the conversation as well as on the turn. Called from the
/// `PlanUpdated` arm with the already-normalized items.
pub fn record(st: &AppState, session_id: &str, turn: Option<&str>, items: &[Value]) {
    let Some(turn) = turn.filter(|turn| !turn.is_empty()) else { return };
    let plan = if items.is_empty() {
        Value::Null
    } else {
        json!({
            "items": items,
            "updatedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "turnId": turn,
        })
    };
    let _ = st.db.merge_session_data(session_id, &json!({"plan": plan}));
}

/// The block that carries an earlier turn's plan into this one. `None` when the
/// conversation has no plan, the plan came from this very turn, or nothing is
/// left to do.
pub fn carried_plan_for_prompt(st: &AppState, session_id: &str, current_turn: &str) -> Option<String> {
    let plan = st.db.session_plan(session_id).ok()??;
    let from = plan["turnId"].as_str().unwrap_or("");
    if from.is_empty() || from == current_turn { return None; }
    let items = plan["items"].as_array()?;
    let unfinished = items.iter().any(|item| item["status"].as_str() != Some("completed"));
    if !unfinished { return None; }
    let header = "Your plan from an earlier turn in this conversation. Continue it or replace it with a new one:\n\n<retinue_plan>";
    let close = "\n</retinue_plan>";
    let mut total = header.chars().count() + close.chars().count() + 1;
    let mut lines = Vec::new();
    for item in items.iter().take(CARRIED_ITEMS) {
        let mark = match item["status"].as_str() {
            Some("completed") => "[x]",
            Some("in_progress") => "[~]",
            _ => "[ ]",
        };
        let line = format!("{mark} {}", item["text"].as_str().unwrap_or(""));
        let length = line.chars().count() + 1;
        if total + length > CARRIED_CHAR_CAP { break; }
        total += length;
        lines.push(line);
    }
    if lines.is_empty() { return None; }
    Some(format!("{header}\n{}{close}", lines.join("\n")))
}
