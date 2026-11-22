//! Tests for `import.rs`: one small fixture per source format, the employee
//! fitting (suggestions, project roll-up, continue and carry over), the size
//! budgets, and the promise that credentials, thinking, instruction blocks and
//! file snapshots are never stored. The fixtures are synthetic; no real
//! conversation is in this repository.

#![allow(clippy::await_holding_lock)]

use crate::import::{self, Roots, TEST_ROOTS};
use crate::phase2a_tests::state;
use crate::AppState;
use axum::http::StatusCode;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// `TEST_ROOTS` is one process-wide seam, so the tests that use it take turns.
static FIXTURE_LOCK: Mutex<()> = Mutex::new(());

struct Fixture {
    dir: std::path::PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(mut roots) = TEST_ROOTS.lock() {
            *roots = None;
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("retinue-import-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, lines: &[&str]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, lines.join("\n")).unwrap();
}

fn set_roots(roots: Roots) {
    if let Ok(mut slot) = TEST_ROOTS.lock() {
        *slot = Some(roots);
    }
}

/// A path as JSON text, so a temp folder's backslashes need no hand escaping.
fn json_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn claude_fixture() -> Fixture {
    let guard = FIXTURE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = fixture_dir("claude");
    let projects = dir.join(".claude").join("projects");
    let session = projects.join("C--work-billing").join("sess-1.jsonl");
    write(&session, &[
        r#"{"type":"custom-title","customTitle":"Invoice parser tests","sessionId":"sess-1"}"#,
        r#"{"type":"file-history-snapshot","snapshot":{"content":"SECRET FILE CONTENT"},"sessionId":"sess-1"}"#,
        r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"text","text":"Why is the invoice parser skipping line 2?"}]},"uuid":"u1","timestamp":"2026-11-13T10:00:00.000Z","cwd":"C:/work/billing","sessionId":"sess-1"}"#,
        r#"{"type":"attachment","attachment":{"type":"environment","workingDirectory":"C:/work/billing","osVersion":"SECRET ENVIRONMENT"},"cwd":"C:/work/billing","sessionId":"sess-1"}"#,
        r#"{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"thinking","thinking":"PRIVATE REASONING","signature":"sig"},{"type":"text","text":"The parser treats the second line as a header."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"npm test -- invoice","description":"run the parser tests"}}]},"timestamp":"2026-11-13T10:00:05.000Z","cwd":"C:/work/billing","sessionId":"sess-1"}"#,
        r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"tool_result","content":"TOOL OUTPUT SECRET"}]},"timestamp":"2026-11-13T10:00:06.000Z","sessionId":"sess-1"}"#,
        r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":[{"type":"text","text":"SUBPROMPT SECRET"}]},"timestamp":"2026-11-13T10:00:07.000Z","sessionId":"sess-1"}"#,
        r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"BASE64IMAGE"}}]},"timestamp":"2026-11-13T10:00:08.000Z","sessionId":"sess-1"}"#,
    ]);
    set_roots(Roots { claude_projects: Some(projects), codex_sessions: None, opencode_db: None });
    Fixture { dir, _guard: guard }
}

