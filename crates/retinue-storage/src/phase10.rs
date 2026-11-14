//! Phase 10 storage: conversations imported from other coding agents.
//!
//! An import writes the whole conversation — its session row, one turn per
//! exchange, the user and assistant text, and one normalised row per completed
//! tool step — in a single transaction together with the ledger entry that makes
//! the same source file idempotent. Nothing provider-private (credentials,
//! tokens, account ids, instruction text, thinking blocks, image bytes) reaches
//! these columns: `crates/retinue-daemon/src/import.rs` normalises the transcript
//! first and this module stores only what it is given.

use super::{
    params, session_event_summary_locked, Connection, OptionalExtension, SecondsFormat, Storage,
    StorageError, TransactionBehavior, Uuid, Utc,
};
use serde_json::{json, Value};

/// Additive and idempotent, like every other `ensure_*` schema.
pub(super) fn ensure_phase10_schema(c: &Connection) -> Result<(), StorageError> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS import_ledger(source TEXT NOT NULL,conversation_id TEXT NOT NULL,sha256 TEXT NOT NULL,session_id TEXT NOT NULL,imported_at TEXT NOT NULL,PRIMARY KEY(source,conversation_id));",
    )?;
    Ok(())
}

fn field<'a>(input: &'a Value, key: &str) -> Result<&'a str, StorageError> {
    input[key].as_str().filter(|value| !value.is_empty()).ok_or(StorageError::InvalidAgent)
}

impl Storage {
    /// The ledger's entry for one source conversation: the SHA-256 it was
    /// imported at, the conversation it became and when.
    pub fn import_ledger(&self, source: &str, conversation_id: &str) -> Result<Option<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let row: Option<(String, String, String)> = c
            .query_row(
                "SELECT sha256,session_id,imported_at FROM import_ledger WHERE source=?1 AND conversation_id=?2",
                params![source, conversation_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        Ok(row.map(|(sha256, session_id, imported_at)| json!({"sha256":sha256,"sessionId":session_id,"importedAt":imported_at})))
    }

    /// An imported conversation's own metadata, or `None` for a conversation the
    /// owner started in Retinue. `main.rs` refuses a turn on one that is read-only.
    /// (`session_import` is the machine-handoff writer in `phase4_sessions.rs`.)
    pub fn session_import_meta(&self, id: &str) -> Result<Option<Value>, StorageError> {
        // Every `session.prompt` asks this, so it reads on the small-read
        // connection and never queues behind a save or a snapshot.
        self.read_in(&self.read_conn, |c| {
            let raw: Option<Option<String>> = c
                .query_row("SELECT json_extract(data,'$.import') FROM sessions WHERE id=?1", [id], |r| r.get(0))
                .optional()?;
            Ok(raw.flatten().and_then(|text| serde_json::from_str::<Value>(&text).ok()).filter(Value::is_object))
        })
    }

    /// Every imported conversation, newest first, or only the ids asked for.
    pub fn import_sessions(&self, ids: &[String]) -> Result<Vec<Value>, StorageError> {
        let c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let mut statement = c.prepare(
            "SELECT id,agent_id,project_path,COALESCE(title_override,title),json_extract(data,'$.import'),(SELECT m.content FROM messages m WHERE m.session_id=sessions.id AND m.role='user' ORDER BY m.sequence LIMIT 1),json_extract(data,'$.projectId') FROM sessions WHERE json_extract(data,'$.import') IS NOT NULL ORDER BY updated_at DESC,rowid DESC",
        )?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .filter(|(id, ..)| ids.is_empty() || ids.iter().any(|wanted| wanted == id))
            .filter_map(|(id, agent_id, project_path, title, import, first_user, project_id)| {
                let import = serde_json::from_str::<Value>(&import?).ok().filter(Value::is_object)?;
                Some(json!({"id":id,"agentId":agent_id,"projectPath":project_path,"title":title,"import":import,"firstUser":first_user,"projectId":project_id}))
            })
            .collect())
    }

