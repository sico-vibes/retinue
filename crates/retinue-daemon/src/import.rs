//! `import.*`: conversations written by Claude Code, Codex or OpenCode on this
//! computer, brought into Retinue as history.
//!
//! The owner asked for this on 14 November 2026 (research:
//! `docs/research/owner-requests-2026-11-14.md`, section 2). The shape follows the
//! reference implementation in `openai/codex` (`codex-rs/external-agent-migration`):
//! a scan phase, a preview phase, and an import phase that takes one conversation
//! at a time and reports progress. The ledger is keyed by the source file's
//! SHA-256 plus the conversation id, so importing twice is harmless.
//!
//! Three stores are read: `~/.claude/projects/<project>/<session>.jsonl`,
//! `~/.codex/sessions/<YYYY>/<MM>/<DD>/rollout-*.jsonl`, and OpenCode's
//! `opencode.db` (opened from a temporary copy). What is never read:
//! `.credentials.json`, `auth.json`, OpenCode's `credential`, `account` and
//! `control_account` tables, account ids, instruction and environment blocks,
//! thinking blocks, image bytes and Claude's `file-history-snapshot` lines. An
//! import is read-only in Retinue unless the agent that wrote it is the bot's own
//! agent and its file is still there (`refuse_read_only`).

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use retinue_storage::Storage;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// The default window: a conversation nobody touched in a month is not offered.
pub(crate) const DEFAULT_DAYS: i64 = 30;
/// The most one `import.apply` call takes.
pub(crate) const MAX_CONVERSATIONS: usize = 50;
const MAX_PREVIEW: usize = 200;
/// A line longer than this is an image or an attachment; nothing imported comes
/// from one, so it is skipped without being parsed.
const MAX_LINE_BYTES: u64 = 4 * 1024 * 1024;
/// Stored text per message, and rows per conversation: a 169 MB rollout must not
/// become a 169 MB transcript.
const MAX_TEXT: usize = 20_000;
const MAX_TOOL_LABEL: usize = 120;
const MAX_TOOL_DETAIL: usize = 2_000;
const MAX_MESSAGES: usize = 2_000;
const MAX_TOOLS: usize = 2_000;
const MAX_TITLE: usize = 120;
/// How many conversations one bot is offered per `import.distribute` call.
const MAX_PER_BOT: usize = 5;

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

fn internal(error: impl std::fmt::Display) -> DispatchError {
    derr("internal", &error.to_string(), StatusCode::INTERNAL_SERVER_ERROR)
}

// ---------------------------------------------------------------------------
// The sources

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    Claude,
    Codex,
    OpenCode,
}

impl Source {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Source::Claude),
            "codex" => Some(Source::Codex),
            "opencode" => Some(Source::OpenCode),
            _ => None,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Source::Claude => "claude",
            Source::Codex => "codex",
            Source::OpenCode => "opencode",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Source::Claude => "Claude Code",
            Source::Codex => "Codex",
            Source::OpenCode => "OpenCode",
        }
    }

    fn all() -> [Source; 3] {
        [Source::Claude, Source::Codex, Source::OpenCode]
    }
}

/// Where each agent keeps its history on this machine.
#[derive(Clone, Default)]
pub(crate) struct Roots {
    pub(crate) claude_projects: Option<PathBuf>,
    pub(crate) codex_sessions: Option<PathBuf>,
    pub(crate) opencode_db: Option<PathBuf>,
}

#[cfg(test)]
pub(crate) static TEST_ROOTS: std::sync::Mutex<Option<Roots>> = std::sync::Mutex::new(None);

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn home_dir() -> Option<PathBuf> {
    env_path("USERPROFILE").or_else(|| env_path("HOME"))
}

fn roots() -> Roots {
    #[cfg(test)]
    {
        if let Some(roots) = TEST_ROOTS.lock().ok().and_then(|guard| guard.clone()) {
            return roots;
        }
    }
    let home = home_dir();
    let claude_base = env_path("CLAUDE_CONFIG_DIR").or_else(|| home.as_ref().map(|home| home.join(".claude")));
    let codex_base = env_path("CODEX_HOME").or_else(|| home.as_ref().map(|home| home.join(".codex")));
    let opencode_db = env_path("OPENCODE_DB").or_else(|| {
        let dir = env_path("XDG_DATA_HOME").map(|base| base.join("opencode")).or_else(|| home.as_ref().map(|home| home.join(".local").join("share").join("opencode")))?;
        let mut found = std::fs::read_dir(dir).ok()?.flatten().map(|entry| entry.path())
            .filter(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("opencode") && name.ends_with(".db")))
            .collect::<Vec<_>>();
        found.sort();
        found.pop()
    });
    Roots { claude_projects: claude_base.map(|base| base.join("projects")), codex_sessions: codex_base.map(|base| base.join("sessions")), opencode_db }
}

/// One conversation found on disk, before anything in it is read.
#[derive(Clone)]
struct Found {
    source: Source,
    id: String,
    path: PathBuf,
    bytes: u64,
    updated: SystemTime,
}

impl Found {
    fn updated_at(&self) -> String {
        iso(self.updated)
    }
}

fn iso(time: SystemTime) -> String {
    let datetime: chrono::DateTime<Utc> = time.into();
    datetime.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn iso_ms(millis: i64) -> Option<String> {
    chrono::DateTime::<Utc>::from_timestamp_millis(millis).map(|at| at.to_rfc3339_opts(SecondsFormat::Millis, true))
}

/// The files under `<home>/.claude/projects`. One folder per project, one
/// `.jsonl` per conversation; the file name is the session id `--resume` takes.
fn claude_files(root: &Path) -> Result<Vec<Found>, String> {
    let mut found = Vec::new();
    let projects = std::fs::read_dir(root).map_err(|error| format!("the Claude Code folder could not be read: {error}"))?;
    for project in projects.flatten() {
        let folder = project.path();
        if !folder.is_dir() { continue; }
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") { continue; }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned) else { continue };
            let Ok(metadata) = entry.metadata() else { continue };
            found.push(Found { source: Source::Claude, id, path, bytes: metadata.len(), updated: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH) });
        }
    }
    Ok(found)
}