/// Two Claude conversations in one folder, for project mode and its paging.
fn claude_pair_fixture() -> Fixture {
    let guard = FIXTURE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = fixture_dir("claude-pair");
    let projects = dir.join(".claude").join("projects");
    let folder = dir.join("workspace").join("billing");
    std::fs::create_dir_all(&folder).unwrap();
    let cwd = json_path(&folder);
    for (id, text, at) in [("sess-a", "Import the invoices.", "2026-11-13T10:00:00.000Z"), ("sess-b", "Add a CSV export.", "2026-11-12T10:00:00.000Z")] {
        let session = projects.join("billing").join(format!("{id}.jsonl"));
        let user = format!(r#"{{"type":"user","isSidechain":false,"message":{{"role":"user","content":[{{"type":"text","text":"{text}"}}]}},"timestamp":"{at}","cwd":"{cwd}","sessionId":"{id}"}}"#);
        let assistant = format!(r#"{{"type":"assistant","isSidechain":false,"message":{{"role":"assistant","content":[{{"type":"text","text":"Done."}}]}},"timestamp":"{at}","cwd":"{cwd}","sessionId":"{id}"}}"#);
        write(&session, &[&user, &assistant]);
    }
    set_roots(Roots { claude_projects: Some(projects), codex_sessions: None, opencode_db: None });
    Fixture { dir, _guard: guard }
}

fn codex_fixture() -> Fixture {
    let guard = FIXTURE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = fixture_dir("codex");
    let sessions = dir.join(".codex").join("sessions");
    let rollout = sessions.join("2026").join("11").join("13").join("rollout-2026-11-13T11-00-00-0190aaaa-bbbb-cccc-dddd-eeeeffff0000.jsonl");
    write(&rollout, &[
        r#"{"timestamp":"2026-11-13T11:00:00.000Z","type":"session_meta","payload":{"session_id":"th-1","id":"th-1","cwd":"C:/work/api","creator_user_id":"user_SECRET","creator_account_id":"acct_SECRET","base_instructions":"BASE INSTRUCTIONS SECRET","originator":"codex_cli"}}"#,
        r#"{"timestamp":"2026-11-13T11:00:01.000Z","type":"turn_context","payload":{"approval_policy":"never","sandbox_policy":"SANDBOX SECRET"}}"#,
        r#"{"timestamp":"2026-11-13T11:00:02.000Z","type":"world_state","payload":{"text":"AGENTS.md SECRET"}}"#,
        r#"{"timestamp":"2026-11-13T11:00:03.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<user_instructions>INSTRUCTIONS SECRET</user_instructions>"},{"type":"input_text","text":"Add a health check endpoint."}]}}"#,
        r#"{"timestamp":"2026-11-13T11:00:04.000Z","type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"REASONING SECRET"}]}}"#,
        r#"{"timestamp":"2026-11-13T11:00:05.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Added it to the router."}]}}"#,
        r#"{"timestamp":"2026-11-13T11:00:06.000Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":\"cargo test\"}"}}"#,
        r#"{"timestamp":"2026-11-13T11:00:07.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"TOOL OUTPUT SECRET"}}"#,
    ]);
    set_roots(Roots { claude_projects: None, codex_sessions: Some(sessions), opencode_db: None });
    Fixture { dir, _guard: guard }
}

/// OpenCode 2 keeps everything in one database. Only the two tables the importer
/// reads are created here; a `credential` row proves it is never touched.
fn opencode_fixture() -> Fixture {
    let guard = FIXTURE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = fixture_dir("opencode");
    let folder = dir.join(".local").join("share").join("opencode");
    std::fs::create_dir_all(&folder).unwrap();
    let db = folder.join("opencode.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE session_v2(id TEXT PRIMARY KEY,parent_id TEXT,title TEXT,directory TEXT,time_created INTEGER,time_updated INTEGER);
             CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,seq INTEGER,time_created INTEGER,data TEXT);
             CREATE TABLE credential(id TEXT PRIMARY KEY,integration_id TEXT,value TEXT);
             INSERT INTO credential VALUES('c1','opencode','sk-SECRET-TOKEN');",
        )
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    connection
        .execute("INSERT INTO session_v2 VALUES('ses_1',NULL,'Billing import','C:/work/billing',?1,?2)", rusqlite::params![now - 60_000, now])
        .unwrap();
    // A second, older conversation, so one changes without the other.
    connection
        .execute("INSERT INTO session_v2 VALUES('ses_2',NULL,'Old session','C:/work/old',?1,?2)", rusqlite::params![now - 240_000, now - 180_000])
        .unwrap();
    let user = json!({"text":"Where does the CSV importer live?","files":[]}).to_string();
    let assistant = json!({"content":[
        {"type":"reasoning","text":"REASONING SECRET"},
        {"type":"text","text":"In src/billing/csv.ts."},
        {"type":"tool","name":"shell","state":{"status":"completed","input":{"command":"ls src/billing"}}}
    ]}).to_string();
    connection
        .execute("INSERT INTO session_message VALUES('m1','ses_1','user',1,?1,?2)", rusqlite::params![now - 50_000, user])
        .unwrap();
    connection
        .execute("INSERT INTO session_message VALUES('m2','ses_1','assistant',2,?1,?2)", rusqlite::params![now - 40_000, assistant])
        .unwrap();
    // A real database has 26,000 of these bookkeeping rows: one here proves the filter.
    connection
        .execute("INSERT INTO session_message VALUES('m3','ses_1','model-switched',3,?1,'{\"model\":\"x\"}')", rusqlite::params![now - 40_000])
        .unwrap();
    connection
        .execute("INSERT INTO session_message VALUES('m4','ses_2','user',1,?1,'{\"text\":\"An older note.\",\"files\":[]}')", rusqlite::params![now - 180_000])
        .unwrap();
    drop(connection);
    set_roots(Roots { claude_projects: None, codex_sessions: None, opencode_db: Some(db) });
    Fixture { dir, _guard: guard }
}

/// One OpenCode database with an archived column and an old conversation, for
/// the window and the archived flag.
fn opencode_archive_fixture() -> Fixture {
    let guard = FIXTURE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = fixture_dir("opencode-archive");
    let folder = dir.join(".local").join("share").join("opencode");
    std::fs::create_dir_all(&folder).unwrap();
    let db = folder.join("opencode.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE session_v2(id TEXT PRIMARY KEY,parent_id TEXT,title TEXT,directory TEXT,time_created INTEGER,time_updated INTEGER,archived INTEGER);
             CREATE TABLE session_message(id TEXT PRIMARY KEY,session_id TEXT,type TEXT,seq INTEGER,time_created INTEGER,data TEXT);",
        )
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    connection.execute("INSERT INTO session_v2 VALUES('ses_new',NULL,'Recent work','C:/work/api',?1,?2,0)", rusqlite::params![now - 10_000, now]).unwrap();
    connection.execute("INSERT INTO session_v2 VALUES('ses_old',NULL,'Last month','C:/work/api',?1,?2,0)", rusqlite::params![now - 40 * 86_400_000, now - 40 * 86_400_000]).unwrap();
    connection.execute("INSERT INTO session_v2 VALUES('ses_archived',NULL,'Archived work','C:/work/api',?1,?2,1)", rusqlite::params![now - 20_000, now - 20_000]).unwrap();
    for (id, session, at) in [("n1", "ses_new", now - 9_000), ("a1", "ses_archived", now - 19_000)] {
        connection.execute("INSERT INTO session_message VALUES(?1,?2,'user',1,?3,'{\"text\":\"A note.\",\"files\":[]}')", rusqlite::params![id, session, at]).unwrap();
    }
    drop(connection);
    set_roots(Roots { claude_projects: None, codex_sessions: None, opencode_db: Some(db) });
    Fixture { dir, _guard: guard }
}

/// Two agents, one folder, one worktree of it, and a conversation whose folder
/// is gone: the shape `import.projects` has to group and flag.
fn rollup_fixture() -> Fixture {
    let guard = FIXTURE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = fixture_dir("rollup");
    let main = dir.join("workspace").join("billing");
    let worktree = dir.join("workspace").join("billing-wt");
    std::fs::create_dir_all(main.join(".git").join("worktrees").join("wt")).unwrap();
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(worktree.join(".git"), format!("gitdir: {}/.git/worktrees/wt", json_path(&main))).unwrap();
    let missing = dir.join("workspace").join("gone");
    let projects = dir.join(".claude").join("projects");
    for (id, folder, text) in [("sess-main", &main, "Main conversation."), ("sess-wt", &worktree, "Worktree conversation."), ("sess-gone", &missing, "Lost folder.")] {
        let cwd = json_path(folder);
        let session = projects.join("billing").join(format!("{id}.jsonl"));
        let line = format!(r#"{{"type":"user","isSidechain":false,"message":{{"role":"user","content":[{{"type":"text","text":"{text}"}}]}},"timestamp":"2026-11-13T10:00:00.000Z","cwd":"{cwd}","sessionId":"{id}"}}"#);
        write(&session, &[&line]);
    }
    let sessions = dir.join(".codex").join("sessions");
    let cwd = json_path(&main);
    let meta = format!(r#"{{"timestamp":"2026-11-13T11:00:00.000Z","type":"session_meta","payload":{{"session_id":"th-1","cwd":"{cwd}"}}}}"#);
    write(&sessions.join("2026").join("11").join("13").join("rollout-2026-11-13T11-00-00-0190aaaa-bbbb-cccc-dddd-eeeeffff0000.jsonl"), &[
        &meta,
        r#"{"timestamp":"2026-11-13T11:00:03.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Codex in the same folder."}]}}"#,
    ]);
    set_roots(Roots { claude_projects: Some(projects), codex_sessions: Some(sessions), opencode_db: None });
    Fixture { dir, _guard: guard }
}

async fn add_runtime(st: &AppState, id: &str, provider: &str) {
    let runtime = json!({"id":id,"provider":provider,"status":"online"});
    st.db.upsert_runtime(&runtime).unwrap();
    st.runtimes.write().await.push(runtime);
}

fn stored(st: &AppState, session_id: &str) -> Value {
    st.db.session_detail(session_id).unwrap()
}

async fn apply_one(st: &crate::AppState, source: &str, conversation: &str) -> Value {
    import::rpc(st, "import.apply", &json!({"items":[{"source":source,"conversationId":conversation}]})).await.unwrap()
}

#[tokio::test]
async fn a_claude_conversation_imports_its_text_and_tools_and_nothing_private() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    let result = apply_one(&st, "claude", "sess-1").await;
    assert_eq!(result["imported"], 1, "{result}");
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    let detail = stored(&st, &session_id);
    assert_eq!(detail["state"], "completed");
    assert_eq!(detail["title"], "Invoice parser tests");
    assert_eq!(detail["provider"], "claude");
    assert_eq!(detail["projectPath"], "C:/work/billing");
    let text = detail.to_string();
    assert!(text.contains("Why is the invoice parser skipping line 2?"));
    assert!(text.contains("The parser treats the second line as a header."));
    assert!(text.contains("(image omitted)"));
    // Never copied: private reasoning, the environment snapshot, a file snapshot,
    // a sub-agent's own prompt, a tool's output and image bytes.
    for secret in ["PRIVATE REASONING", "SECRET ENVIRONMENT", "SECRET FILE CONTENT", "SUBPROMPT SECRET", "TOOL OUTPUT SECRET", "BASE64IMAGE"] {
        assert!(!text.contains(secret), "{secret} reached the stored conversation");
    }
    let tools = detail["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["title"], "Bash: npm test -- invoice");
    assert_eq!(tools[0]["state"], "completed");
    let import = st.db.session_import_meta(&session_id).unwrap().unwrap();
    assert_eq!(import["source"], "claude");
    assert_eq!(import["messageCount"], 3);
    assert_eq!(import["sha256"].as_str().unwrap().len(), 64);
    // The list row carries the same read-only mark as the event summary, so the
    // window's banner survives a restart (22 November 2026).
    let listed = st.db.session_list(true).unwrap().into_iter().find(|row| row["id"] == session_id.as_str()).unwrap();
    assert_eq!(listed["imported"], st.db.session_event_summary(&session_id).unwrap()["imported"], "{listed}");
    assert_eq!(listed["imported"]["source"], "claude");
    assert_eq!(listed["imported"]["readOnly"], true);
    let own = st.db.session_list(true).unwrap();
    assert!(own.iter().filter(|row| row["id"] != session_id.as_str()).all(|row| row.get("imported").is_none()), "a conversation started here has no imported mark");
}

#[tokio::test]
async fn importing_the_same_file_twice_stores_it_once() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    let first = apply_one(&st, "claude", "sess-1").await;
    assert_eq!(first["imported"], 1);
    let second = apply_one(&st, "claude", "sess-1").await;
    assert_eq!(second["imported"], 0);
    assert_eq!(second["skipped"], 1);
    assert_eq!(st.db.import_sessions(&[]).unwrap().len(), 1);
}

#[tokio::test]
async fn a_codex_rollout_drops_accounts_instructions_and_reasoning() {
    let _fixture = codex_fixture();
    let (st, _events) = state();
    let result = apply_one(&st, "codex", "rollout-2026-11-13T11-00-00-0190aaaa-bbbb-cccc-dddd-eeeeffff0000").await;
    assert_eq!(result["imported"], 1, "{result}");
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    let detail = stored(&st, &session_id);
    let text = detail.to_string();
    assert!(text.contains("Add a health check endpoint."));
    assert!(text.contains("Added it to the router."));
    // The title falls back to the first user message.
    assert_eq!(detail["title"], "Add a health check endpoint.");
    assert_eq!(detail["projectPath"], "C:/work/api");
    for secret in ["user_SECRET", "acct_SECRET", "BASE INSTRUCTIONS SECRET", "SANDBOX SECRET", "AGENTS.md SECRET", "INSTRUCTIONS SECRET", "REASONING SECRET", "TOOL OUTPUT SECRET"] {
        assert!(!text.contains(secret), "{secret} reached the stored conversation");
    }
    let tools = detail["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["title"], "shell: cargo test");
    // The same-agent case keeps Codex's own thread id, so `thread/resume` can work.
    assert_eq!(detail["providerSessionId"], "th-1");
    assert_eq!(detail["resumable"], true);
}

#[tokio::test]
async fn an_opencode_database_is_read_through_a_copy_and_never_its_credentials() {
    let _fixture = opencode_fixture();
    let (st, _events) = state();
    let preview = import::rpc(&st, "import.preview", &json!({"source":"opencode"})).await.unwrap();
    let row = &preview["rows"][0];
    assert_eq!(row["conversationId"], "ses_1");
    assert_eq!(row["title"], "Billing import");
    assert_eq!(row["messageCount"], 2);
    assert_eq!(row["alreadyImported"], false);
    assert_eq!(row["archived"], Value::Null);
    let result = apply_one(&st, "opencode", "ses_1").await;
    assert_eq!(result["imported"], 1, "{result}");
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    let detail = stored(&st, &session_id);
    let text = detail.to_string();
    assert!(text.contains("Where does the CSV importer live?"));
    assert!(text.contains("In src/billing/csv.ts."));
    for secret in ["REASONING SECRET", "sk-SECRET-TOKEN", "model-switched"] {
        assert!(!text.contains(secret), "{secret} reached the stored conversation");
    }
    assert_eq!(detail["tools"][0]["title"], "shell: ls src/billing");
    assert_eq!(detail["providerSessionId"], "ses_1");
}

#[tokio::test]
async fn an_opencode_conversation_is_marked_imported_and_a_repeat_write_is_harmless() {
    let _fixture = opencode_fixture();
    let (st, _events) = state();
    let first = apply_one(&st, "opencode", "ses_1").await;
    assert_eq!(first["imported"], 1);
    let session_id = first["sessions"][0].as_str().unwrap().to_owned();
    let preview = import::rpc(&st, "import.preview", &json!({"source":"opencode"})).await.unwrap();
    let row = &preview["rows"][0];
    assert_eq!(row["alreadyImported"], true);
    assert_eq!(row["importedSessionId"], session_id);
    // The database changes when a conversation that was not imported does; the
    // imported conversation's own text is its identity, so it is not copied again.
    let db = {
        let roots = TEST_ROOTS.lock().unwrap().clone().unwrap();
        roots.opencode_db.unwrap()
    };
    let connection = Connection::open(&db).unwrap();
    connection.execute("INSERT INTO session_message VALUES('m5','ses_2','assistant',2,0,'{\"content\":[]}')", []).unwrap();
    drop(connection);
    let second = apply_one(&st, "opencode", "ses_1").await;
    assert_eq!(second["imported"], 0);
    assert_eq!(second["skipped"], 1);
    assert_eq!(st.db.import_sessions(&[]).unwrap().len(), 1);
}

#[tokio::test]
async fn scan_and_preview_report_what_is_on_disk() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    let scan = import::rpc(&st, "import.scan", &json!({})).await.unwrap();
    let sources = scan["sources"].as_array().unwrap();
    // Codex and OpenCode are not in this fixture, so they are not offered at all.
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["source"], "claude");
    assert_eq!(sources[0]["conversations"], 1);
    assert_eq!(sources[0]["older"], 0);
    assert!(sources[0]["bytes"].as_u64().unwrap() > 0);
    // No Claude Code runtime is installed, so the scan says so.
    assert_eq!(sources[0]["installed"], false);
    assert_eq!(sources[0]["runtimeId"], Value::Null);
    let preview = import::rpc(&st, "import.preview", &json!({"source":"claude"})).await.unwrap();
    assert_eq!(preview["total"], 1);
    let row = &preview["rows"][0];
    assert_eq!(row["title"], "Invoice parser tests");
    assert_eq!(row["firstUser"], "Why is the invoice parser skipping line 2?");
    // The file is there, but no agent of that provider is, so it cannot resume.
    assert_eq!(row["canResumeWith"], Value::Null);
    assert_eq!(row["alreadyImported"], false);
}

#[tokio::test]
async fn scan_counts_what_the_window_hides_and_reports_the_source_installed() {
    let _fixture = opencode_archive_fixture();
    let (st, _events) = state();
    add_runtime(&st, "rt-opencode", "opencode").await;
    let scan = import::rpc(&st, "import.scan", &json!({"sinceDays":30})).await.unwrap();
    let source = &scan["sources"][0];
    assert_eq!(scan["sinceDays"], 30);
    assert_eq!(source["source"], "opencode");
    assert_eq!(source["conversations"], 3);
    assert_eq!(source["recent"], 2);
    // Nothing disappears silently: the one the 30-day window hides is counted.
    assert_eq!(source["older"], 1);
    assert_eq!(source["installed"], true);
    assert_eq!(source["runtimeId"], "rt-opencode");
    assert_eq!(source["providerStatus"], "online");
    let preview = import::rpc(&st, "import.preview", &json!({"source":"opencode","sinceDays":30})).await.unwrap();
    assert_eq!(preview["total"], 2);
    assert_eq!(preview["older"], 1);
    assert_eq!(preview["rows"].as_array().unwrap().len(), 2);
    // The folder roll-up is a scan too, so it says what its window hides.
    let rollup = import::rpc(&st, "import.projects", &json!({"sinceDays":30})).await.unwrap();
    assert_eq!(rollup["older"], 1);
    assert_eq!(rollup["projects"].as_array().unwrap().len(), 1);
    // The source's own archived flag is carried where it has one.
    let archived = preview["rows"].as_array().unwrap().iter().find(|row| row["conversationId"] == "ses_archived").unwrap();
    assert_eq!(archived["archived"], true);
}

#[tokio::test]
async fn preview_says_can_resume_only_when_that_agent_is_installed() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    let before = import::rpc(&st, "import.preview", &json!({"source":"claude"})).await.unwrap();
    assert_eq!(before["rows"][0]["canResumeWith"], Value::Null);
    // Installing the agent makes the honest label available.
    add_runtime(&st, "rt-claude", "claude").await;
    let after = import::rpc(&st, "import.preview", &json!({"source":"claude"})).await.unwrap();
    assert_eq!(after["rows"][0]["canResumeWith"], "claude");
    let scan = import::rpc(&st, "import.scan", &json!({})).await.unwrap();
    assert_eq!(scan["sources"][0]["installed"], true);
    assert_eq!(scan["sources"][0]["runtimeId"], "rt-claude");
}

