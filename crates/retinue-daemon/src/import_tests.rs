//! Tests for `import.rs`: one small fixture per source format, and the promise
//! that credentials, thinking, instruction blocks and file snapshots are never
//! stored. The fixtures are synthetic; no real conversation is in this repository.

#![allow(clippy::await_holding_lock)]

use crate::import::{self, Roots, TEST_ROOTS};
use crate::phase2a_tests::state;
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
    drop(connection);
    set_roots(Roots { claude_projects: None, codex_sessions: None, opencode_db: Some(db) });
    Fixture { dir, _guard: guard }
}

fn stored(st: &crate::AppState, session_id: &str) -> Value {
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
async fn scan_and_preview_report_what_is_on_disk() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    let scan = import::rpc(&st, "import.scan", &json!({})).await.unwrap();
    let sources = scan["sources"].as_array().unwrap();
    // Codex and OpenCode are not in this fixture, so they are not offered at all.
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["source"], "claude");
    assert_eq!(sources[0]["conversations"], 1);
    assert!(sources[0]["bytes"].as_u64().unwrap() > 0);
    let preview = import::rpc(&st, "import.preview", &json!({"source":"claude"})).await.unwrap();
    assert_eq!(preview["total"], 1);
    let row = &preview["rows"][0];
    assert_eq!(row["title"], "Invoice parser tests");
    assert_eq!(row["firstUser"], "Why is the invoice parser skipping line 2?");
    assert_eq!(row["canResumeWith"], "claude");
    assert_eq!(row["alreadyImported"], false);
}

#[tokio::test]
async fn an_import_is_read_only_until_the_same_agent_still_has_it() {
    let fixture = claude_fixture();
    let (st, _events) = state();
    let result = apply_one(&st, "claude", "sess-1").await;
    let session_id = result["sessions"][0].as_str().unwrap().to_owned();
    // The bot runs on Claude Code, so that coding agent has to exist first.
    st.db.upsert_runtime(&json!({"id":"rt-claude","provider":"claude","status":"online"})).unwrap();
    let bot = st.db.agent_create(&json!({"name":"Claude bot","runtimeId":"rt-claude"})).unwrap().0;
    let bot_id = bot["id"].as_str().unwrap().to_owned();
    *st.runtimes.write().await = vec![json!({"id":"rt-claude","provider":"claude","status":"online"})];
    st.db.import_assign(&session_id, Some(Some(bot_id.as_str())), Some("rt-claude"), None, false).unwrap();
    let session = st.db.session_brief(&session_id).unwrap();
    // The bot speaks the same agent, and the file is still there: it may continue.
    assert!(import::refuse_read_only(&st, &session).await.is_ok());
    // No bot at all, then a file that has gone: refused both times.
    assert_eq!(import::refuse_read_only(&st, &json!({"id":session_id,"agentId":Value::Null,"providerSessionId":"sess-1"})).await.unwrap_err().1.code, "imported_read_only");
    std::fs::remove_dir_all(fixture.dir.join(".claude")).unwrap();
    assert_eq!(import::refuse_read_only(&st, &st.db.session_brief(&session_id).unwrap()).await.unwrap_err().1.code, "imported_read_only");
    // A conversation Retinue owns is never affected.
    let (own, _events) = state();
    own.db.create_session("own", "rt-test", "codex", ".", "Mine").unwrap();
    assert!(import::refuse_read_only(&own, &json!({"id":"own"})).await.is_ok());
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
async fn apply_refuses_a_request_that_names_no_source_or_too_many_conversations() {
    let _fixture = claude_fixture();
    let (st, _events) = state();
    assert_eq!(import::rpc(&st, "import.apply", &json!({"items":[]})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(import::rpc(&st, "import.apply", &json!({"items":[{"source":"cursor","conversationId":"x"}]})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(import::rpc(&st, "import.preview", &json!({"source":"cursor"})).await.unwrap_err().1.code, "invalid_argument");
    let many = (0..import::MAX_CONVERSATIONS + 1).map(|index| json!({"source":"claude","conversationId":format!("s{index}")})).collect::<Vec<_>>();
    assert_eq!(import::rpc(&st, "import.apply", &json!({"items":many})).await.unwrap_err().1.code, "invalid_argument");
}