/// `rollout-<timestamp>-<uuid>.jsonl` under `<year>/<month>/<day>`.
fn codex_files(root: &Path) -> Result<Vec<Found>, String> {
    fn walk(dir: &Path, depth: usize, found: &mut Vec<Found>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 4 { walk(&path, depth + 1, found); }
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") { continue; }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else { continue };
            let Ok(metadata) = entry.metadata() else { continue };
            found.push(Found { source: Source::Codex, id: stem.to_owned(), path, bytes: metadata.len(), updated: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH) });
        }
    }
    let mut found = Vec::new();
    std::fs::read_dir(root).map_err(|error| format!("the Codex folder could not be read: {error}"))?;
    walk(root, 0, &mut found);
    Ok(found)
}

fn opencode_files(db: &Path) -> Result<Vec<Found>, String> {
    let copy = TempCopy::of(db)?;
    let connection = open_copy(&copy.path)?;
    let bytes = std::fs::metadata(db).map(|metadata| metadata.len()).unwrap_or(0);
    let mut statement = connection
        .prepare("SELECT id,time_updated FROM session_v2 WHERE parent_id IS NULL ORDER BY time_updated DESC")
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|(id, updated)| Found {
            source: Source::OpenCode,
            id,
            path: db.to_path_buf(),
            bytes,
            updated: chrono::DateTime::<Utc>::from_timestamp_millis(updated).map(SystemTime::from).unwrap_or(SystemTime::UNIX_EPOCH),
        })
        .collect())
}

fn discover(source: Source, roots: &Roots) -> Result<Vec<Found>, String> {
    match source {
        Source::Claude => match &roots.claude_projects {
            Some(root) if root.is_dir() => claude_files(root),
            Some(root) => Err(format!("{} is not there any more.", root.display())),
            None => Ok(Vec::new()),
        },
        Source::Codex => match &roots.codex_sessions {
            Some(root) if root.is_dir() => codex_files(root),
            Some(root) => Err(format!("{} is not there any more.", root.display())),
            None => Ok(Vec::new()),
        },
        Source::OpenCode => match &roots.opencode_db {
            Some(db) if db.is_file() => opencode_files(db),
            _ => Ok(Vec::new()),
        },
    }
}

fn discover_all() -> Vec<(Source, Result<Vec<Found>, String>)> {
    let roots = roots();
    Source::all().into_iter().map(|source| (source, discover(source, &roots))).collect()
}

// ---------------------------------------------------------------------------
// A temporary copy of a database somebody else may be writing

struct TempCopy {
    dir: PathBuf,
    path: PathBuf,
}

impl TempCopy {
    /// Copies the database, and its write-ahead log first, into a fresh folder.
    /// The source is only ever read, never opened, so nothing here can write to
    /// the owner's OpenCode store. Copying the log before the database means a
    /// frame newer than the copy is simply ignored by SQLite.
    fn of(db: &Path) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("retinue-import-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).map_err(|error| format!("a temporary folder could not be made: {error}"))?;
        let guard = TempCopy { dir: dir.clone(), path: dir.join("opencode.db") };
        let mut log = db.as_os_str().to_owned();
        log.push("-wal");
        let log = PathBuf::from(log);
        if log.is_file() {
            std::fs::copy(&log, dir.join("opencode.db-wal")).map_err(|error| format!("the OpenCode log could not be read: {error}"))?;
        }
        std::fs::copy(db, &guard.path).map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
        Ok(guard)
    }
}

impl Drop for TempCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn open_copy(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|error| format!("the OpenCode database could not be opened: {error}"))?;
    connection.busy_timeout(Duration::from_secs(5)).map_err(|error| error.to_string())?;
    connection.pragma_update(None, "query_only", "ON").map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    Ok(connection)
}

// ---------------------------------------------------------------------------
// Reading one conversation into the shape Retinue stores

#[derive(Default)]
struct Turn {
    created_at: String,
    completed_at: Option<String>,
    messages: Vec<Value>,
    tools: Vec<Value>,
}

#[derive(Default)]
struct Transcript {
    title: Option<String>,
    project_path: String,
    provider_session_id: Option<String>,
    created_at: Option<String>,
    turns: Vec<Turn>,
    messages: u64,
    tools: u64,
    skipped: u64,
}

impl Transcript {
    fn ensure_turn(&mut self, at: &str) {
        if self.turns.is_empty() { self.turns.push(Turn { created_at: at.to_owned(), ..Turn::default() }); }
    }

    fn full(&self) -> bool {
        self.messages as usize >= MAX_MESSAGES
    }

    fn note(&mut self, role: &str, text: &str, at: &str) {
        if self.created_at.is_none() { self.created_at = Some(at.to_owned()); }
        if let Some(turn) = self.turns.last_mut() {
            turn.messages.push(json!({"role":role,"text":text,"createdAt":at}));
        }
    }

    fn user(&mut self, text: &str, at: &str) {
        let text = clip(text.trim(), MAX_TEXT);
        if text.is_empty() { return; }
        if self.full() { self.skipped += 1; return; }
        self.messages += 1;
        self.turns.push(Turn { created_at: at.to_owned(), ..Turn::default() });
        self.note("user", &text, at);
    }

    fn assistant(&mut self, text: &str, at: &str) {
        let text = clip(text.trim(), MAX_TEXT);
        if text.is_empty() { return; }
        if self.full() { self.skipped += 1; return; }
        self.messages += 1;
        self.ensure_turn(at);
        self.note("assistant", &text, at);
    }

    fn tool(&mut self, label: &str, detail: &str, at: &str) {
        if self.tools as usize >= MAX_TOOLS { return; }
        self.tools += 1;
        self.ensure_turn(at);
        if let Some(turn) = self.turns.last_mut() {
            turn.tools.push(json!({"title":clip(&one_line(label), MAX_TOOL_LABEL),"detail":clip(detail, MAX_TOOL_DETAIL),"createdAt":at,"completedAt":at}));
        }
    }