#[tokio::test]
async fn import_projects_groups_by_folder_across_agents_and_flags_a_missing_folder() {
    let fixture = rollup_fixture();
    let (st, _events) = state();
    let rollup = import::rpc(&st, "import.projects", &json!({})).await.unwrap();
    let projects = rollup["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 2, "{rollup}");
    let main = fixture.dir.join("workspace").join("billing");
    let billing = projects.iter().find(|project| project["path"].as_str().is_some_and(|path| Path::new(path).ends_with("billing"))).unwrap();
    // The worktree maps back to the folder it was cut from.
    assert_eq!(billing["total"], 3, "{billing}");
    assert_eq!(billing["exists"], true);
    assert_eq!(billing["existingProjectId"], Value::Null);
    let sources = billing["sources"].as_array().unwrap();
    assert!(sources.contains(&json!("claude")) && sources.contains(&json!("codex")));
    let rows = billing["conversations"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row["error"].is_null()));
    let main_text = main.to_string_lossy().to_string();
    let (project, _events) = st.db.project_create("Billing", Some(&main_text)).unwrap();
    let again = import::rpc(&st, "import.projects", &json!({})).await.unwrap();
    let billing = again["projects"].as_array().unwrap().iter().find(|project| project["total"] == 3).unwrap();
    assert_eq!(billing["existingProjectId"], project["id"]);
    let gone = again["projects"].as_array().unwrap().iter().find(|project| project["path"].as_str().is_some_and(|path| path.ends_with("gone"))).unwrap();
    assert_eq!(gone["exists"], false);
    assert_eq!(gone["total"], 1);
}

