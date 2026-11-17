//! Wave 2 (nod adoption, 17 November 2026): what a bot can reach, in plain words.
//!
//! A bot is not sandboxed: it is the owner's own coding-agent CLI, running as the
//! owner's Windows account, in a folder on the owner's disk, with the owner's
//! network and the tools they have installed. nod is honest about its trust
//! boundary in one table (`01-sandbox.md`, idea 1); Retinue had that knowledge
//! only in the threat model and in an engineer's head. This module turns it into
//! one sentence and a few rows the window can show, per conversation or per bot.
//!
//! Everything here is a read of what is already stored and configured. Nothing is
//! probed, nothing is started, and nothing is said that the daemon has not been
//! told: an unknown folder reads as unknown, never as a guess.
//!
//! Desktop only: the reply names the host's folder, which is why its row in
//! `tiers.json` is `never`.

use crate::AppState;
use retinue_agent_core::ApprovalMode;
use serde_json::{json, Value};

/// `reach.statement`: the sentence and rows for one conversation, or for a bot
/// when no conversation is named.
pub(crate) async fn rpc(st: &AppState, p: &Value) -> Result<Value, crate::DispatchError> {
    use axum::http::StatusCode;
    let session_id = p["sessionId"].as_str().filter(|id| !id.is_empty());
    let agent_id = p["agentId"].as_str().filter(|id| !id.is_empty());
    if session_id.is_none() && agent_id.is_none() {
        return Err(crate::derr("invalid_argument", "sessionId or agentId is required", StatusCode::BAD_REQUEST));
    }
    let session = match session_id {
        Some(id) => Some(st.db.session_execution_row(id).map_err(|_| crate::derr("not_found", "session not found", StatusCode::NOT_FOUND))?),
        None => None,
    };
    let agent = match agent_id.or_else(|| session.as_ref().and_then(|row| row["agentId"].as_str())) {
        Some(id) => st.db.agent_get(id).ok(),
        None => None,
    };
    Ok(json!({ "reach": reach(st, session.as_ref(), agent.as_ref()) }))
}

/// Everything the answer is built from. `approval_mode_for_session` is the one
/// place the mode is decided, so the sentence and the launch cannot drift.
struct Facts {
    bot_name: String,
    session_id: Option<String>,
    agent_id: Option<String>,
    folder: Option<String>,
    isolation: &'static str,
    mode: ApprovalMode,
    unattended: bool,
    computer: bool,
}

fn reach(st: &AppState, session: Option<&Value>, agent: Option<&Value>) -> Value {
    let link = session.map(|row| row["link"].clone()).unwrap_or(Value::Null);
    let facts = Facts {
        bot_name: agent.and_then(|agent| agent["name"].as_str()).unwrap_or("This bot").to_owned(),
        session_id: session.and_then(|row| row["id"].as_str()).map(str::to_owned),
        agent_id: agent.and_then(|agent| agent["id"].as_str()).map(str::to_owned),
        folder: session
            .and_then(|row| row["projectPath"].as_str())
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .or_else(|| agent.map(|agent| crate::team::workspace_for(st, agent).to_string_lossy().into_owned())),
        isolation: session.map(isolation_of).unwrap_or("unknown"),
        // The mode follows the same order the daemon launches with: the project's
        // mode, then the bot's, then the default, then the conversation's own pin.
        mode: session.map(|row| crate::approval_mode_for_session(st, row)).unwrap_or_default(),
        unattended: crate::is_unattended(&link),
        computer: agent.is_some_and(|agent| crate::computer::workplace_instructions(st, agent).is_some()),
    };
    answer(&facts)
}