    fn finish(&mut self, updated: &str) {
        if self.created_at.is_none() { self.created_at = Some(updated.to_owned()); }
        for turn in self.turns.iter_mut() { turn.completed_at = Some(updated.to_owned()); }
        if self.skipped > 0 {
            self.ensure_turn(updated);
            let note = format!("{} more messages were not imported: this conversation is longer than {MAX_MESSAGES} messages.", self.skipped);
            self.note("notice", &note, updated);
        }
        if self.title.as_deref().map(str::trim).unwrap_or("").is_empty() {
            self.title = self.turns.iter().flat_map(|turn| turn.messages.iter())
                .find(|message| message["role"].as_str() == Some("user"))
                .and_then(|message| message["text"].as_str())
                .map(|text| clip(&one_line(text), MAX_TITLE));
        }
    }

    fn message_count(&self) -> u64 {
        self.messages + self.skipped
    }
}

/// Lines longer than `MAX_LINE_BYTES` are skipped, never parsed: they are image or
/// attachment payloads, which are not imported at all.
fn read_lines(path: &Path, mut visit: impl FnMut(&Value)) -> Result<(), String> {
    let file = File::open(path).map_err(|error| format!("the file could not be read: {error}"))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).map_err(|error| format!("the file could not be read: {error}"))?;
        if read == 0 { break; }
        if line.len() as u64 > MAX_LINE_BYTES { continue; }
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) { visit(&value); }
    }
    Ok(())
}

/// A message's text blocks plus the number of image blocks it carried. Images are
/// never copied: a note stands in for them.
fn claude_text(content: &Value) -> (String, usize) {
    match content {
        Value::String(text) => (text.clone(), 0),
        Value::Array(blocks) => {
            let mut texts = Vec::new();
            let mut images = 0;
            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => { if let Some(text) = block["text"].as_str().filter(|text| !text.trim().is_empty()) { texts.push(text); } }
                    Some("image") => images += 1,
                    _ => {}
                }
            }
            (texts.join("\n\n"), images)
        }
        _ => (String::new(), 0),
    }
}

fn with_image_note(text: &str, images: usize) -> String {
    if images == 0 { return text.to_owned(); }
    if text.trim().is_empty() { return "(image omitted)".to_owned(); }
    format!("{text}\n\n(image omitted)")
}

fn read_claude(found: &Found) -> Result<Transcript, String> {
    let mut transcript = Transcript::default();
    read_lines(&found.path, |value| {
        let at = value["timestamp"].as_str().unwrap_or("");
        match value["type"].as_str().unwrap_or("") {
            // The owner's own rename, when they made one.
            "custom-title" => {
                if let Some(title) = value["customTitle"].as_str().filter(|title| !title.trim().is_empty()) {
                    transcript.title = Some(clip(&one_line(title), MAX_TITLE));
                }
            }
            "user" => {
                if value["isSidechain"].as_bool() == Some(true) { return; }
                if transcript.project_path.is_empty() {
                    if let Some(cwd) = value["cwd"].as_str() { transcript.project_path = cwd.to_owned(); }
                }
                if transcript.provider_session_id.is_none() {
                    if let Some(id) = value["sessionId"].as_str() { transcript.provider_session_id = Some(id.to_owned()); }
                }
                let (text, images) = claude_text(&value["message"]["content"]);
                transcript.user(&with_image_note(&text, images), at);
            }
            "assistant" => {
                if value["isSidechain"].as_bool() == Some(true) { return; }
                if transcript.project_path.is_empty() {
                    if let Some(cwd) = value["cwd"].as_str() { transcript.project_path = cwd.to_owned(); }
                }
                let Some(blocks) = value["message"]["content"].as_array() else { return };
                for block in blocks {
                    match block["type"].as_str() {
                        Some("text") => { if let Some(text) = block["text"].as_str() { transcript.assistant(text, at); } }
                        Some("tool_use") => {
                            let name = block["name"].as_str().unwrap_or("tool");
                            transcript.tool(&tool_label(name, &block["input"]), &tool_detail(&block["input"]), at);
                        }
                        // `thinking` and `redacted_thinking` are private reasoning,
                        // `tool_result` is output: neither is transcript.
                        _ => {}
                    }
                }
            }
            // `attachment` (an environment snapshot), `system`,
            // `file-history-snapshot` (file contents), `queue-operation` and the
            // rest of the bookkeeping are never read.
            _ => {}
        }
    })?;
    transcript.finish(&found.updated_at());
    Ok(transcript)
}

/// Codex prepends project context to a user message; it is another project's
/// rules and environment, so those blocks are dropped.
fn is_context_block(text: &str) -> bool {
    let head = text.trim_start();
    if !head.starts_with('<') { return false; }
    ["<environment_context", "<user_instructions", "<permissions", "<permission_profile", "<turn_aborted", "<user_shell_command", "<system"]
        .iter()
        .any(|tag| head.starts_with(tag))
}

fn codex_arguments(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
        other => other.clone(),
    }
}

fn read_codex(found: &Found) -> Result<Transcript, String> {
    let mut transcript = Transcript::default();
    read_lines(&found.path, |value| {
        let at = value["timestamp"].as_str().unwrap_or("");
        let payload = &value["payload"];
        match value["type"].as_str().unwrap_or("") {
            "session_meta" => {
                if transcript.project_path.is_empty() {
                    if let Some(cwd) = payload["cwd"].as_str() { transcript.project_path = cwd.to_owned(); }
                }
                // `creator_user_id`, `creator_account_id` and `base_instructions`
                // are deliberately not read.
                if transcript.provider_session_id.is_none() {
                    transcript.provider_session_id = payload["session_id"].as_str().or_else(|| payload["id"].as_str()).map(str::to_owned);
                }
            }
            "response_item" => match payload["type"].as_str().unwrap_or("") {
                "message" => {
                    let mut texts: Vec<String> = Vec::new();
                    let mut images = 0;
                    for part in payload["content"].as_array().cloned().unwrap_or_default() {
                        match part["type"].as_str() {
                            Some("input_text") | Some("output_text") | Some("text") => {
                                if let Some(text) = part["text"].as_str().filter(|text| !text.trim().is_empty()) {
                                    if is_context_block(text) { continue; }
                                    texts.push(text.to_owned());
                                }
                            }
                            Some("input_image") | Some("image") => images += 1,
                            _ => {}
                        }
                    }
                    let text = with_image_note(&texts.join("\n\n"), images);
                    match payload["role"].as_str() {
                        Some("user") => transcript.user(&text, at),
                        Some("assistant") => transcript.assistant(&text, at),
                        _ => {}
                    }
                }
                "function_call" | "custom_tool_call" => {
                    let name = payload["name"].as_str().unwrap_or("tool");
                    let arguments = codex_arguments(payload.get("arguments").or_else(|| payload.get("input")).unwrap_or(&Value::Null));
                    transcript.tool(&tool_label(name, &arguments), &tool_detail(&arguments), at);
                }
                // `reasoning` is private, `function_call_output` is output.
                _ => {}
            },
            // `turn_context` carries sandbox and permission profiles, `world_state`
            // the project's AGENTS.md: neither is conversation.
            _ => {}
        }
    })?;
    transcript.finish(&found.updated_at());
    Ok(transcript)
}