#[tokio::test]
async fn project_mode_imports_a_folder_once_and_a_repeat_does_nothing() {
    let fixture = claude_pair_fixture();
    let (st, _events) = state();
    let rollup = import::rpc(&st, "import.projects", &json!({})).await.unwrap();
    let key = rollup["projects"][0]["key"].as_str().unwrap().to_owned();
    // Paging: the older conversation first, with nothing left after it.
    let first = import::rpc(&st, "import.apply", &json!({"projectKey":key,"offset":1,"assign":{"mode":"project"}})).await.unwrap();
    assert_eq!(first["imported"], 1, "{first}");
    assert_eq!(first["next"], Value::Null);
    assert_eq!(first["remaining"], 0);
    assert_eq!(first["project"]["path"].as_str().unwrap(), json_path(&fixture.dir.join("workspace").join("billing")));
    // Exactly one project was made, and the rest of the folder came in under it.
    let second = import::rpc(&st, "import.apply", &json!({"projectKey":key,"assign":{"mode":"project"}})).await.unwrap();
    assert_eq!(second["imported"], 1, "{second}");
    assert_eq!(second["skipped"], 1);
    let projects = st.db.project_list().unwrap();
    assert_eq!(projects.len(), 1, "project mode creates exactly one project");
    let session_id = second["sessions"][0].as_str().unwrap();
    let detail = stored(&st, session_id);
    assert_eq!(detail["projectId"], projects[0]["id"]);
    // A repeat import writes nothing.
    let third = import::rpc(&st, "import.apply", &json!({"projectKey":key,"assign":{"mode":"project"}})).await.unwrap();
    assert_eq!(third["imported"], 0);
    assert_eq!(third["skipped"], 2);
    assert_eq!(st.db.project_list().unwrap().len(), 1);
    assert_eq!(st.db.import_sessions(&[]).unwrap().len(), 2);
}