    /// Writes one imported conversation and its ledger entry, or reports that the
    /// same content was already imported. `input` is built by
    /// `import::transcript`; the shape is documented there.
    pub fn import_session(&self, input: &Value) -> Result<Value, StorageError> {
        let id = field(input, "sessionId")?;
        let runtime_id = field(input, "runtimeId")?;
        let provider = field(input, "provider")?;
        let project_path = input["projectPath"].as_str().unwrap_or("");
        let title = field(input, "title")?;
        let source = field(input, "source")?;
        let conversation_id = field(input, "conversationId")?;
        let sha256 = field(input, "sha256")?;
        let imported_at = field(input, "importedAt")?;
        let turns = input["turns"].as_array().cloned().unwrap_or_default();
        let mut c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // The check and the write share one transaction, so two applies racing on
        // the same file cannot import it twice.
        let existing: Option<(String, String)> = tx
            .query_row("SELECT sha256,session_id FROM import_ledger WHERE source=?1 AND conversation_id=?2", params![source, conversation_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        if let Some((known, session_id)) = existing {
            if known == sha256 {
                return Ok(json!({"skipped":true,"sessionId":session_id}));
            }
        }
        let provider_session_id = input["providerSessionId"].as_str().filter(|value| !value.is_empty());
        let agent_id = input["agentId"].as_str().filter(|value| !value.is_empty());
        let mut data = json!({"import": {
            "source": source,
            "importedAt": imported_at,
            "messageCount": input["messageCount"].as_u64().unwrap_or(0),
            "sha256": sha256,
            "conversationId": conversation_id,
            "sourcePath": input["sourcePath"].as_str().unwrap_or(""),
            "canResumeWith": input["canResumeWith"].clone(),
            "truncated": input["truncated"].as_bool() == Some(true),
        }});
        if let Some(project) = input["projectId"].as_str().filter(|value| !value.is_empty()) {
            data["projectId"] = json!(project);
        }
        tx.execute(
            "INSERT INTO sessions(id,runtime_id,provider,provider_session_id,project_path,title,state,resumable,created_at,updated_at,agent_id,data) VALUES(?1,?2,?3,?4,?5,?6,'completed',?7,?8,?9,?10,?11)",
            params![
                id,
                runtime_id,
                provider,
                provider_session_id,
                project_path,
                title.chars().take(120).collect::<String>(),
                i64::from(provider_session_id.is_some() && input["canResumeWith"].is_string()),
                input["createdAt"].as_str().filter(|value| !value.is_empty()).unwrap_or(imported_at),
                imported_at,
                agent_id,
                data.to_string()
            ],
        )?;
        let mut sequence: i64 = 0;
        for turn in turns.iter().filter(|turn| turn.is_object()) {
            let turn_id = Uuid::new_v4().to_string();
            let created = turn["createdAt"].as_str().unwrap_or(imported_at);
            let completed = turn["completedAt"].as_str().or(Some(created));
            tx.execute("INSERT INTO turns(id,session_id,state,created_at,completed_at) VALUES(?1,?2,'completed',?3,?4)", params![turn_id, id, created, completed])?;
            for message in turn["messages"].as_array().cloned().unwrap_or_default() {
                let Some(text) = message["text"].as_str().filter(|text| !text.trim().is_empty()) else { continue };
                let role = match message["role"].as_str() {
                    Some("user") => "user",
                    Some("assistant") => "assistant",
                    Some("notice") => "notice",
                    _ => continue,
                };
                sequence += 1;
                tx.execute(
                    "INSERT INTO messages(id,session_id,turn_id,sequence,role,content,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![Uuid::new_v4().to_string(), id, turn_id, sequence, role, text, message["createdAt"].as_str().unwrap_or(created)],
                )?;
            }
            for tool in turn["tools"].as_array().cloned().unwrap_or_default() {
                let Some(label) = tool["title"].as_str().filter(|label| !label.trim().is_empty()) else { continue };
                let detail = json!({
                    "kind": "tool",
                    "title": label,
                    "detail": {"summary": tool["detail"].as_str().unwrap_or("")},
                    "startedAt": tool["createdAt"].as_str().unwrap_or(created),
                    "completedAt": tool["completedAt"].as_str().or(Some(created)),
                });
                tx.execute(
                    "INSERT INTO tool_calls(id,session_id,turn_id,state,data) VALUES(?1,?2,?3,'completed',?4)",
                    params![Uuid::new_v4().to_string(), id, turn_id, detail.to_string()],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO import_ledger(source,conversation_id,sha256,session_id,imported_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(source,conversation_id) DO UPDATE SET sha256=excluded.sha256,session_id=excluded.session_id,imported_at=excluded.imported_at",
            params![source, conversation_id, sha256, id, imported_at],
        )?;
        tx.commit()?;
        Ok(json!({"skipped":false,"sessionId":id}))
    }

    /// Files an imported conversation under a bot (or nobody) and a project.
    /// `None` leaves a field as it is, `Some(None)` clears it and `Some(Some(x))`
    /// sets it. `runtime` is written only when the receiving bot speaks the same
    /// provider as the conversation came from, so a read-only import never claims
    /// a runtime it cannot run on. `clear_provider` drops the provider session id
    /// when the receiving bot's agent could not resume it anyway.
    #[allow(clippy::too_many_arguments)]
    pub fn import_assign(
        &self,
        session_id: &str,
        agent: Option<Option<&str>>,
        runtime: Option<&str>,
        project: Option<Option<&str>>,
        clear_provider: bool,
    ) -> Result<Value, StorageError> {
        let mut c = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND json_extract(data,'$.import') IS NOT NULL)", [session_id], |r| r.get(0))?;
        if !exists { return Err(StorageError::AgentNotFound); }
        if let Some(agent) = agent {
            if let Some(agent_id) = agent {
                let found: Option<bool> = tx.query_row("SELECT archived FROM agents WHERE id=?1", [agent_id], |r| r.get(0)).optional()?;
                if found != Some(false) { return Err(StorageError::AgentNotFound); }
            }
            tx.execute("UPDATE sessions SET agent_id=?2 WHERE id=?1", params![session_id, agent])?;
            if let Some(runtime) = runtime { tx.execute("UPDATE sessions SET runtime_id=?2 WHERE id=?1", params![session_id, runtime])?; }
        }
        if clear_provider {
            tx.execute("UPDATE sessions SET provider_session_id=NULL,resumable=0 WHERE id=?1", [session_id])?;
        }
        let patch = match project {
            None => None,
            Some(None) => Some(json!({"projectId": null})),
            Some(Some(project_id)) => Some(json!({"projectId": project_id})),
        };
        if let Some(patch) = patch {
            tx.execute("UPDATE sessions SET data=json_patch(COALESCE(NULLIF(data,''),'{}'),json(?2)) WHERE id=?1", params![session_id, patch.to_string()])?;
        }
        tx.execute("UPDATE sessions SET updated_at=?2 WHERE id=?1", params![session_id, Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)])?;
        let summary = session_event_summary_locked(&tx, session_id)?;
        tx.commit()?;
        Ok(summary)
    }
}
