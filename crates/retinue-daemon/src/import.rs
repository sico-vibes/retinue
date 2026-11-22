//! `import.*`: conversations written by Claude Code, Codex or OpenCode on this
//! computer, brought into Retinue as history and fitted to employees.
//!
//! The owner asked for this on 14 November 2026 (research:
//! `docs/research/owner-requests-2026-11-14.md`, section 2). The shape follows the
//! reference implementation in `openai/codex` (`codex-rs/external-agent-migration`):
//! a scan phase, a preview phase, and an import phase that takes one conversation
//! at a time and reports progress. The ledger is keyed by the source file's
//! SHA-256 plus the conversation id, so importing twice is harmless.
//!
//! On 21 November 2026 the owner approved `docs/research/conversation-import.md`
//! ("Yes, use them"): **an import is not a conversation until an employee owns
//! it.** Each source conversation ends as Continue (same provider, file still
//! there), Carry over (the new conversation starts from a bounded, editable
//! draft) or Keep as history (read-only). Suggestions are made before anything
//! is written, a hire is only ever suggested, and an import of a whole project
//! may create the project (never an employee).
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
/// A project roll-up reads at most this many conversations in one call.
pub(crate) const MAX_PROJECT_CONVERSATIONS: usize = 1_000;
/// A line longer than this is an image or an attachment; nothing imported comes
/// from one, so it is skipped without being parsed.
const MAX_LINE_BYTES: u64 = 4 * 1024 * 1024;
/// Stored text per message: the tail of a huge message is rarely why the owner
/// imported it, so one message is clipped to 8,000 characters.
const MAX_TEXT: usize = 8_000;
const MAX_TOOL_LABEL: usize = 120;
const MAX_TOOL_DETAIL: usize = 2_000;
const MAX_MESSAGES: usize = 2_000;
const MAX_TOOLS: usize = 2_000;
const MAX_TITLE: usize = 120;
/// How many conversations one bot is offered per `import.distribute` call.
const MAX_PER_BOT: usize = 5;
/// One conversation's stored text and tool labels, measured as it is read. Two
/// MiB is roughly 500 pages: enough for any real conversation.
pub(crate) const MAX_CONVERSATION_BYTES: u64 = 2 * 1024 * 1024;
/// One `import.apply` call writes at most this much stored text.
pub(crate) const MAX_BATCH_BYTES: u64 = 50 * 1024 * 1024;
/// The longest note `import.continue` accepts.
const MAX_CONTINUE_NOTE: usize = 2_000;

/// Test only: a smaller per-call budget, so the batch limit can be checked
/// without a 50 MB fixture. `0` means the real `MAX_BATCH_BYTES`.
#[cfg(test)]
pub(crate) static TEST_BATCH_LIMIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Test only: how many times a database was copied, so a batch can be shown to
/// copy it once rather than once per conversation.
#[cfg(test)]
pub(crate) static TEST_COPIES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn copy_count() -> usize {
    TEST_COPIES.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(test)]
pub(crate) fn reset_copy_count() {
    TEST_COPIES.store(0, std::sync::atomic::Ordering::SeqCst);
}

fn batch_limit() -> u64 {
    #[cfg(test)]
    {
        let limit = TEST_BATCH_LIMIT.load(std::sync::atomic::Ordering::SeqCst);
        if limit > 0 { return limit; }
    }
    MAX_BATCH_BYTES
}

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
    /// The source's own size (a file's length, or an OpenCode conversation's
    /// message bytes), never the whole database repeated per session.
    bytes: u64,
    updated: SystemTime,
    /// The folder the conversation worked in, read from its header (or, for
    /// OpenCode, from the database), so a project roll-up need not read it all.
    folder: String,
    /// The source's own archived flag, where it has one.
    archived: Option<bool>,
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
            found.push(Found { source: Source::Claude, id, path, bytes: metadata.len(), updated: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), folder: String::new(), archived: None });
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
            found.push(Found { source: Source::Codex, id: stem.to_owned(), path, bytes: metadata.len(), updated: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), folder: String::new(), archived: None });
        }
    }
    let mut found = Vec::new();
    std::fs::read_dir(root).map_err(|error| format!("the Codex folder could not be read: {error}"))?;
    walk(root, 0, &mut found);
    Ok(found)
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})")).map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    let mut rows = statement.query([]).map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    while let Some(row) = rows.next().map_err(|error| format!("the OpenCode database could not be read: {error}"))? {
        let name: String = row.get(1).map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
        if name.eq_ignore_ascii_case(column) { return Ok(true); }
    }
    Ok(false)
}

fn opencode_files(db: &Path) -> Result<Vec<Found>, String> {
    let copy = TempCopy::of(db)?;
    let connection = open_copy(&copy.path)?;
    let archived = if table_has_column(&connection, "session_v2", "archived")? { "COALESCE(s.archived,0)" } else { "0" };
    let sql = format!(
        "SELECT s.id,s.time_updated,COALESCE(s.directory,''),COALESCE((SELECT SUM(LENGTH(m.data)) FROM session_message m WHERE m.session_id=s.id),0),{archived} FROM session_v2 s WHERE s.parent_id IS NULL ORDER BY s.time_updated DESC"
    );
    let mut statement = connection.prepare(&sql).map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, i64>(4)?)))
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("the OpenCode database could not be read: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|(id, updated, folder, bytes, archived)| Found {
            source: Source::OpenCode,
            id,
            path: db.to_path_buf(),
            bytes: bytes.max(0) as u64,
            updated: chrono::DateTime::<Utc>::from_timestamp_millis(updated).map(SystemTime::from).unwrap_or(SystemTime::UNIX_EPOCH),
            folder,
            archived: Some(archived != 0),
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
        #[cfg(test)]
        TEST_COPIES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

/// Why a conversation stopped before its end: the message cap or the size
/// budget. `finish` turns it into one notice so nothing vanishes silently.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Truncation {
    Messages,
    Size,
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
    /// Stored text and tool labels so far, in bytes.
    bytes: u64,
    truncation: Option<Truncation>,
    /// A hash of every stored text, so an OpenCode conversation is idempotent
    /// even though its whole database changes all the time.
    fingerprint: Sha256,
}

impl Transcript {
    fn ensure_turn(&mut self, at: &str) {
        if self.turns.is_empty() { self.turns.push(Turn { created_at: at.to_owned(), ..Turn::default() }); }
    }

    fn hash_text(&mut self, text: &str) {
        self.fingerprint.update(text.as_bytes());
        self.fingerprint.update(b"\n");
    }

    /// Whether one more message of `size` bytes fits. Counts what was dropped,
    /// and records the first reason.
    fn accepts(&mut self, size: usize) -> bool {
        if self.messages as usize >= MAX_MESSAGES {
            self.skipped += 1;
            self.truncation.get_or_insert(Truncation::Messages);
            return false;
        }
        if self.bytes + size as u64 > MAX_CONVERSATION_BYTES {
            self.skipped += 1;
            self.truncation.get_or_insert(Truncation::Size);
            return false;
        }
        true
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
        if !self.accepts(text.len()) { return; }
        self.messages += 1;
        self.bytes += text.len() as u64;
        self.hash_text(&text);
        self.turns.push(Turn { created_at: at.to_owned(), ..Turn::default() });
        self.note("user", &text, at);
    }