#[tokio::test]
async fn a_suggestion_can_offer_a_hire_and_never_hires() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    add_runtime(&st, "rt-claude", "claude").await;
    let bot = st.db.agent_create(&json!({"name":"Billing","runtimeId":"rt-test","role":"billing invoices"})).unwrap().0;
    let suggestion = import::rpc(&st, "import.distribute", &json!({"items":[{"source":"claude","conversationId":"sess-1"}]})).await.unwrap();
    let row = &suggestion["suggestions"][0];
    // A preview row has no Retinue conversation yet.
    assert_eq!(row["sessionId"], Value::Null);
    assert_eq!(row["conversationId"], "sess-1");
    assert_eq!(row["title"], "Invoice parser tests");
    assert_eq!(row["agentId"], bot["id"]);
    // Nobody but the single Codex bot speaks Claude, so a hire is offered.
    assert_eq!(row["hire"]["runtimeId"], "rt-claude");
    assert!(row["hire"]["name"].as_str().is_some_and(|name| !name.is_empty()));
    assert!(!row["hire"]["presetId"].is_null());
    // Nothing was hired: the hire is the owner's to confirm.
    let agents = st.db.agent_list(false, None).unwrap();
    assert_eq!(agents.len(), 1);
    assert!(agents.iter().all(|agent| agent["runtimeId"] != "rt-claude"));
    // Once an employee speaks Claude, the hire goes away and the rule says why.
    let claude_bot = st.db.agent_create(&json!({"name":"Claude helper","runtimeId":"rt-claude"})).unwrap().0;
    let suggestion = import::rpc(&st, "import.distribute", &json!({"items":[{"source":"claude","conversationId":"sess-1"}]})).await.unwrap();
    let row = &suggestion["suggestions"][0];
    assert_eq!(row["agentId"], claude_bot["id"]);
    assert_eq!(row["reason"], "agent");
    assert_eq!(row["hire"], Value::Null);
}