fn read_opencode(found: &Found) -> Result<Transcript, String> {
    let copy = TempCopy::of(&found.path)?;
    let connection = open_copy(&copy.path)?;
    let mut transcript = Transcript::default();
    let row: Option<(String, String, i64)> = connection
        .query_row("SELECT COALESCE(title,''),COALESCE(directory,''),time_created FROM session_v2 WHERE id=?1", [&found.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .optional()
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    let Some((title, directory, created)) = row else { return Err("that conversation is no longer in OpenCode's database.".into()) };
    transcript.title = Some(clip(&one_line(&title), MAX_TITLE)).filter(|title| !title.is_empty());
    transcript.project_path = directory;
    transcript.provider_session_id = Some(found.id.clone());
    transcript.created_at = iso_ms(created);
    let mut statement = connection
        .prepare("SELECT type,data,time_created FROM session_message WHERE session_id=?1 AND type IN ('user','assistant') ORDER BY seq")
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    let rows = statement.query_map([&found.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    for (kind, data, at) in rows {
        let Ok(data) = serde_json::from_str::<Value>(&data) else { continue };
        let at = iso_ms(at).unwrap_or_default();
        match kind.as_str() {
            "user" => {
                let text = data["text"].as_str().unwrap_or("").to_owned();
                let files = data["files"].as_array().map(Vec::len).unwrap_or(0);
                transcript.user(&with_image_note(&text, files), &at);
            }
            "assistant" => {
                for part in data["content"].as_array().cloned().unwrap_or_default() {
                    match part["type"].as_str() {
                        Some("text") => { if let Some(text) = part["text"].as_str() { transcript.assistant(text, &at); } }
                        // `reasoning` is private thinking.
                        Some("tool") => {
                            if part["state"]["status"].as_str() != Some("completed") { continue; }
                            let name = part["name"].as_str().unwrap_or("tool");
                            let input = &part["state"]["input"];
                            transcript.tool(&tool_label(name, input), &tool_detail(input), &at);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    transcript.finish(&found.updated_at());
    Ok(transcript)
}

fn read_conversation(found: &Found) -> Result<Transcript, String> {
    match found.source {
        Source::Claude => read_claude(found),
        Source::Codex => read_codex(found),
        Source::OpenCode => read_opencode(found),
    }
}

/// "Bash: npm test", "Read: src/lib.rs": the tool's name and the one argument that
/// says what it was for.
fn tool_label(name: &str, input: &Value) -> String {
    let hint = ["command", "description", "file_path", "path", "pattern", "query", "url", "prompt", "title"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .map(|text| clip(&one_line(text), 80))
        .unwrap_or_default();
    if hint.is_empty() { name.to_owned() } else { format!("{name}: {hint}") }
}

/// Only named scalar arguments. A tool's `content`, `old_string` or `new_string`
/// carries file contents, which are never copied.
fn tool_detail(input: &Value) -> String {
    let parts = ["command", "description", "file_path", "path", "pattern", "query", "url"]
        .iter()
        .filter_map(|key| input.get(*key).and_then(Value::as_str).map(|text| format!("{key}: {}", clip(&one_line(text), 200))))
        .collect::<Vec<_>>();
    clip(&parts.join("\n"), MAX_TOOL_DETAIL)
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("the file could not be read: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| format!("the file could not be read: {error}"))?;
        if read == 0 { break; }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut hash = String::with_capacity(64);
    for byte in digest { hash.push_str(&format!("{byte:02x}")); }
    Ok(hash)
}

// ---------------------------------------------------------------------------
// import.scan

async fn scan(_st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let since = since_days(p)?;
    let cutoff = cutoff(since);
    let rows = tokio::task::spawn_blocking(discover_all).await.map_err(internal)?;
    let mut sources = Vec::new();
    for (source, found) in rows {
        match found {
            // A source that is not on this machine is not offered at all.
            Ok(found) if found.is_empty() && absent(source) => {}
            Ok(found) => {
                let recent = found.iter().filter(|row| row.updated >= cutoff).count();
                let newest = found.iter().map(|row| row.updated).max();
                let oldest = found.iter().map(|row| row.updated).min();
                sources.push(json!({
                    "source": source.id(),
                    "label": source.label(),
                    "folder": folder_for(source),
                    "conversations": found.len(),
                    "recent": recent,
                    "newest": newest.map(iso),
                    "oldest": oldest.map(iso),
                    "bytes": found.iter().map(|row| row.bytes).sum::<u64>(),
                    "error": Value::Null,
                }));
            }
            Err(error) => sources.push(json!({
                "source": source.id(),
                "label": source.label(),
                "folder": folder_for(source),
                "conversations": 0,
                "recent": 0,
                "newest": Value::Null,
                "oldest": Value::Null,
                "bytes": 0,
                "error": error,
            })),
        }
    }
    Ok(json!({"sinceDays": since, "scannedAt": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true), "sources": sources}))
}

fn absent(source: Source) -> bool {
    let roots = roots();
    match source {
        Source::Claude => roots.claude_projects.is_none_or(|root| !root.is_dir()),
        Source::Codex => roots.codex_sessions.is_none_or(|root| !root.is_dir()),
        Source::OpenCode => roots.opencode_db.is_none_or(|db| !db.is_file()),
    }
}

fn folder_for(source: Source) -> Option<String> {
    let roots = roots();
    match source {
        Source::Claude => roots.claude_projects,
        Source::Codex => roots.codex_sessions,
        Source::OpenCode => roots.opencode_db,
    }
    .map(|path| path.to_string_lossy().to_string())
}

fn since_days(p: &Value) -> Result<i64, DispatchError> {
    match p.get("sinceDays") {
        None | Some(Value::Null) => Ok(DEFAULT_DAYS),
        Some(value) => value.as_i64().filter(|days| (0..=3650).contains(days)).ok_or_else(|| invalid("sinceDays must be a whole number of days between 0 and 3650.")),
    }
}

fn cutoff(days: i64) -> SystemTime {
    if days <= 0 { return SystemTime::UNIX_EPOCH; }
    SystemTime::now() - Duration::from_secs(days as u64 * 86_400)
}

// ---------------------------------------------------------------------------
// import.preview

async fn preview(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let source = Source::parse(p["source"].as_str().unwrap_or("")).ok_or_else(|| invalid("source must be claude, codex or opencode."))?;
    let since = since_days(p)?;
    let offset = match p.get("offset") {
        None | Some(Value::Null) => 0usize,
        Some(value) => value.as_u64().ok_or_else(|| invalid("offset must be a whole number."))? as usize,
    };
    let limit = match p.get("limit") {
        None | Some(Value::Null) => 50usize,
        Some(value) => value.as_u64().filter(|limit| (1..=MAX_PREVIEW as u64).contains(limit)).ok_or_else(|| invalid(&format!("limit must be between 1 and {MAX_PREVIEW}.")))? as usize,
    };
    let db = st.db.clone();
    let rows = tokio::task::spawn_blocking(move || preview_rows(source, since, offset, limit, &db)).await.map_err(internal)?;
    rows.map_err(|error| derr("unavailable", &error, StatusCode::SERVICE_UNAVAILABLE))
}

fn preview_rows(source: Source, since: i64, offset: usize, limit: usize, db: &Storage) -> Result<Value, String> {
    let cutoff = cutoff(since);
    let mut found = discover(source, &roots())?;
    found.retain(|row| row.updated >= cutoff);
    found.sort_by(|a, b| b.updated.cmp(&a.updated));
    let total = found.len();
    let page = found.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
    let rows = match source {
        Source::OpenCode => opencode_preview(&page)?,
        _ => page.iter().map(|row| json_preview(row, db)).collect::<Vec<_>>(),
    };
    let next = if offset + rows.len() < total { json!(offset + rows.len()) } else { Value::Null };
    Ok(json!({"source": source.id(), "total": total, "offset": offset, "limit": limit, "next": next, "rows": rows}))
}

/// Title, folder, first user line, count and size need the file read once. The
/// conversation is not stored here: `import.apply` does that.
fn json_preview(found: &Found, db: &Storage) -> Value {
    let read = read_conversation(found);
    let (title, project, first, count, error) = match &read {
        Ok(transcript) => {
            let first = transcript.turns.iter().flat_map(|turn| turn.messages.iter())
                .find(|message| message["role"].as_str() == Some("user"))
                .and_then(|message| message["text"].as_str())
                .map(|text| clip(&one_line(text), 160))
                .unwrap_or_default();
            (transcript.title.clone().unwrap_or_default(), transcript.project_path.clone(), first, transcript.message_count(), Value::Null)
        }
        Err(reason) => (String::new(), String::new(), String::new(), 0, json!(clip(&one_line(reason), 200))),
    };
    let imported = db.import_ledger(found.source.id(), &found.id).ok().flatten();
    json!({
        "conversationId": found.id,
        "source": found.source.id(),
        "agent": found.source.id(),
        "agentLabel": found.source.label(),
        "title": title,
        "projectPath": project,
        "firstUser": first,
        "messageCount": count,
        "bytes": found.bytes,
        "updatedAt": found.updated_at(),
        "canResumeWith": if found.path.is_file() { json!(found.source.id()) } else { Value::Null },
        "alreadyImported": imported.is_some(),
        "importedSessionId": imported.map(|ledger| ledger["sessionId"].clone()).unwrap_or(Value::Null),
        "error": error,
    })
}

fn opencode_preview(page: &[Found]) -> Result<Vec<Value>, String> {
    if page.is_empty() { return Ok(Vec::new()); }
    let copy = TempCopy::of(&page[0].path)?;
    let connection = open_copy(&copy.path)?;
    let mut rows = Vec::new();
    for found in page {
        let row: Option<(String, String, i64, i64, String)> = connection
            .query_row(
                "SELECT COALESCE(title,''),COALESCE(directory,''),(SELECT COUNT(*) FROM session_message m WHERE m.session_id=s.id AND m.type IN ('user','assistant')),(SELECT COALESCE(SUM(LENGTH(m.data)),0) FROM session_message m WHERE m.session_id=s.id),(SELECT COALESCE(json_extract(m.data,'$.text'),'') FROM session_message m WHERE m.session_id=s.id AND m.type='user' ORDER BY m.seq LIMIT 1) FROM session_v2 s WHERE s.id=?1",
                [&found.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
        let Some((title, directory, count, bytes, first)) = row else { continue };
        rows.push(json!({
            "conversationId": found.id,
            "source": found.source.id(),
            "agent": found.source.id(),
            "agentLabel": found.source.label(),
            "title": clip(&one_line(&title), MAX_TITLE),
            "projectPath": directory,
            "firstUser": clip(&one_line(&first), 160),
            "messageCount": count,
            "bytes": bytes,
            "updatedAt": found.updated_at(),
            "canResumeWith": found.source.id(),
            "alreadyImported": Value::Null,
            "importedSessionId": Value::Null,
            "error": Value::Null,
        }));
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------
// import.apply

struct Assign {
    agent_id: Option<String>,
    project_id: Option<String>,
}

fn parse_assign(p: &Value, st: &AppState) -> Result<Assign, DispatchError> {
    let assign = p.get("assign").filter(|value| value.is_object()).unwrap_or(&Value::Null);
    let agent_id = match assign.get("agentId").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => {
            let agent = st.db.agent_get(id).map_err(|_| invalid("That bot no longer exists."))?;
            if agent["archived"] == true || agent["hidden"] == true { return Err(invalid("That bot cannot take a conversation.")); }
            Some(id.to_owned())
        }
        None => None,
    };
    let project_id = match assign.get("projectId").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => {
            st.db.project_get(id).map_err(|_| invalid("That project no longer exists."))?;
            Some(id.to_owned())
        }
        None => None,
    };
    Ok(Assign { agent_id, project_id })
}

async fn apply(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let items = p["items"].as_array().cloned().filter(|items| !items.is_empty()).ok_or_else(|| invalid("items must list at least one conversation."))?;
    let since = since_days(p)?;
    let assign = parse_assign(p, st)?;
    let mut wanted: Vec<(Source, String)> = Vec::new();
    for item in &items {
        let source = Source::parse(item["source"].as_str().unwrap_or("")).ok_or_else(|| invalid("every item needs a source: claude, codex or opencode."))?;
        let id = item["conversationId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("every item needs a conversationId."))?;
        if !wanted.iter().any(|(known, known_id)| *known == source && known_id == id) { wanted.push((source, id.to_owned())); }
    }
    if wanted.len() > MAX_CONVERSATIONS { return Err(invalid(&format!("at most {MAX_CONVERSATIONS} conversations can be imported at once."))); }
    let import_id = Uuid::new_v4().to_string();
    let cutoff = cutoff(since);
    let total = wanted.len();
    // The paths always come from the daemon's own scan, never from the request,
    // and only the sources this call names are read (reading OpenCode copies its
    // database).
    let mut distinct: Vec<Source> = Vec::new();
    for (source, _) in &wanted {
        if !distinct.contains(source) { distinct.push(*source); }
    }
    let rows = tokio::task::spawn_blocking(move || {
        let roots = roots();
        distinct.into_iter().map(|source| (source, discover(source, &roots))).collect::<Vec<_>>()
    })
    .await
    .map_err(internal)?;
    let mut found: HashMap<&'static str, HashMap<String, Found>> = HashMap::new();
    for (source, rows) in rows {
        let rows = rows.unwrap_or_default();
        found.insert(source.id(), rows.into_iter().map(|row| (row.id.clone(), row)).collect());
    }
    let runtimes = st.runtimes.read().await.clone();
    let assigned = assign.agent_id.as_deref()
        .and_then(|id| st.db.agent_get(id).ok())
        .filter(|agent| !agent["id"].is_null());
    let assigned_runtime = assigned.as_ref().and_then(|agent| agent["runtimeId"].as_str().map(str::to_owned));
    // A bot on another agent could not resume this conversation anyway, so the
    // provider's own session id is not kept for it.
    let assigned_provider = assigned_runtime.as_deref()
        .and_then(|runtime_id| runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(runtime_id)))
        .and_then(|runtime| runtime["provider"].as_str())
        .map(str::to_owned);
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut imported = Vec::new();
    let mut skipped = 0usize;
    let mut failed = 0usize;
    for (index, (source, conversation_id)) in wanted.iter().enumerate() {
        let progress = |state: &str, extra: Value| {
            let mut payload = json!({
                "importId": import_id.clone(),
                "index": index + 1,
                "total": total,
                "source": source.id(),
                "conversationId": conversation_id,
                "state": state,
            });
            if let (Some(map), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
                for (key, value) in extra { map.insert(key.clone(), value.clone()); }
            }
            payload
        };
        let Some(row) = found.get(source.id()).and_then(|rows| rows.get(conversation_id)).cloned() else {
            failed += 1;
            st.emit("import.progress", progress("failed", json!({"reason":"not_found","message":"That conversation is no longer on this computer."}))).await;
            continue;
        };
        st.emit("import.progress", progress("started", json!({}))).await;
        if row.updated < cutoff {
            skipped += 1;
            st.emit("import.progress", progress("skipped", json!({"reason":"older_than_window","message":format!("It was last changed more than {since} days ago.")}))).await;
            continue;
        }
        let work = row.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            let sha256 = sha256_file(&work.path)?;
            let transcript = read_conversation(&work)?;
            Ok::<_, String>((sha256, transcript))
        })
        .await;
        let (sha256, transcript) = match prepared {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(reason)) => {
                failed += 1;
                st.emit("import.progress", progress("failed", json!({"reason":"unreadable","message":clip(&one_line(&reason), 200)}))).await;
                continue;
            }
            Err(_) => {
                failed += 1;
                st.emit("import.progress", progress("failed", json!({"reason":"unreadable","message":"The conversation could not be read."}))).await;
                continue;
            }
        };
        if let Ok(Some(ledger)) = st.db.import_ledger(source.id(), conversation_id) {
            if ledger["sha256"].as_str() == Some(sha256.as_str()) {
                skipped += 1;
                st.emit("import.progress", progress("skipped", json!({"reason":"already_imported","importedSessionId":ledger["sessionId"].clone(),"title":transcript.title.clone().unwrap_or_default()}))).await;
                continue;
            }
        }
        let can_resume = row.path.is_file() && assigned_provider.as_deref().is_none_or(|provider| provider == source.id());
        let runtime_id = runtime_for(&runtimes, *source, assigned_runtime.as_deref());
        let input = json!({
            "sessionId": Uuid::new_v4().to_string(),
            "runtimeId": runtime_id,
            "provider": source.id(),
            "providerSessionId": if can_resume { transcript.provider_session_id.clone() } else { None },
            "canResumeWith": if can_resume { json!(source.id()) } else { Value::Null },
            "projectPath": transcript.project_path,
            "title": transcript.title.clone().filter(|title| !title.trim().is_empty()).unwrap_or_else(|| format!("{} conversation", source.label())),
            "agentId": assign.agent_id,
            "projectId": assign.project_id,
            "source": source.id(),
            "conversationId": conversation_id,
            "sourcePath": row.path.to_string_lossy().to_string(),
            "sha256": sha256,
            "importedAt": now,
            "createdAt": transcript.created_at,
            "messageCount": transcript.message_count(),
            "truncated": transcript.skipped > 0,
            "turns": transcript.turns.iter().map(|turn| json!({"createdAt":turn.created_at,"completedAt":turn.completed_at,"messages":turn.messages,"tools":turn.tools})).collect::<Vec<_>>(),
        });
        match st.db.import_session(&input) {
            Ok(stored) if stored["skipped"] == true => {
                skipped += 1;
                st.emit("import.progress", progress("skipped", json!({"reason":"already_imported","importedSessionId":stored["sessionId"].clone()}))).await;
            }
            Ok(stored) => {
                let session_id = stored["sessionId"].clone();
                imported.push(session_id.clone());
                st.emit("import.progress", progress("done", json!({"importedSessionId":session_id,"title":input["title"].clone()}))).await;
            }
            Err(error) => {
                failed += 1;
                st.emit("import.progress", progress("failed", json!({"reason":"storage","message":clip(&one_line(&error.to_string()), 200)}))).await;
            }
        }
    }
    let result = json!({"importId": import_id, "imported": imported.len(), "skipped": skipped, "failed": failed, "sessions": imported});
    st.emit("import.completed", json!({"importId": import_id, "imported": imported.len(), "skipped": skipped, "failed": failed, "conversationIds": imported})).await;
    Ok(result)
}

/// The runtime a conversation runs on: the receiving bot's own when it speaks the
/// same provider, else the first runtime of the provider that wrote it, else the
/// provider's name (a row that names no runtime, which only a read-only import has).
fn runtime_for(runtimes: &[Value], source: Source, agent_runtime: Option<&str>) -> String {
    let provider_of = |runtime_id: &str| runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(runtime_id)).and_then(|runtime| runtime["provider"].as_str()).map(str::to_owned);
    if let Some(runtime_id) = agent_runtime {
        if provider_of(runtime_id).as_deref() == Some(source.id()) { return runtime_id.to_owned(); }
    }
    runtimes.iter().find(|runtime| runtime["provider"].as_str() == Some(source.id())).and_then(|runtime| runtime["id"].as_str()).map(str::to_owned).unwrap_or_else(|| source.id().to_owned())
}

// ---------------------------------------------------------------------------
// import.distribute

async fn distribute(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let ids = match p.get("ids") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => value.as_array().ok_or_else(|| invalid("ids must be a list of conversations."))?
            .iter()
            .map(|id| id.as_str().map(str::trim).filter(|id| !id.is_empty()).map(str::to_owned).ok_or_else(|| invalid("ids must be a list of conversations.")))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let sessions = st.db.import_sessions(&ids).map_err(internal)?;
    let agents = st.db.agent_list(false, None).map_err(internal)?;
    let runtimes = st.runtimes.read().await.clone();
    let projects = st.db.project_list().map_err(internal)?;
    let bots = agents.iter()
        .filter(|agent| agent["hidden"] != true && agent["archived"] != true && agent["systemRole"].as_str() != Some("manager"))
        .cloned()
        .collect::<Vec<_>>();
    let provider_of = |agent: &Value| agent["runtimeId"].as_str().and_then(|id| runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(id))).and_then(|runtime| runtime["provider"].as_str()).map(str::to_owned);
    let mut per_bot: HashMap<String, usize> = HashMap::new();
    let mut suggestions = Vec::new();
    for session in &sessions {
        // With no ids given, only what has not been filed yet is offered.
        if ids.is_empty() && session["agentId"].is_string() { continue; }
        let id = session["id"].as_str().unwrap_or("").to_owned();
        let project_path = session["projectPath"].as_str().unwrap_or("");
        let project = session["projectId"].as_str().map(str::to_owned)
            .or_else(|| st.db.project_id_for_path(project_path).ok().flatten());
        let project_name = project.as_ref().and_then(|id| projects.iter().find(|row| row["id"].as_str() == Some(id))).and_then(|row| row["name"].as_str()).map(str::to_owned);
        let source = session["import"]["source"].as_str().unwrap_or("");
        let word = source_label(source);
        let mut chosen: Option<(Value, &str, String)> = None;
        // 1. The project folder: the strongest signal, and it is exact.
        if let (Some(project), Some(name)) = (&project, &project_name) {
            if let Ok(members) = st.db.project_members(project) {
                let candidate = members.iter().filter_map(|member| member["agentId"].as_str())
                    .filter_map(|id| bots.iter().find(|bot| bot["id"].as_str() == Some(id)))
                    .filter(|bot| under_cap(&per_bot, bot))
                    .min_by_key(|bot| per_bot.get(bot["id"].as_str().unwrap_or("")).copied().unwrap_or(0));
                if let Some(bot) = candidate {
                    chosen = Some((json!(bot["id"]), "project", format!("same folder as {name}")));
                }
            }
        }
        // 2. The agent it came from: the only case where "continue" can work.
        if chosen.is_none() {
            if let Some(bot) = bots.iter().filter(|bot| under_cap(&per_bot, bot)).find(|bot| provider_of(bot).as_deref() == Some(source)) {
                chosen = Some((json!(bot["id"]), "agent", format!("came from {word}, and {} is on {word}", bot_name(bot))));
            }
        }
        // 3. Role and title: a suggestion with its reason, never a silent decision.
        if chosen.is_none() {
            let haystack = format!("{} {}", session["title"].as_str().unwrap_or(""), session["firstUser"].as_str().unwrap_or(""));
            if let Some((bot, word)) = best_by_role(&bots, &haystack, &per_bot) {
                let role = bot["role"].as_str().map(one_line).filter(|role| !role.is_empty()).unwrap_or_else(|| bot_name(bot));
                chosen = Some((json!(bot["id"]), "role", format!("mentions {word}; {role}")));
            }
        }
        // 4. Balance: the least loaded bot, or the CEO when nobody is left.
        let (agent_id, reason, detail) = chosen.unwrap_or_else(|| {
            match bots.iter().filter(|bot| under_cap(&per_bot, bot)).min_by_key(|bot| per_bot.get(bot["id"].as_str().unwrap_or("")).copied().unwrap_or(0)) {
                Some(bot) => (json!(bot["id"]), "balance", format!("spread the load; {} has the fewest", bot_name(bot))),
                None => (Value::Null, "balance", "no bot is free; the CEO can file it".to_owned()),
            }
        });
        if let Some(agent_id) = agent_id.as_str() { *per_bot.entry(agent_id.to_owned()).or_insert(0) += 1; }
        suggestions.push(json!({
            "conversationId": id,
            "sessionId": id,
            "title": session["title"],
            "source": source,
            "agentId": agent_id,
            "reason": reason,
            "detail": detail,
        }));
    }
    Ok(json!({"suggestions": suggestions}))
}

fn under_cap(per_bot: &HashMap<String, usize>, bot: &Value) -> bool {
    per_bot.get(bot["id"].as_str().unwrap_or("")).copied().unwrap_or(0) < MAX_PER_BOT
}

fn bot_name(bot: &Value) -> String {
    bot["name"].as_str().map(str::to_owned).unwrap_or_else(|| "that bot".to_owned())
}

fn source_label(source: &str) -> &'static str {
    match source {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        _ => "another agent",
    }
}

fn role_words(text: &str) -> BTreeSet<String> {
    const STOP: [&str; 24] = ["that", "this", "with", "from", "have", "will", "your", "when", "what", "them", "then", "they", "been", "were", "there", "here", "into", "about", "would", "could", "should", "make", "made", "some"];
    text.split(|c: char| !c.is_alphanumeric())
        .map(|word| word.to_lowercase())
        .filter(|word| word.len() >= 4 && !STOP.contains(&word.as_str()))
        .collect()
}

fn best_by_role<'a>(bots: &'a [Value], text: &str, per_bot: &HashMap<String, usize>) -> Option<(&'a Value, String)> {
    let words = role_words(text);
    if words.is_empty() { return None; }
    let mut best: Option<(&Value, String, usize)> = None;
    for bot in bots.iter().filter(|bot| under_cap(per_bot, bot)) {
        let haystack = format!(
            "{} {} {}",
            bot["name"].as_str().unwrap_or(""),
            bot["role"].as_str().unwrap_or(""),
            bot["instructions"].as_str().unwrap_or("")
        )
        .to_lowercase();
        let Some(word) = words.iter().find(|word| haystack.contains(word.as_str())) else { continue };
        let score = words.iter().filter(|word| haystack.contains(word.as_str())).count();
        if best.as_ref().is_none_or(|(_, _, known)| score > *known) { best = Some((bot, word.clone(), score)); }
    }
    best.map(|(bot, word, _)| (bot, word))
}

// ---------------------------------------------------------------------------
// import.assign

async fn assign(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let session_id = p["sessionId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("sessionId is required."))?;
    let import = st.db.session_import_meta(session_id).map_err(internal)?
        .ok_or_else(|| invalid("That conversation was not imported."))?;
    let agent = match p.get("agentId") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(id)) if !id.trim().is_empty() => {
            let found = st.db.agent_get(id.trim()).map_err(|_| invalid("That bot no longer exists."))?;
            if found["archived"] == true || found["hidden"] == true { return Err(invalid("That bot cannot take a conversation.")); }
            Some(Some(found))
        }
        Some(_) => return Err(invalid("agentId must be a bot id or null.")),
    };
    let project = match p.get("projectId") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(id)) if !id.trim().is_empty() => {
            st.db.project_get(id.trim()).map_err(|_| invalid("That project no longer exists."))?;
            Some(Some(id.trim().to_owned()))
        }
        Some(_) => return Err(invalid("projectId must be a project id or null.")),
    };
    let runtimes = st.runtimes.read().await.clone();
    let mut runtime = None;
    let mut clear_provider = false;
    if let Some(Some(bot)) = &agent {
        let bot_provider = bot["runtimeId"].as_str().and_then(|id| runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(id))).and_then(|runtime| runtime["provider"].as_str());
        if bot_provider == Some(import["source"].as_str().unwrap_or("")) {
            runtime = bot["runtimeId"].as_str().map(str::to_owned);
        } else {
            // The receiving agent could not resume this conversation anyway.
            clear_provider = true;
        }
    }
    let summary = st.db.import_assign(
        session_id,
        agent.as_ref().map(|agent| agent.as_ref().and_then(|bot| bot["id"].as_str())),
        runtime.as_deref(),
        project.as_ref().map(|project| project.as_deref()),
        clear_provider,
    ).map_err(internal)?;
    st.emit("session.changed", summary.clone()).await;
    Ok(json!({"session": summary}))
}

// ---------------------------------------------------------------------------
// The read-only guard

/// A conversation that was imported is history, not a conversation Retinue owns:
/// a turn may only continue it when the bot's own agent wrote it and its file is
/// still where it was. Everything else is refused with `imported_read_only`, so
/// the window can offer "Give this to a bot" instead.
pub(crate) async fn refuse_read_only(st: &AppState, session: &Value) -> Result<(), DispatchError> {
    let Some(id) = session["id"].as_str() else { return Ok(()) };
    let Some(import) = st.db.session_import_meta(id).map_err(internal)? else { return Ok(()) };
    let source = import["source"].as_str().unwrap_or("");
    let source_path = import["sourcePath"].as_str().unwrap_or("");
    let present = session["providerSessionId"].as_str().is_some_and(|value| !value.is_empty()) && !source_path.is_empty() && Path::new(source_path).exists();
    let mut same_agent = false;
    if let Some(agent_id) = session["agentId"].as_str() {
        if let Ok(agent) = st.db.agent_get(agent_id) {
            if let Some(runtime_id) = agent["runtimeId"].as_str() {
                let runtimes = st.runtimes.read().await;
                same_agent = runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(runtime_id)).and_then(|runtime| runtime["provider"].as_str()) == Some(source);
            }
        }
    }
    if present && same_agent { return Ok(()) }
    Err(derr(
        "imported_read_only",
        &format!("This conversation was imported from {} and is read-only here. Give it to a bot to start a new conversation.", source_label(source)),
        StatusCode::FORBIDDEN,
    ))
}

// ---------------------------------------------------------------------------
// Routing

pub(crate) async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "import.scan" => scan(st, p).await,
        "import.preview" => preview(st, p).await,
        "import.apply" => apply(st, p).await,
        "import.distribute" => distribute(st, p).await,
        "import.assign" => assign(st, p).await,
        _ => Err(derr("unsupported", "unsupported import method", StatusCode::NOT_FOUND)),
    }
}