    fn assistant(&mut self, text: &str, at: &str) {
        let text = clip(text.trim(), MAX_TEXT);
        if text.is_empty() { return; }
        if !self.accepts(text.len()) { return; }
        self.messages += 1;
        self.bytes += text.len() as u64;
        self.hash_text(&text);
        self.ensure_turn(at);
        self.note("assistant", &text, at);
    }

    fn tool(&mut self, label: &str, detail: &str, at: &str) {
        if self.tools as usize >= MAX_TOOLS { return; }
        let label = clip(&one_line(label), MAX_TOOL_LABEL);
        let detail = clip(detail, MAX_TOOL_DETAIL);
        let size = label.len() + detail.len();
        if self.bytes + size as u64 > MAX_CONVERSATION_BYTES {
            self.truncation.get_or_insert(Truncation::Size);
            return;
        }
        self.tools += 1;
        self.bytes += size as u64;
        self.hash_text(&label);
        self.hash_text(&detail);
        self.ensure_turn(at);
        if let Some(turn) = self.turns.last_mut() {
            turn.tools.push(json!({"title":label,"detail":detail,"createdAt":at,"completedAt":at}));
        }
    }

    fn finish(&mut self, updated: &str) {
        if self.created_at.is_none() { self.created_at = Some(updated.to_owned()); }
        for turn in self.turns.iter_mut() { turn.completed_at = Some(updated.to_owned()); }
        if let Some(truncation) = self.truncation {
            self.ensure_turn(updated);
            let note = match truncation {
                Truncation::Messages => format!("{} more messages were not imported: this conversation is longer than {MAX_MESSAGES} messages.", self.skipped),
                Truncation::Size => format!("Later messages were not imported: this conversation is larger than {} MB.", MAX_CONVERSATION_BYTES / (1024 * 1024)),
            };
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

    fn fingerprint(&self) -> Vec<u8> {
        self.fingerprint.clone().finalize().to_vec()
    }
}

/// Lines longer than `MAX_LINE_BYTES` are skipped, never parsed: they are image or
/// attachment payloads, which are not imported at all. `max_lines` bounds the
/// header reads that only look for a folder.
fn read_lines(path: &Path, max_lines: usize, mut visit: impl FnMut(&Value)) -> Result<(), String> {
    let file = File::open(path).map_err(|error| format!("the file could not be read: {error}"))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut line = String::new();
    let mut count = 0usize;
    loop {
        if count >= max_lines { break; }
        line.clear();
        let read = reader.read_line(&mut line).map_err(|error| format!("the file could not be read: {error}"))?;
        if read == 0 { break; }
        count += 1;
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
    read_lines(&found.path, usize::MAX, |value| {
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
    read_lines(&found.path, usize::MAX, |value| {
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

fn read_opencode(connection: &Connection, found: &Found) -> Result<Transcript, String> {
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

fn read_file(found: &Found) -> Result<Transcript, String> {
    match found.source {
        Source::Claude => read_claude(found),
        Source::Codex => read_codex(found),
        // The file reader is never used for OpenCode: a database is read through
        // `Reader`, which copies it once per batch.
        Source::OpenCode => Err("OpenCode conversations are read from the database.".into()),
    }
}

/// Reads conversations of one source, copying OpenCode's database once for the
/// whole batch (G7) instead of once per conversation.
struct Reader {
    source: Source,
    copy: Option<(TempCopy, Connection)>,
}

/// One conversation's hash and transcript, ready to write.
struct Prepared {
    sha256: String,
    transcript: Transcript,
    source_path: String,
}

impl Reader {
    fn new(source: Source) -> Self {
        Reader { source, copy: None }
    }

    fn connection(&mut self, db: &Path) -> Result<&Connection, String> {
        if self.copy.is_none() {
            let copy = TempCopy::of(db)?;
            let connection = open_copy(&copy.path)?;
            self.copy = Some((copy, connection));
        }
        Ok(&self.copy.as_ref().expect("the copy was just made").1)
    }

    fn read(&mut self, found: &Found) -> Result<Prepared, String> {
        match self.source {
            Source::OpenCode => {
                let connection = self.connection(&found.path)?;
                let transcript = read_opencode(connection, found)?;
                // The database changes whenever any other conversation does, so a
                // file hash would import every OpenCode conversation again. The
                // conversation's own text is the identity.
                let sha256 = sha256_hex(&transcript.fingerprint());
                Ok(Prepared { sha256, transcript, source_path: found.path.to_string_lossy().to_string() })
            }
            _ => {
                let sha256 = sha256_file(&found.path)?;
                let transcript = read_file(found)?;
                Ok(Prepared { sha256, transcript, source_path: found.path.to_string_lossy().to_string() })
            }
        }
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
    Ok(sha256_hex(&hasher.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hash = String::with_capacity(bytes.len() * 2);
    for byte in bytes { hash.push_str(&format!("{byte:02x}")); }
    hash
}

// ---------------------------------------------------------------------------
// Folders, so conversations are grouped by project

/// The comparison form of a folder: canonicalised when it exists (so symlinks
/// and `\\?\` prefixes agree), separators unified, case folded for Windows.
fn path_key(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() { return String::new(); }
    let text = std::fs::canonicalize(trimmed).map(|path| path.to_string_lossy().to_string()).unwrap_or_else(|_| trimmed.to_owned());
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

#[cfg(test)]
pub(crate) fn path_key_for_test(path: &str) -> String {
    path_key(path)
}

/// A git worktree's own folder maps back to the folder it was cut from, so the
/// copies one project made are imported under the same project.
fn worktree_root(folder: &Path) -> Option<PathBuf> {
    let git = folder.join(".git");
    if !git.is_file() { return None; }
    let text = std::fs::read_to_string(&git).ok()?;
    let target = text.lines().next()?.trim().strip_prefix("gitdir:")?.trim();
    let mut target = PathBuf::from(target);
    if target.is_relative() { target = folder.join(target); }
    let name = target.parent()?.file_name()?.to_string_lossy().to_string();
    if name != "worktrees" { return None; }
    let git_dir = target.parent()?.parent()?;
    if git_dir.file_name()?.to_string_lossy() != ".git" { return None; }
    let root = git_dir.parent()?;
    root.is_dir().then(|| root.to_path_buf())
}

fn group_of(row: &Found) -> (String, String, bool) {
    let raw = row.folder.trim();
    if raw.is_empty() { return (String::new(), String::new(), false); }
    let folder = worktree_root(Path::new(raw)).unwrap_or_else(|| PathBuf::from(raw));
    let exists = Path::new(raw).is_dir() || folder.is_dir();
    let display = folder.to_string_lossy().to_string();
    (path_key(&display), display, exists)
}

/// The folder a conversation ran in, read from its header. Claude and Codex put
/// it on an early line; OpenCode's discovery already has it.
fn folder_of(row: &Found) -> String {
    let mut folder = String::new();
    match row.source {
        Source::Claude => {
            let _ = read_lines(&row.path, 200, |value| {
                if folder.is_empty() {
                    if let Some(cwd) = value["cwd"].as_str().filter(|cwd| !cwd.trim().is_empty()) { folder = cwd.to_owned(); }
                }
            });
        }
        Source::Codex => {
            let _ = read_lines(&row.path, 20, |value| {
                if folder.is_empty() && value["type"].as_str() == Some("session_meta") {
                    if let Some(cwd) = value["payload"]["cwd"].as_str().filter(|cwd| !cwd.trim().is_empty()) { folder = cwd.to_owned(); }
                }
            });
        }
        Source::OpenCode => folder = row.folder.clone(),
    }
    folder
}

/// One project's conversations, before any preview row is built.
struct Group {
    key: String,
    path: String,
    exists: bool,
    found: Vec<Found>,
}

fn project_groups(since: i64, sources: &[Source]) -> (Vec<Group>, bool, usize, Vec<(Source, String)>) {
    let cutoff = cutoff(since);
    let roots = roots();
    let mut found: Vec<Found> = Vec::new();
    // What the window hides is counted, so a folder import can say it too.
    let mut older = 0usize;
    let mut errors: Vec<(Source, String)> = Vec::new();
    for source in sources {
        match discover(*source, &roots) {
            Ok(rows) => {
                for row in rows {
                    if row.updated >= cutoff { found.push(row); } else { older += 1; }
                }
            }
            Err(error) => errors.push((*source, error)),
        }
    }
    found.sort_by(|a, b| b.updated.cmp(&a.updated));
    let truncated = found.len() > MAX_PROJECT_CONVERSATIONS;
    found.truncate(MAX_PROJECT_CONVERSATIONS);
    for row in &mut found {
        if row.folder.trim().is_empty() { row.folder = folder_of(row); }
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for row in found {
        let (key, path, exists) = group_of(&row);
        match index.get(&key) {
            Some(&at) => groups[at].found.push(row),
            None => {
                index.insert(key.clone(), groups.len());
                groups.push(Group { key, path, exists, found: vec![row] });
            }
        }
    }
    (groups, truncated, older, errors)
}

// ---------------------------------------------------------------------------
// Runtimes and employees

fn runtime_for_provider<'a>(runtimes: &'a [Value], provider: &str) -> Option<&'a Value> {
    runtimes.iter().find(|runtime| runtime["provider"].as_str() == Some(provider) && runtime["status"].as_str() == Some("online"))
        .or_else(|| runtimes.iter().find(|runtime| runtime["provider"].as_str() == Some(provider)))
}

fn providers_with_runtimes(runtimes: &[Value]) -> Vec<String> {
    let mut providers: Vec<String> = Vec::new();
    for runtime in runtimes {
        if let Some(provider) = runtime["provider"].as_str() {
            if !providers.iter().any(|known| known == provider) { providers.push(provider.to_owned()); }
        }
    }
    providers
}

fn provider_of_runtime(runtimes: &[Value], runtime_id: &str) -> Option<String> {
    runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(runtime_id)).and_then(|runtime| runtime["provider"].as_str()).map(str::to_owned)
}

/// Every imported conversation, keyed by the source conversation it came from.
fn imported_index(sessions: &[Value]) -> HashMap<(String, String), String> {
    sessions.iter().filter_map(|session| {
        let source = session["import"]["source"].as_str()?;
        let conversation = session["import"]["conversationId"].as_str()?;
        let id = session["id"].as_str()?;
        Some(((source.to_owned(), conversation.to_owned()), id.to_owned()))
    }).collect()
}

fn source_filter(p: &Value) -> Result<Vec<Source>, DispatchError> {
    match p.get("sources") {
        None | Some(Value::Null) => Ok(Source::all().to_vec()),
        Some(Value::Array(items)) => {
            let mut wanted: Vec<Source> = Vec::new();
            for item in items {
                let source = Source::parse(item.as_str().unwrap_or("")).ok_or_else(|| invalid("sources must name claude, codex or opencode."))?;
                if !wanted.contains(&source) { wanted.push(source); }
            }
            if wanted.is_empty() { return Err(invalid("sources must name at least one of claude, codex or opencode.")); }
            Ok(wanted)
        }
        Some(_) => Err(invalid("sources must be a list of source names.")),
    }
}

/// A preview row, whether it came from `import.preview` or a project roll-up.
fn row_from(found: &Found, read: Result<Prepared, String>, imported: &HashMap<(String, String), String>, providers: &[String]) -> Value {
    let (title, project, first, count, error) = match &read {
        Ok(prepared) => {
            let transcript = &prepared.transcript;
            let first = transcript.turns.iter().flat_map(|turn| turn.messages.iter())
                .find(|message| message["role"].as_str() == Some("user"))
                .and_then(|message| message["text"].as_str())
                .map(|text| clip(&one_line(text), 160))
                .unwrap_or_default();
            (transcript.title.clone().unwrap_or_default(), transcript.project_path.clone(), first, transcript.message_count(), Value::Null)
        }
        Err(reason) => (String::new(), String::new(), String::new(), 0, json!(clip(&one_line(reason), 200))),
    };
    let imported_session = imported.get(&(found.source.id().to_owned(), found.id.clone()));
    let can_resume = found.path.is_file() && providers.iter().any(|provider| provider == found.source.id());
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
        "archived": found.archived,
        "canResumeWith": if can_resume { json!(found.source.id()) } else { Value::Null },
        "alreadyImported": imported_session.is_some(),
        "importedSessionId": imported_session,
        "error": error,
    })
}

// ---------------------------------------------------------------------------
// import.scan

async fn scan(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let since = since_days(p)?;
    let cutoff = cutoff(since);
    let runtimes = st.runtimes.read().await.clone();
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
                let runtime = runtime_for_provider(&runtimes, source.id());
                sources.push(json!({
                    "source": source.id(),
                    "label": source.label(),
                    "folder": folder_for(source),
                    "conversations": found.len(),
                    "recent": recent,
                    // Nothing disappears silently: what the window hides is said.
                    "older": found.len().saturating_sub(recent),
                    "newest": newest.map(iso),
                    "oldest": oldest.map(iso),
                    "bytes": found.iter().map(|row| row.bytes).sum::<u64>(),
                    "installed": runtime.is_some(),
                    "runtimeId": runtime.map(|runtime| runtime["id"].clone()).unwrap_or(Value::Null),
                    "providerStatus": runtime.map(|runtime| runtime["status"].clone()).unwrap_or(Value::Null),
                    "error": Value::Null,
                }));
            }
            Err(error) => sources.push(json!({
                "source": source.id(),
                "label": source.label(),
                "folder": folder_for(source),
                "conversations": 0,
                "recent": 0,
                "older": 0,
                "newest": Value::Null,
                "oldest": Value::Null,
                "bytes": 0,
                "installed": runtime_for_provider(&runtimes, source.id()).is_some(),
                "runtimeId": runtime_for_provider(&runtimes, source.id()).map(|runtime| runtime["id"].clone()).unwrap_or(Value::Null),
                "providerStatus": runtime_for_provider(&runtimes, source.id()).map(|runtime| runtime["status"].clone()).unwrap_or(Value::Null),
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
    let providers = providers_with_runtimes(&st.runtimes.read().await.clone());
    let rows = tokio::task::spawn_blocking(move || preview_rows(source, since, offset, limit, &db, &providers)).await.map_err(internal)?;
    rows.map_err(|error| derr("unavailable", &error, StatusCode::SERVICE_UNAVAILABLE))
}

fn preview_rows(source: Source, since: i64, offset: usize, limit: usize, db: &Storage, providers: &[String]) -> Result<Value, String> {
    let cutoff = cutoff(since);
    let mut found = discover(source, &roots())?;
    let discovered = found.len();
    found.retain(|row| row.updated >= cutoff);
    found.sort_by(|a, b| b.updated.cmp(&a.updated));
    let total = found.len();
    // What the window hides is counted here too, so every scan can say it.
    let older = discovered.saturating_sub(total);
    let page = found.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
    let imported = imported_index(&db.import_sessions(&[]).unwrap_or_default());
    let mut reader = Reader::new(source);
    let rows = page.iter().map(|row| row_from(row, reader.read(row), &imported, providers)).collect::<Vec<_>>();
    let next = if offset + rows.len() < total { json!(offset + rows.len()) } else { Value::Null };
    Ok(json!({"source": source.id(), "total": total, "older": older, "offset": offset, "limit": limit, "next": next, "rows": rows}))
}

// ---------------------------------------------------------------------------
// import.projects

async fn projects(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let since = since_days(p)?;
    let sources = source_filter(p)?;
    let db = st.db.clone();
    let providers = providers_with_runtimes(&st.runtimes.read().await.clone());
    let payload = tokio::task::spawn_blocking(move || {
        let (groups, truncated, older, errors) = project_groups(since, &sources);
        let imported = imported_index(&db.import_sessions(&[]).unwrap_or_default());
        let project_paths = db.project_list().unwrap_or_default().into_iter().filter_map(|row| {
            let id = row["id"].as_str()?.to_owned();
            let path = row["path"].as_str().filter(|path| !path.trim().is_empty())?;
            Some((path_key(path), id))
        }).collect::<HashMap<String, String>>();
        let mut readers: HashMap<&'static str, Reader> = HashMap::new();
        let mut budget = MAX_PROJECT_CONVERSATIONS;
        let mut projects = Vec::new();
        for group in groups {
            let mut conversations = Vec::new();
            for found in &group.found {
                if budget == 0 { break; }
                let reader = readers.entry(found.source.id()).or_insert_with(|| Reader::new(found.source));
                conversations.push(row_from(found, reader.read(found), &imported, &providers));
                budget -= 1;
            }
            let mut group_sources: Vec<&str> = Vec::new();
            for found in &group.found {
                if !group_sources.contains(&found.source.id()) { group_sources.push(found.source.id()); }
            }
            let name = Path::new(&group.path).file_name().and_then(|name| name.to_str()).map(str::to_owned)
                .unwrap_or_else(|| if group.path.is_empty() { "Folder not recorded".to_owned() } else { group.path.clone() });
            let newest = group.found.iter().map(|row| row.updated).max();
            projects.push(json!({
                "key": group.key,
                "path": group.path,
                "name": name,
                "exists": group.exists,
                "existingProjectId": project_paths.get(&group.key),
                "total": group.found.len(),
                "shown": conversations.len(),
                "sources": group_sources,
                "bytes": group.found.iter().map(|row| row.bytes).sum::<u64>(),
                "newest": newest.map(iso),
                "conversations": conversations,
            }));
        }
        let mut source_errors = Vec::new();
        for (source, message) in errors { source_errors.push(json!({"source": source.id(), "label": source.label(), "message": message})); }
        Ok::<_, String>(json!({"projects": projects, "older": older, "truncated": truncated, "errors": source_errors}))
    }).await.map_err(internal)?;
    let mut reply = payload.map_err(|error| derr("unavailable", &error, StatusCode::SERVICE_UNAVAILABLE))?;
    reply["sinceDays"] = json!(since);
    reply["scannedAt"] = json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
    Ok(reply)
}

// ---------------------------------------------------------------------------
// Finding or making the project for a folder

async fn find_or_create_project(st: &AppState, folder: &str) -> Result<(String, bool), DispatchError> {
    let folder = folder.trim();
    if folder.is_empty() { return Err(invalid("That conversation records no folder, so it cannot make a project.")); }
    let key = path_key(folder);
    let projects = st.db.project_list().map_err(internal)?;
    if let Some(project) = projects.iter().find(|project| project["path"].as_str().is_some_and(|path| path_key(path) == key)) {
        if let Some(id) = project["id"].as_str() { return Ok((id.to_owned(), false)); }
    }
    if !Path::new(folder).is_dir() { return Err(invalid("That folder is not on this computer any more, so the project cannot be made.")); }
    let name = Path::new(folder).file_name().and_then(|name| name.to_str()).unwrap_or("Imported project");
    let name: String = name.chars().take(80).collect();
    let identity = match crate::repo_identity::of_path(Some(folder)).await {
        crate::repo_identity::Origin::Found(remote) => Some(remote.identity),
        _ => None,
    };
    let (project, events) = st.db.project_create_with_identity(&name, Some(folder), identity.as_deref()).map_err(internal)?;
    st.broadcast_persisted(events);
    let id = project["id"].as_str().ok_or_else(|| internal("the project was not written"))?.to_owned();
    Ok((id, true))
}

// ---------------------------------------------------------------------------
// The suggestion rule, shared by import.distribute and project mode

struct Suggester<'a> {
    db: &'a Storage,
    bots: Vec<Value>,
    providers: HashMap<String, String>,
    projects: Vec<Value>,
    per_bot: HashMap<String, usize>,
}

impl<'a> Suggester<'a> {
    fn new(st: &'a AppState, runtimes: &[Value], agents: &[Value], projects: &[Value]) -> Self {
        let bots = agents.iter()
            .filter(|agent| agent["hidden"] != true && agent["archived"] != true && agent["systemRole"].as_str() != Some("manager"))
            .cloned()
            .collect::<Vec<_>>();
        let mut providers = HashMap::new();
        for bot in &bots {
            if let Some(id) = bot["id"].as_str() {
                if let Some(provider) = bot["runtimeId"].as_str().and_then(|runtime| provider_of_runtime(runtimes, runtime)) {
                    providers.insert(id.to_owned(), provider);
                }
            }
        }
        Suggester { db: &st.db, bots, providers, projects: projects.to_vec(), per_bot: HashMap::new() }
    }

    fn project_name(&self, project_id: &str) -> Option<String> {
        self.projects.iter().find(|project| project["id"].as_str() == Some(project_id)).and_then(|project| project["name"].as_str()).map(str::to_owned)
    }

    /// Project, then the agent it came from, then role, then balance. The reason
    /// is for a card, never a silent decision.
    fn suggest(&mut self, source: &str, project: Option<&str>, text: &str) -> (Value, &'static str, String) {
        let mut chosen: Option<(Value, &'static str, String)> = None;
        if let Some(project_id) = project {
            if let Ok(members) = self.db.project_members(project_id) {
                let name = self.project_name(project_id).unwrap_or_else(|| "this project".to_owned());
                let candidate = members.iter().filter_map(|member| member["agentId"].as_str())
                    .filter_map(|id| self.bots.iter().find(|bot| bot["id"].as_str() == Some(id)))
                    .filter(|bot| self.under_cap(bot))
                    .min_by_key(|bot| self.per_bot.get(bot["id"].as_str().unwrap_or("")).copied().unwrap_or(0));
                if let Some(bot) = candidate {
                    chosen = Some((json!(bot["id"]), "project", format!("same folder as {name}")));
                }
            }
        }
        if chosen.is_none() {
            if let Some(bot) = self.bots.iter().filter(|bot| self.under_cap(bot)).find(|bot| self.provider_of(bot).as_deref() == Some(source)) {
                let word = source_label(source);
                chosen = Some((json!(bot["id"]), "agent", format!("came from {word}, and {} is on {word}", bot_name(bot))));
            }
        }
        if chosen.is_none() {
            if let Some((bot, word)) = self.best_by_role(text) {
                let role = bot["role"].as_str().map(one_line).filter(|role| !role.is_empty()).unwrap_or_else(|| bot_name(bot));
                chosen = Some((json!(bot["id"]), "role", format!("mentions {word}; {role}")));
            }
        }
        let suggestion = chosen.unwrap_or_else(|| match self.bots.iter().filter(|bot| self.under_cap(bot)).min_by_key(|bot| self.per_bot.get(bot["id"].as_str().unwrap_or("")).copied().unwrap_or(0)) {
            Some(bot) => (json!(bot["id"]), "balance", format!("spread the load; {} has the fewest", bot_name(bot))),
            None => (Value::Null, "balance", "no bot is free; the CEO can file it".to_owned()),
        });
        if let Some(agent_id) = suggestion.0.as_str() { *self.per_bot.entry(agent_id.to_owned()).or_insert(0) += 1; }
        suggestion
    }

    fn provider_of(&self, bot: &Value) -> Option<String> {
        bot["id"].as_str().and_then(|id| self.providers.get(id)).cloned()
    }

    fn under_cap(&self, bot: &Value) -> bool {
        self.per_bot.get(bot["id"].as_str().unwrap_or("")).copied().unwrap_or(0) < MAX_PER_BOT
    }

    fn best_by_role(&self, text: &str) -> Option<(&Value, String)> {
        let words = role_words(text);
        if words.is_empty() { return None; }
        let mut best: Option<(&Value, String, usize)> = None;
        for bot in self.bots.iter().filter(|bot| self.under_cap(bot)) {
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

    fn speaks(&self, source: &str) -> bool {
        self.providers.values().any(|provider| provider == source)
    }
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

/// A hire, only ever suggested: the owner confirms it with `project.hire`, which
/// is what makes an employee. `runtimeId` is the source provider's own runtime,
/// so the suggestion is possible at all; `presetId` is a persona nobody wears yet.
fn hire_suggestion(st: &AppState, runtimes: &[Value], source: &str, project: Option<&Value>) -> Option<Value> {
    let runtime = runtime_for_provider(runtimes, source)?;
    let model = project.and_then(|project| project["overrides"]["models"].get(source)).cloned().unwrap_or(Value::Null);
    let hired = st.db.preset_agents().unwrap_or_default();
    let taken = st.db.agent_list(false, None).unwrap_or_default().iter()
        .filter_map(|agent| agent["name"].as_str().map(|name| name.trim().to_lowercase())).collect::<Vec<_>>();
    let free = crate::presets::presets().iter().find(|preset| !hired.iter().any(|row| row["presetId"].as_str() == Some(preset.id.as_str())));
    let (name, role, preset_id) = match free {
        Some(preset) => (unique_name(&preset.nickname, &taken), preset.role.clone(), json!(preset.id)),
        None => (unique_name(&format!("{} employee", source_label(source)), &taken), format!("Works with {} history", source_label(source)), Value::Null),
    };
    Some(json!({
        "runtimeId": runtime["id"],
        "model": model,
        "name": name,
        "role": role,
        "presetId": preset_id,
    }))
}

fn unique_name(base: &str, taken: &[String]) -> String {
    let mut attempt = 1;
    loop {
        let name = if attempt == 1 { base.to_owned() } else { format!("{base} {attempt}") };
        if !taken.contains(&name.to_lowercase()) { return name; }
        attempt += 1;
    }
}

// ---------------------------------------------------------------------------
// import.distribute

/// One conversation the rule looks at, whether it is already imported or is a
/// preview row that has not been written yet.
struct Subject {
    source: String,
    /// The source conversation id for a preview row; the Retinue id otherwise.
    conversation_id: String,
    session_id: Option<String>,
    title: String,
    first_user: String,
    project_path: String,
    recorded_project_id: Option<String>,
}

async fn distribute(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let ids = match p.get("ids") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => value.as_array().ok_or_else(|| invalid("ids must be a list of conversations."))?
            .iter()
            .map(|id| id.as_str().map(str::trim).filter(|id| !id.is_empty()).map(str::to_owned).ok_or_else(|| invalid("ids must be a list of conversations.")))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let mut preview_items: Vec<(Source, String)> = Vec::new();
    match p.get("items") {
        None | Some(Value::Null) => {}
        Some(Value::Array(items)) => {
            for item in items {
                let source = Source::parse(item["source"].as_str().unwrap_or("")).ok_or_else(|| invalid("every item needs a source: claude, codex or opencode."))?;
                let id = item["conversationId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("every item needs a conversationId."))?.to_owned();
                if !preview_items.iter().any(|(known, known_id)| *known == source && known_id == &id) { preview_items.push((source, id)); }
            }
        }
        Some(_) => return Err(invalid("items must be a list of conversations.")),
    }
    if preview_items.len() > MAX_CONVERSATIONS { return Err(invalid(&format!("at most {MAX_CONVERSATIONS} preview rows can be suggested at once."))); }

    let runtimes = st.runtimes.read().await.clone();
    let projects = st.db.project_list().map_err(internal)?;
    let agents = st.db.agent_list(false, None).map_err(internal)?;

    let mut subjects: Vec<Subject> = Vec::new();
    // With neither named, every imported conversation not yet filed is offered.
    if !ids.is_empty() || preview_items.is_empty() {
        let sessions = st.db.import_sessions(&ids).map_err(internal)?;
        for session in &sessions {
            if ids.is_empty() && session["agentId"].is_string() { continue; }
            let Some(id) = session["id"].as_str() else { continue };
            subjects.push(Subject {
                source: session["import"]["source"].as_str().unwrap_or("").to_owned(),
                conversation_id: id.to_owned(),
                session_id: Some(id.to_owned()),
                title: session["title"].as_str().unwrap_or("").to_owned(),
                first_user: session["firstUser"].as_str().unwrap_or("").to_owned(),
                project_path: session["projectPath"].as_str().unwrap_or("").to_owned(),
                recorded_project_id: session["projectId"].as_str().map(str::to_owned),
            });
        }
    }
    if !preview_items.is_empty() {
        let wanted = preview_items.clone();
        let rows = tokio::task::spawn_blocking(move || {
            let roots = roots();
            // One discovery per source, so an OpenCode database is copied once for
            // the whole request rather than once per preview row.
            let mut found_maps: HashMap<&'static str, HashMap<String, Found>> = HashMap::new();
            let mut readers: HashMap<&'static str, Reader> = HashMap::new();
            let mut rows = Vec::new();
            for (source, id) in wanted {
                if !found_maps.contains_key(source.id()) {
                    let map = discover(source, &roots).unwrap_or_default().into_iter().map(|row| (row.id.clone(), row)).collect();
                    found_maps.insert(source.id(), map);
                }
                let Some(found) = found_maps.get(source.id()).and_then(|map| map.get(&id)).cloned() else { continue };
                let reader = readers.entry(found.source.id()).or_insert_with(|| Reader::new(found.source));
                let read = reader.read(&found);
                rows.push((found, read));
            }
            rows
        }).await.map_err(internal)?;
        for (found, read) in rows {
            let (title, first, project) = match &read {
                Ok(prepared) => {
                    let transcript = &prepared.transcript;
                    let first = transcript.turns.iter().flat_map(|turn| turn.messages.iter())
                        .find(|message| message["role"].as_str() == Some("user"))
                        .and_then(|message| message["text"].as_str())
                        .map(|text| clip(&one_line(text), 160))
                        .unwrap_or_default();
                    (transcript.title.clone().unwrap_or_default(), first, transcript.project_path.clone())
                }
                Err(_) => (String::new(), String::new(), found.folder.clone()),
            };
            subjects.push(Subject {
                source: found.source.id().to_owned(),
                conversation_id: found.id,
                session_id: None,
                title,
                first_user: first,
                project_path: project,
                recorded_project_id: None,
            });
        }
    }

    let mut suggester = Suggester::new(st, &runtimes, &agents, &projects);
    let mut hires: HashMap<String, Option<Value>> = HashMap::new();
    let mut suggestions = Vec::new();
    for subject in &subjects {
        let project = subject.recorded_project_id.clone()
            .or_else(|| projects.iter().find(|project| project["path"].as_str().is_some_and(|path| path_key(path) == path_key(&subject.project_path))).and_then(|project| project["id"].as_str().map(str::to_owned)));
        let haystack = format!("{} {}", subject.title, subject.first_user);
        let (agent_id, reason, detail) = suggester.suggest(&subject.source, project.as_deref(), &haystack);
        // A hire is offered only when nobody but the CEO speaks the source, and
        // only when that provider is installed; nothing is hired here.
        let hire = if suggester.speaks(&subject.source) { Value::Null } else {
            let entry = hires.entry(subject.source.clone()).or_insert_with(|| {
                let project_row = project.as_ref().and_then(|id| projects.iter().find(|row| row["id"].as_str() == Some(id)));
                hire_suggestion(st, &runtimes, &subject.source, project_row)
            });
            entry.clone().unwrap_or(Value::Null)
        };
        suggestions.push(json!({
            "conversationId": subject.conversation_id,
            "sessionId": subject.session_id,
            "title": subject.title,
            "source": subject.source,
            "agentId": agent_id,
            "reason": reason,
            "detail": detail,
            "hire": hire,
        }));
    }
    Ok(json!({"suggestions": suggestions}))
}

// ---------------------------------------------------------------------------
// import.apply

#[derive(Clone, Copy, PartialEq, Eq)]
enum AssignMode {
    /// Nothing chosen yet: the window asks the owner before an employee owns it.
    Ask,
    /// Every conversation is filed under its own folder's project and its suggestion.
    Project,
    /// Every conversation goes to the one employee in the request.
    Agent,
    /// Every conversation goes to the CEO.
    Ceo,
}

struct Assignment {
    mode: AssignMode,
    agent_id: Option<String>,
    project_id: Option<String>,
    project_path: Option<String>,
}

async fn parse_assignment(st: &AppState, p: &Value) -> Result<Assignment, DispatchError> {
    let body = p.get("assign").filter(|value| value.is_object()).cloned().unwrap_or(Value::Null);
    let mode_name = body["mode"].as_str().map(str::trim).filter(|mode| !mode.is_empty());
    let agent_id = match body.get("agentId").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => {
            let agent = st.db.agent_get(id).map_err(|_| invalid("That bot no longer exists."))?;
            if agent["archived"] == true || agent["hidden"] == true { return Err(invalid("That bot cannot take a conversation.")); }
            Some(id.to_owned())
        }
        None => None,
    };
    let project_id = match body.get("projectId").and_then(Value::as_str).map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => {
            st.db.project_get(id).map_err(|_| invalid("That project no longer exists."))?;
            Some(id.to_owned())
        }
        None => None,
    };
    let project_path = match body.get("projectPath").and_then(Value::as_str).map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => {
            if project_id.is_some() { return Err(invalid("Give projectId or projectPath, not both.")); }
            Some(path.to_owned())
        }
        None => None,
    };
    let mode = match mode_name {
        Some("ask") => AssignMode::Ask,
        Some("project") => AssignMode::Project,
        Some("agent") => AssignMode::Agent,
        Some("ceo") => AssignMode::Ceo,
        Some(_) => return Err(invalid("assign.mode must be ask, project, agent or ceo.")),
        None if agent_id.is_some() => AssignMode::Agent,
        // A project alone files the conversation in it; an employee is chosen
        // only when the owner asks for one (`agent`, `ceo`) or for the rule (`project`).
        None => AssignMode::Ask,
    };
    let agent_id = match mode {
        AssignMode::Agent if agent_id.is_none() => return Err(invalid("assign.mode agent needs an agentId.")),
        AssignMode::Ceo => Some(st.db.manager_agent().map_err(internal)?.and_then(|manager| manager["id"].as_str().map(str::to_owned))
            .ok_or_else(|| invalid("The CEO has not been set up yet, so no conversation can be filed there."))?),
        _ => agent_id,
    };
    Ok(Assignment { mode, agent_id, project_id, project_path })
}

/// What the batch read found for one conversation.
enum Outcome {
    NotFound,
    Older,
    Failed(String),
    /// Boxed: one prepared conversation is much larger than any other variant.
    Ready(Box<Prepared>),
}

/// Reads the named conversations for one apply, copying OpenCode's database once
/// per batch. Outcomes stay in the order of the request, which is the order the
/// progress events use.
fn prepare_wanted(wanted: Vec<(Source, String)>, cutoff: SystemTime) -> Vec<(Source, String, Outcome)> {
    let roots = roots();
    let mut found_maps: HashMap<&'static str, HashMap<String, Found>> = HashMap::new();
    let mut readers: HashMap<&'static str, Reader> = HashMap::new();
    let mut out = Vec::with_capacity(wanted.len());
    for (source, id) in wanted {
        if !found_maps.contains_key(source.id()) {
            let map = discover(source, &roots).unwrap_or_default().into_iter().map(|row| (row.id.clone(), row)).collect();
            found_maps.insert(source.id(), map);
        }
        let outcome = match found_maps.get(source.id()).and_then(|map| map.get(&id)) {
            None => Outcome::NotFound,
            Some(row) if row.updated < cutoff => Outcome::Older,
            Some(row) => {
                if !readers.contains_key(source.id()) { readers.insert(source.id(), Reader::new(source)); }
                let reader = readers.get_mut(source.id()).expect("the reader was just made");
                match reader.read(row) { Ok(prepared) => Outcome::Ready(Box::new(prepared)), Err(reason) => Outcome::Failed(clip(&one_line(&reason), 200)) }
            }
        };
        out.push((source, id, outcome));
    }
    out
}

async fn apply(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let since = since_days(p)?;
    let cutoff = cutoff(since);
    let assignment = parse_assignment(st, p).await?;
    let batch_key = match p.get("projectKey") {
        None | Some(Value::Null) => None,
        Some(value) => {
            if p.get("items").is_some() { return Err(invalid("Give either items or projectKey, not both.")); }
            Some(value.as_str().map(str::trim).filter(|key| !key.is_empty()).ok_or_else(|| invalid("projectKey must be a folder key from import.projects."))?.to_owned())
        }
    };
    let mut next = Value::Null;
    let mut remaining = 0u64;
    let mut batch_project: Option<String> = None;
    let mut wanted: Vec<(Source, String)> = Vec::new();
    if let Some(key) = batch_key {
        let sources = source_filter(p)?;
        let offset = match p.get("offset") {
            None | Some(Value::Null) => 0usize,
            Some(value) => value.as_u64().ok_or_else(|| invalid("offset must be a whole number."))? as usize,
        };
        let (groups, _truncated, _older, _errors) = tokio::task::spawn_blocking(move || project_groups(since, &sources)).await.map_err(internal)?;
        let group = groups.into_iter().find(|group| group.key == key).ok_or_else(|| invalid("That folder is no longer in the window. Scan again."))?;
        let total = group.found.len();
        let page = group.found.into_iter().skip(offset).take(MAX_CONVERSATIONS).collect::<Vec<_>>();
        if offset + page.len() < total { next = json!(offset + page.len()); }
        remaining = total.saturating_sub(offset + page.len()) as u64;
        batch_project = Some(group.path);
        wanted = page.into_iter().map(|row| (row.source, row.id)).collect();
    } else {
        let items = p["items"].as_array().cloned().filter(|items| !items.is_empty()).ok_or_else(|| invalid("items must list at least one conversation."))?;
        for item in &items {
            let source = Source::parse(item["source"].as_str().unwrap_or("")).ok_or_else(|| invalid("every item needs a source: claude, codex or opencode."))?;
            let id = item["conversationId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("every item needs a conversationId."))?;
            if !wanted.iter().any(|(known, known_id)| *known == source && known_id == id) { wanted.push((source, id.to_owned())); }
        }
        if wanted.len() > MAX_CONVERSATIONS { return Err(invalid(&format!("at most {MAX_CONVERSATIONS} conversations can be imported at once."))); }
    }

    let import_id = Uuid::new_v4().to_string();
    let total = wanted.len();
    let prepared = tokio::task::spawn_blocking(move || prepare_wanted(wanted, cutoff)).await.map_err(internal)?;

    let runtimes = st.runtimes.read().await.clone();
    let agents = st.db.agent_list(false, None).map_err(internal)?;
    let projects = st.db.project_list().map_err(internal)?;
    let mut suggester = Suggester::new(st, &runtimes, &agents, &projects);
    let mut project_cache: HashMap<String, Option<String>> = HashMap::new();
    // The project of a whole-folder import is settled before the first write.
    let mut assignment = assignment;
    let mut created_project: Option<Value> = None;
    if assignment.project_id.is_none() {
        if let Some(folder) = assignment.project_path.clone().or_else(|| batch_project.clone()) {
            let (id, created) = find_or_create_project(st, &folder).await?;
            if created { created_project = st.db.project_get(&id).ok(); }
            assignment.project_id = Some(id);
        }
    }
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut imported = Vec::new();
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut written_bytes = 0u64;
    for (index, (source, conversation_id, outcome)) in prepared.into_iter().enumerate() {
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
        let prepared = match outcome {
            Outcome::NotFound => {
                failed += 1;
                st.emit("import.progress", progress("failed", json!({"reason":"not_found","message":"That conversation is no longer on this computer."}))).await;
                continue;
            }
            Outcome::Older => {
                skipped += 1;
                st.emit("import.progress", progress("skipped", json!({"reason":"older_than_window","message":format!("It was last changed more than {since} days ago.")}))).await;
                continue;
            }
            Outcome::Failed(reason) => {
                failed += 1;
                st.emit("import.progress", progress("failed", json!({"reason":"unreadable","message":reason}))).await;
                continue;
            }
            Outcome::Ready(prepared) => *prepared,
        };
        st.emit("import.progress", progress("started", json!({}))).await;
        if let Ok(Some(ledger)) = st.db.import_ledger(source.id(), &conversation_id) {
            if ledger["sha256"].as_str() == Some(prepared.sha256.as_str()) {
                skipped += 1;
                st.emit("import.progress", progress("skipped", json!({"reason":"already_imported","importedSessionId":ledger["sessionId"].clone(),"title":prepared.transcript.title.clone().unwrap_or_default()}))).await;
                continue;
            }
        }
        // Measured before writing: one call never stores more than its budget.
        if written_bytes.saturating_add(prepared.transcript.bytes) > batch_limit() {
            skipped += 1;
            st.emit("import.progress", progress("skipped", json!({"reason":"batch_budget","message":"This import reached its size limit; import the rest in another call."}))).await;
            continue;
        }
        written_bytes += prepared.transcript.bytes;
        let transcript = prepared.transcript;
        let message_count = transcript.message_count();
        let (agent_id, project_id) = match assignment.mode {
            AssignMode::Ask | AssignMode::Agent | AssignMode::Ceo => (assignment.agent_id.clone(), assignment.project_id.clone()),
            AssignMode::Project => {
                let project = match assignment.project_id.clone() {
                    Some(id) => Some(id),
                    None => {
                        let folder = transcript.project_path.clone();
                        if folder.trim().is_empty() { None } else if let Some(known) = project_cache.get(&path_key(&folder)) { known.clone() } else {
                            let made = find_or_create_project(st, &folder).await.map(|(id, created)| { if created { let _ = st.db.project_get(&id).map(|row| created_project = Some(row)); } id }).ok();
                            project_cache.insert(path_key(&folder), made.clone());
                            made
                        }
                    }
                };
                let haystack = format!("{} {}", transcript.title.clone().unwrap_or_default(), transcript.project_path);
                let (agent, _reason, _detail) = suggester.suggest(source.id(), project.as_deref(), &haystack);
                (agent.as_str().map(str::to_owned), project)
            }
        };
        let assigned_runtime = agent_id.as_deref().and_then(|id| st.db.agent_get(id).ok()).and_then(|agent| agent["runtimeId"].as_str().map(str::to_owned));
        let assigned_provider = assigned_runtime.as_deref().and_then(|runtime_id| provider_of_runtime(&runtimes, runtime_id));
        // A bot on another agent could not resume this conversation anyway, so the
        // provider's own session id is not kept for it.
        let can_resume = Path::new(&prepared.source_path).is_file() && assigned_provider.as_deref().is_none_or(|provider| provider == source.id());
        let runtime_id = runtime_for(&runtimes, source, assigned_runtime.as_deref());
        let input = json!({
            "sessionId": Uuid::new_v4().to_string(),
            "runtimeId": runtime_id,
            "provider": source.id(),
            "providerSessionId": if can_resume { transcript.provider_session_id.clone() } else { None },
            "canResumeWith": if can_resume { json!(source.id()) } else { Value::Null },
            "projectPath": transcript.project_path,
            "title": transcript.title.clone().filter(|title| !title.trim().is_empty()).unwrap_or_else(|| format!("{} conversation", source.label())),
            "agentId": agent_id,
            "projectId": project_id,
            "source": source.id(),
            "conversationId": conversation_id.clone(),
            "sourcePath": prepared.source_path,
            "sha256": prepared.sha256,
            "importedAt": now.clone(),
            "createdAt": transcript.created_at,
            "messageCount": message_count,
            "truncated": transcript.truncation.is_some(),
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
    let result = json!({
        "importId": import_id,
        "imported": imported.len(),
        "skipped": skipped,
        "failed": failed,
        "sessions": imported,
        "next": next,
        "remaining": remaining,
        "project": created_project,
    });
    st.emit("import.completed", json!({"importId": import_id, "imported": imported.len(), "skipped": skipped, "failed": failed, "conversationIds": imported})).await;
    Ok(result)
}

/// The runtime a conversation runs on: the receiving bot's own when it speaks the
/// same provider, else the first runtime of the provider that wrote it, else the
/// provider's name (a row that names no runtime, which only a read-only import has).
fn runtime_for(runtimes: &[Value], source: Source, agent_runtime: Option<&str>) -> String {
    if let Some(runtime_id) = agent_runtime {
        if provider_of_runtime(runtimes, runtime_id).as_deref() == Some(source.id()) { return runtime_id.to_owned(); }
    }
    runtime_for_provider(runtimes, source.id()).and_then(|runtime| runtime["id"].as_str()).map(str::to_owned).unwrap_or_else(|| source.id().to_owned())
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
    // B6: a folder, not only a project that already exists.
    let project_from_path = p.get("projectPath").and_then(Value::as_str).map(str::trim).filter(|path| !path.is_empty()).map(str::to_owned);
    if project_from_path.is_some() && p.get("projectId").is_some_and(|value| !value.is_null()) { return Err(invalid("Give projectId or projectPath, not both.")); }
    let project = match p.get("projectId") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(id)) if !id.trim().is_empty() => {
            st.db.project_get(id.trim()).map_err(|_| invalid("That project no longer exists."))?;
            Some(Some(id.trim().to_owned()))
        }
        Some(_) => return Err(invalid("projectId must be a project id or null.")),
    };
    let project = match project_from_path {
        Some(folder) => Some(Some(find_or_create_project(st, &folder).await?.0)),
        None => project,
    };
    let runtimes = st.runtimes.read().await.clone();
    let mut runtime = None;
    let mut clear_provider = false;
    if let Some(Some(bot)) = &agent {
        let bot_provider = bot["runtimeId"].as_str().and_then(|id| provider_of_runtime(&runtimes, id));
        if bot_provider.as_deref() == Some(import["source"].as_str().unwrap_or("")) {
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
// import.continue

async fn continue_with(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let session_id = p["sessionId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("sessionId is required."))?;
    let agent_id = p["agentId"].as_str().map(str::trim).filter(|id| !id.is_empty()).ok_or_else(|| invalid("agentId is required."))?;
    let note = match p.get("note") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.chars().count() > MAX_CONTINUE_NOTE { return Err(invalid(&format!("note must be at most {MAX_CONTINUE_NOTE} characters."))); }
            (!text.is_empty()).then(|| text.to_owned())
        }
        Some(_) => return Err(invalid("note must be a string.")),
    };
    let import = st.db.session_import_meta(session_id).map_err(internal)?
        .ok_or_else(|| invalid("That conversation was not imported."))?;
    let session = st.db.session_brief(session_id).map_err(internal)?;
    let agent = st.db.agent_get(agent_id).map_err(|_| invalid("That bot no longer exists."))?;
    if agent["archived"] == true || agent["hidden"] == true { return Err(invalid("That bot cannot take a conversation.")); }
    let runtimes = st.runtimes.read().await.clone();
    let agent_runtime = agent["runtimeId"].as_str().map(str::to_owned);
    let agent_provider = agent_runtime.as_deref().and_then(|runtime| provider_of_runtime(&runtimes, runtime));
    let source = import["source"].as_str().unwrap_or("").to_owned();
    let source_path = import["sourcePath"].as_str().unwrap_or("");
    let source_present = !source_path.is_empty() && Path::new(source_path).is_file() && session["providerSessionId"].as_str().is_some_and(|id| !id.is_empty());
    if agent_provider.as_deref() == Some(source.as_str()) && source_present {
        // Continue: the same agent still has the file, so it resumes natively.
        let summary = st.db.import_assign(session_id, Some(Some(agent_id)), agent_runtime.as_deref(), None, false).map_err(internal)?;
        st.emit("session.changed", summary.clone()).await;
        return Ok(json!({"mode":"continue","session":summary,"draft":Value::Null,"importedSessionId":session_id}));
    }
    // Carry over: a new conversation for the employee, from a bounded draft.
    let project_id = session["projectId"].as_str()
        .filter(|id| st.db.project_get(id).ok().and_then(|project| project["path"].as_str().map(|path| Path::new(path).is_dir())).unwrap_or(false));
    let title = session["title"].as_str().map(|title| title.chars().take(120).collect::<String>()).filter(|title| !title.is_empty()).unwrap_or_else(|| "Carried over".to_owned());
    let mut params = json!({"agentId": agent_id, "title": title});
    params["projectId"] = project_id.map(|id| json!(id)).unwrap_or(Value::Null);
    let created = crate::team::agent_conversation_create(st, &params).await?;
    let new_id = created["session"]["id"].as_str().ok_or_else(|| internal("the conversation was not created"))?.to_owned();
    let base = crate::handoff_draft(st, &json!({"fromSessionId": session_id, "note": note}))?;
    let mut text = base["draft"].as_str().unwrap_or("").to_owned();
    if !text.is_empty() { text.push_str("\n\n"); }
    text.push_str(&format!(
        "The full history is the imported conversation {session_id} (from {}). Read it with read_conversation before you start; search_history finds a phrase in it. This summary is only the latest exchanges.",
        source_label(&source)
    ));
    let draft = json!({
        "text": text,
        "carried": base["carried"].clone(),
        "importedSessionId": session_id,
        "source": source.clone(),
        "sourceConversationId": import["conversationId"].clone(),
    });
    // The two links G8 asked for: the new conversation says where it came from,
    // and the imported one says where it went.
    st.db.merge_session_data(&new_id, &json!({"origin":{"importedFrom":{"sessionId":session_id,"source":source,"conversationId":import["conversationId"].clone()}}})).map_err(internal)?;
    st.db.merge_session_data(session_id, &json!({"import":{"continuedAs":new_id}})).map_err(internal)?;
    let summary = st.db.session_event_summary(&new_id).map_err(internal)?;
    st.emit("session.changed", summary.clone()).await;
    if let Ok(changed) = st.db.session_event_summary(session_id) { st.emit("session.changed", changed).await; }
    Ok(json!({"mode":"carry_over","session":summary,"draft":draft,"importedSessionId":session_id}))
}

// ---------------------------------------------------------------------------
// The read-only guard

/// A conversation that was imported is history, not a conversation Retinue owns:
/// a turn may only continue it when the bot's own agent wrote it and its file is
/// still where it was. Everything else is refused with `imported_read_only`, so
/// the window can offer Continue or a carry-over instead.
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
                same_agent = provider_of_runtime(&runtimes, runtime_id).as_deref() == Some(source);
            }
        }
    }
    if present && same_agent { return Ok(()) }
    Err(derr(
        "imported_read_only",
        &format!("This conversation was imported from {} and is read-only here. Continue with an employee, or carry it over to one.", source_label(source)),
        StatusCode::FORBIDDEN,
    ))
}

// ---------------------------------------------------------------------------
// Routing

pub(crate) async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "import.scan" => scan(st, p).await,
        "import.preview" => preview(st, p).await,
        "import.projects" => projects(st, p).await,
        "import.apply" => apply(st, p).await,
        "import.distribute" => distribute(st, p).await,
        "import.assign" => assign(st, p).await,
        "import.continue" => continue_with(st, p).await,
        _ => Err(derr("unsupported", "unsupported import method", StatusCode::NOT_FOUND)),
    }
}