#[tokio::test]
async fn distributing_many_preview_rows_copies_the_database_once_for_the_call() {
    let _fixture = opencode_fixture();
    let (st, _events) = state();
    import::reset_copy_count();
    let suggestions = import::rpc(&st, "import.distribute", &json!({"items":[
        {"source":"opencode","conversationId":"ses_1"},
        {"source":"opencode","conversationId":"ses_2"}
    ]})).await.unwrap();
    let rows = suggestions["suggestions"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{suggestions}");
    assert_eq!(rows[0]["conversationId"], "ses_1");
    assert_eq!(rows[0]["title"], "Billing import");
    assert_eq!(rows[0]["sessionId"], Value::Null);
    assert_eq!(rows[1]["conversationId"], "ses_2");
    assert_eq!(rows[1]["title"], "Old session");
    // One copy to discover, one to read the whole batch: never one per row.
    assert!(import::copy_count() <= 2, "the database was copied {} times for two rows", import::copy_count());
}

#[tokio::test]
async fn continue_and_carry_over_are_the_two_outcomes() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    add_runtime(&st, "rt-claude", "claude").await;
    let claude_bot = st.db.agent_create(&json!({"name":"Claude bot","runtimeId":"rt-claude"})).unwrap().0;
    let codex_bot = st.db.agent_create(&json!({"name":"Codex bot","runtimeId":"rt-test"})).unwrap().0;
    let result = apply_one(&st, "claude", "sess-1").await;
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    // The same agent still has the file: Continue.
    let continued = import::rpc(&st, "import.continue", &json!({"sessionId":session_id,"agentId":claude_bot["id"]})).await.unwrap();
    assert_eq!(continued["mode"], "continue");
    assert_eq!(continued["session"]["agentId"], claude_bot["id"]);
    assert_eq!(continued["session"]["imported"]["readOnly"], false);
    assert!(continued["draft"].is_null());
    // Another employee: Carry over, from a bounded and editable draft.
    let carried = import::rpc(&st, "import.continue", &json!({"sessionId":session_id,"agentId":codex_bot["id"],"note":"Keep it short."})).await.unwrap();
    assert_eq!(carried["mode"], "carry_over");
    let new_id = carried["session"]["id"].as_str().unwrap().to_owned();
    assert_ne!(new_id, session_id);
    assert_eq!(carried["session"]["agentId"], codex_bot["id"]);
    let text = carried["draft"]["text"].as_str().unwrap();
    assert!(text.contains("Keep it short."));
    assert!(text.contains("read_conversation"));
    assert!(text.contains(&session_id));
    assert!(text.chars().count() < 6_000, "the draft is bounded");
    // The two links: where it went and where it came from.
    let meta = st.db.session_import_meta(&session_id).unwrap().unwrap();
    assert_eq!(meta["continuedAs"], new_id);
    let origin = stored(&st, &new_id);
    assert_eq!(origin["origin"]["importedFrom"]["sessionId"], session_id);
    assert_eq!(origin["origin"]["importedFrom"]["source"], "claude");
    // The new conversation is a retry of Continue, not a new import.
    assert_eq!(import::rpc(&st, "import.continue", &json!({"sessionId":"own","agentId":claude_bot["id"]})).await.unwrap_err().1.code, "invalid_argument");
}