/// The reply's own body: one sentence, the rows under it, and the limits.
fn answer(facts: &Facts) -> Value {
    let bot = facts.bot_name.as_str();
    let statement = match (facts.folder.as_deref(), facts.isolation) {
        (Some(folder), "isolated") => format!("{bot} runs as you, as your own Windows account, in its own copy of your project at {folder}, with your network and the tools you have installed."),
        (Some(folder), _) => format!("{bot} runs as you, as your own Windows account, in {folder}, with your network and the tools you have installed."),
        (None, _) => format!("{bot} runs as you, as your own Windows account, on this computer, with your network and the tools you have installed."),
    };
    let permission_sentence = match facts.mode {
        ApprovalMode::Ask => "Every tool call asks you first.",
        ApprovalMode::Auto => "Reads and safe commands run on their own; anything else asks you first.",
        ApprovalMode::AcceptEdits => "Edits and reads run on their own; anything else asks you first.",
        ApprovalMode::Bypass => "Full access: nothing asks.",
    };
    let mut limits = vec![
        "It runs as your own Windows account, so it can change anything you can.".to_owned(),
        permission_sentence.to_owned(),
        "The coding agent's own sandbox and sign-in are its own, not Retinue's.".to_owned(),
    ];
    match facts.isolation {
        "isolated" => limits.push("It works in its own copy of the repository, and merging that copy back is your decision.".to_owned()),
        "shared" => limits.push("It works in the project folder itself, the same one you and your other conversations use.".to_owned()),
        _ => {}
    }
    if facts.unattended {
        limits.push("Nobody is watching this run, so the computer's commands and screen are refused.".to_owned());
    }
    if facts.computer {
        limits.push("The linked computer's screen and input keep asking, even in Full access.".to_owned());
    }
    json!({
        "sessionId": facts.session_id,
        "agentId": facts.agent_id,
        "botName": facts.bot_name,
        "statement": statement,
        "folder": facts.folder,
        "isolation": facts.isolation,
        "runsAs": "your own Windows account",
        "network": "your network",
        "permissionMode": facts.mode.as_str(),
        "permissionSentence": permission_sentence,
        "unattended": facts.unattended,
        "tools": [
            { "id": "files", "label": "Files in its project folder", "on": facts.folder.is_some() },
            { "id": "commands", "label": "Commands on your computer, as you", "on": true },
            { "id": "internet", "label": "The internet, through the tools you have installed", "on": true },
            { "id": "team", "label": "Your other bots, through their own conversations", "on": true },
            { "id": "computer", "label": "Your linked computer, its browser and its screen", "on": facts.computer },
        ],
        "limits": limits,
    })
}

/// Whether the conversation works in its own copy of the repository, read through
/// the same rule `session.workspace` uses.
fn isolation_of(row: &Value) -> &'static str {
    crate::workspace::isolation_of(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(folder: Option<&str>, isolation: &'static str) -> Facts {
        Facts {
            bot_name: "Ada".into(), session_id: Some("ses-1".into()), agent_id: Some("agent-1".into()),
            folder: folder.map(str::to_owned), isolation, mode: ApprovalMode::Ask, unattended: false, computer: false,
        }
    }

    #[test]
    fn a_shared_folder_is_named_and_the_sentence_says_shared() {
        let answer = answer(&facts(Some("C:\\work\\project"), "shared"));
        assert_eq!(answer["folder"], json!("C:\\work\\project"));
        assert_eq!(answer["statement"], json!("Ada runs as you, as your own Windows account, in C:\\work\\project, with your network and the tools you have installed."));
        assert_eq!(answer["isolation"], "shared");
        assert_eq!(answer["permissionMode"], "ask");
        assert_eq!(answer["tools"][0]["on"], true);
        let limits = answer["limits"].as_array().unwrap();
        assert!(limits.iter().any(|line| line.as_str().unwrap().contains("the same one you")));
        assert!(!limits.iter().any(|line| line.as_str().unwrap().contains("Nobody is watching")));
    }

    #[test]
    fn no_folder_claims_nothing_about_files() {
        let answer = answer(&facts(None, "unknown"));
        assert!(answer["folder"].is_null());
        assert_eq!(answer["isolation"], "unknown");
        assert!(answer["statement"].as_str().unwrap().contains("on this computer"));
        assert_eq!(answer["tools"][0]["on"], false);
    }

    #[test]
    fn full_access_says_nothing_asks() {
        let mut facts = facts(Some("C:\\work\\project"), "shared");
        facts.mode = ApprovalMode::Bypass;
        let answer = answer(&facts);
        assert_eq!(answer["permissionMode"], "bypass");
        assert_eq!(answer["permissionSentence"], "Full access: nothing asks.");
    }

    #[test]
    fn an_unattended_run_says_the_computer_is_out_of_reach() {
        let mut facts = facts(Some("C:\\work\\project"), "shared");
        facts.unattended = true;
        facts.computer = true;
        let answer = answer(&facts);
        let limits = answer["limits"].as_array().unwrap();
        assert!(limits.iter().any(|line| line.as_str().unwrap().contains("Nobody is watching")));
        assert!(limits.iter().any(|line| line.as_str().unwrap().contains("even in Full access")));
        assert_eq!(answer["tools"][4]["on"], true);
    }

    #[test]
    fn a_copy_of_the_repository_reads_as_isolated() {
        let row = json!({"projectPath":"C:\\work\\project","link":{"worktree":{"path":"C:\\work\\project"}}});
        assert_eq!(isolation_of(&row), "isolated");
        let shared = json!({"projectPath":"C:\\work\\project","link":{}});
        assert_eq!(isolation_of(&shared), "shared");
        let elsewhere = json!({"projectPath":"C:\\work\\project","link":{"worktree":{"path":"C:\\other"}}});
        assert_eq!(isolation_of(&elsewhere), "shared");
        let none = json!({"projectPath":"","link":{}});
        assert_eq!(isolation_of(&none), "unknown");
    }
}