#[tokio::test]
async fn a_conversation_over_its_size_budget_stops_with_one_notice() {
    let guard = FIXTURE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = fixture_dir("oversize");
    let projects = dir.join(".claude").join("projects");
    let session = projects.join("billing").join("sess-big.jsonl");
    let big = "a".repeat(8_000);
    let mut lines: Vec<String> = Vec::new();
    lines.push(r#"{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"text","text":"Start."}]},"timestamp":"2026-11-13T10:00:00.000Z","cwd":"C:/work/billing","sessionId":"sess-big"}"#.to_owned());
    for index in 0..299 {
        let (role, text) = if index % 2 == 0 { ("assistant", &big) } else { ("user", &big) };
        lines.push(format!(r#"{{"type":"{role}","isSidechain":false,"message":{{"role":"{role}","content":[{{"type":"text","text":"{text}"}}]}},"timestamp":"2026-11-13T10:00:00.000Z","cwd":"C:/work/billing","sessionId":"sess-big"}}"#));
    }
    let refs = lines.iter().map(String::as_str).collect::<Vec<_>>();
    write(&session, &refs);
    set_roots(Roots { claude_projects: Some(projects), codex_sessions: None, opencode_db: None });
    let fixture = Fixture { dir, _guard: guard };
    let (st, _events) = state();
    let result = apply_one(&st, "claude", "sess-big").await;
    assert_eq!(result["imported"], 1, "{result}");
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    let meta = st.db.session_import_meta(&session_id).unwrap().unwrap();
    assert_eq!(meta["truncated"], true);
    let detail = stored(&st, &session_id);
    let text = detail.to_string();
    assert!(text.contains("larger than 2 MB"), "one notice says why the rest is gone");
    assert!(text.len() < 3_000_000, "the stored conversation is bounded");
    drop(fixture);
}

struct SmallBatch;

impl SmallBatch {
    fn of(bytes: u64) -> Self {
        import::TEST_BATCH_LIMIT.store(bytes, std::sync::atomic::Ordering::SeqCst);
        SmallBatch
    }
}

impl Drop for SmallBatch {
    fn drop(&mut self) {
        import::TEST_BATCH_LIMIT.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

#[tokio::test]
async fn one_call_never_writes_more_than_its_batch_budget() {
    let _fixture = claude_pair_fixture();
    let _limit = SmallBatch::of(1);
    let (st, mut events) = state();
    let result = import::rpc(&st, "import.apply", &json!({"items":[
        {"source":"claude","conversationId":"sess-a"},
        {"source":"claude","conversationId":"sess-b"}
    ]})).await.unwrap();
    assert_eq!(result["imported"], 0, "{result}");
    assert_eq!(result["skipped"], 2);
    assert!(st.db.import_sessions(&[]).unwrap().is_empty());
    let mut reasons: Vec<String> = Vec::new();
    while let Ok(event) = events.try_recv() {
        if event.event_type == "import.progress" {
            if let Some(reason) = event.payload["reason"].as_str() { reasons.push(reason.to_owned()); }
        }
    }
    assert_eq!(reasons.iter().filter(|reason| reason.as_str() == "batch_budget").count(), 2, "{reasons:?}");
}

#[tokio::test]
async fn an_import_is_read_only_until_the_same_agent_still_has_it() {
    let fixture = claude_fixture();
    let (st, _events) = state();
    let result = apply_one(&st, "claude", "sess-1").await;
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    // An import nobody owns is read-only: it is not a conversation yet.
    let unowned = st.db.session_event_summary(&session_id).unwrap();
    assert_eq!(unowned["imported"]["readOnly"], true);
    assert_eq!(unowned["imported"]["canResumeWith"], Value::Null);
    // The bot runs on Claude Code, so that coding agent has to exist first.
    add_runtime(&st, "rt-claude", "claude").await;
    let bot = st.db.agent_create(&json!({"name":"Claude bot","runtimeId":"rt-claude"})).unwrap().0;
    let bot_id = bot["id"].as_str().unwrap().to_owned();
    st.db.import_assign(&session_id, Some(Some(bot_id.as_str())), Some("rt-claude"), None, false).unwrap();
    let session = st.db.session_brief(&session_id).unwrap();
    // The bot speaks the same agent, and the file is still there: it may continue.
    assert!(import::refuse_read_only(&st, &session).await.is_ok());
    assert_eq!(session["imported"]["readOnly"], false);
    // No bot at all, then a file that has gone: refused both times.
    assert_eq!(import::refuse_read_only(&st, &json!({"id":session_id,"agentId":Value::Null,"providerSessionId":"sess-1"})).await.unwrap_err().1.code, "imported_read_only");
    std::fs::remove_dir_all(fixture.dir.join(".claude")).unwrap();
    assert_eq!(import::refuse_read_only(&st, &st.db.session_brief(&session_id).unwrap()).await.unwrap_err().1.code, "imported_read_only");
    // A conversation Retinue owns is never affected, and carries no imported field.
    let (own, _events) = state();
    own.db.create_session("own", "rt-test", "codex", ".", "Mine").unwrap();
    let brief = own.db.session_brief("own").unwrap();
    assert!(brief.get("imported").is_none());
    assert!(import::refuse_read_only(&own, &json!({"id":"own"})).await.is_ok());
    // A summary says the same thing the guard decides with.
    let summary = st.db.session_event_summary(&session_id).unwrap();
    assert_eq!(summary["imported"]["source"], "claude");
    assert_eq!(summary["imported"]["conversationId"], "sess-1");
}

#[tokio::test]
async fn session_prompt_and_the_queue_refuse_a_read_only_import() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    let result = apply_one(&st, "claude", "sess-1").await;
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    let error = crate::dispatch(&st, "session.prompt", json!({"sessionId":session_id,"text":"carry on"})).await.unwrap_err();
    assert_eq!(error.1.code, "imported_read_only", "{error:?}");
    assert_eq!(error.0, StatusCode::FORBIDDEN);
    let queued = crate::dispatch(&st, "session.queue.add", json!({"sessionId":session_id,"text":"later"})).await.unwrap_err();
    assert_eq!(queued.1.code, "imported_read_only");
}

#[tokio::test]
async fn distribute_files_each_conversation_under_a_bot_with_its_reason() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    let bot = st.db.agent_create(&json!({"name":"Billing","runtimeId":"rt-test","role":"billing invoices"})).unwrap().0;
    let bot_id = bot["id"].as_str().unwrap().to_owned();
    let result = apply_one(&st, "claude", "sess-1").await;
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    let suggestions = import::rpc(&st, "import.distribute", &json!({"ids":[session_id]})).await.unwrap();
    let suggestion = &suggestions["suggestions"][0];
    assert_eq!(suggestion["conversationId"], session_id);
    assert_eq!(suggestion["sessionId"], session_id);
    assert_eq!(suggestion["agentId"], bot_id);
    // The role match wins once there is no project and no same-agent bot.
    assert_eq!(suggestion["reason"], "role");
    assert!(suggestion["detail"].as_str().unwrap().contains("invoice"));
    let assigned = import::rpc(&st, "import.assign", &json!({"sessionId":session_id,"agentId":bot_id,"projectId":Value::Null})).await.unwrap();
    assert_eq!(assigned["session"]["agentId"], bot_id);
    // A conversation Retinue owns cannot be filed this way.
    st.db.create_session("own", "rt-test", "codex", ".", "Mine").unwrap();
    assert_eq!(import::rpc(&st, "import.assign", &json!({"sessionId":"own","agentId":bot_id})).await.unwrap_err().1.code, "invalid_argument");
}

#[tokio::test]
async fn assign_finds_or_makes_the_project_for_a_folder() {
    let fixture = claude_pair_fixture();
    let (st, _events) = state();
    let result = apply_one(&st, "claude", "sess-a").await;
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    let folder = fixture.dir.join("workspace").join("billing");
    let assigned = import::rpc(&st, "import.assign", &json!({"sessionId":session_id,"projectPath":folder.to_string_lossy()})).await.unwrap();
    assert_eq!(assigned["session"]["projectId"], st.db.project_list().unwrap()[0]["id"]);
    // A second assignment to the same folder reuses the project.
    let other = apply_one(&st, "claude", "sess-b").await;
    let other_id = other["sessions"][0].as_str().unwrap().to_owned();
    import::rpc(&st, "import.assign", &json!({"sessionId":other_id,"projectPath":folder.to_string_lossy()})).await.unwrap();
    assert_eq!(st.db.project_list().unwrap().len(), 1);
}

#[tokio::test]
async fn apply_refuses_a_request_that_names_no_source_or_too_many_conversations() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    assert_eq!(import::rpc(&st, "import.apply", &json!({"items":[]})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(import::rpc(&st, "import.apply", &json!({"items":[{"source":"cursor","conversationId":"x"}]})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(import::rpc(&st, "import.preview", &json!({"source":"cursor"})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(import::rpc(&st, "import.projects", &json!({"sources":["cursor"]})).await.unwrap_err().1.code, "invalid_argument");
    let many = (0..import::MAX_CONVERSATIONS + 1).map(|index| json!({"source":"claude","conversationId":format!("s{index}")})).collect::<Vec<_>>();
    assert_eq!(import::rpc(&st, "import.apply", &json!({"items":many})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(import::rpc(&st, "import.continue", &json!({"sessionId":"none","agentId":"nobody"})).await.unwrap_err().1.code, "invalid_argument");
}

#[test]
fn import_stays_on_this_machine_in_the_remote_policy() {
    for method in ["import.scan", "import.preview", "import.projects", "import.apply", "import.distribute", "import.assign", "import.continue"] {
        let rule = crate::remote::policy::rule_for(method).unwrap_or_else(|| panic!("{method} has no policy row"));
        assert_eq!(rule.class, crate::remote::policy::MethodClass::Never, "{method} must stay never");
    }
    for event in ["import.progress", "import.completed"] {
        let envelope = json!({"type": event, "payload": {}});
        assert!(crate::remote::policy::redact_event_for(&envelope, "phone").is_none(), "{event} must be dropped");
    }
}

#[test]
fn a_project_key_is_the_same_folder_written_differently() {
    let dir = std::env::temp_dir().join(format!("retinue-import-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = dir.to_string_lossy().to_string();
    let forward = text.replace('\\', "/");
    let dotted = format!("{}/./", forward);
    assert_eq!(import::path_key_for_test(&text), import::path_key_for_test(&forward));
    assert_eq!(import::path_key_for_test(&forward), import::path_key_for_test(&dotted));
    assert!(import::path_key_for_test(&forward).contains("retinue-import-key-"));
    let _ = std::fs::remove_dir_all(dir);
}
