// Section D: connectors. Included into `phase2a_tests`, so it shares that
// module's fixtures (`state`, `dispatch`, `json!`, `Value`, `Uuid`, `Arc`,
// `AppState`, `team`). Each test starts its own fake MCP server on 127.0.0.1
// (plain HTTP is allowed for a server on this PC) and its own rows, so tests
// running in one process never meet.

use std::sync::Mutex as ConnMutex;
use std::time::Duration as ConnDuration;

const CONN_TOKEN: &str = "tok_live_9f2c1b7a4d6e8";
const CONN_REPLY: &str = "The issue ENG-42 is open.";

/// One request as the fake server saw it: the Authorization header, if any.
type ConnSeen = Arc<ConnMutex<Vec<String>>>;

fn conn_position(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

async fn conn_read_request(stream: &mut tokio::net::TcpStream) -> Option<(String, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..read]);
        if let Some(at) = conn_position(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..at]).to_string();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if name.eq_ignore_ascii_case("content-length") {
                        value.trim().parse::<usize>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            let mut body = buf[at + 4..].to_vec();
            while body.len() < length {
                let read = stream.read(&mut chunk).await.ok()?;
                if read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read]);
            }
            body.truncate(length);
            return Some((head, body));
        }
    }
}

fn conn_header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// A minimal Streamable HTTP MCP server: initialize, tools/list (one page) and
/// tools/call. `reply` is the text `tools/call` returns. Each answer is JSON,
/// unless `sse` is set, in which case it is one server-sent event, as several
/// real servers send every POST answer.
async fn conn_fake_server_with(tools: Value, reply: &'static str, sse: bool) -> (String, ConnSeen) {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen: ConnSeen = Arc::new(ConnMutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { break };
            let log = log.clone();
            let tools = tools.clone();
            tokio::spawn(async move {
                let Some((head, body)) = conn_read_request(&mut stream).await else { return };
                log.lock().unwrap().push(conn_header(&head, "authorization").unwrap_or("").to_owned());
                let message: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let id = message["id"].clone();
                let method = message["method"].as_str().unwrap_or("");
                let (status, payload) = match method {
                    "initialize" => (200, json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1"}}})),
                    "tools/list" => (200, json!({"jsonrpc":"2.0","id":id,"result":{"tools":tools.clone()}})),
                    "tools/call" => (200, json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":reply}],"isError":false}})),
                    _ if id.is_null() => (202, Value::Null),
                    _ => (200, json!({"jsonrpc":"2.0","id":id,"result":{}})),
                };
                let (content_type, text) = if payload.is_null() {
                    ("application/json", String::new())
                } else if sse {
                    ("text/event-stream", format!("event: message\ndata: {payload}\n\n"))
                } else {
                    ("application/json", payload.to_string())
                };
                let response = format!(
                    "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    if status == 202 { "Accepted" } else { "OK" },
                    text.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (format!("http://{address}/mcp"), seen)
}

async fn conn_fake_server(tools: Value, reply: &'static str) -> (String, ConnSeen) {
    conn_fake_server_with(tools, reply, false).await
}

fn conn_tools() -> Value {
    json!([
        {"name":"search_issues","description":"Search issues in a workspace.","annotations":{"title":"Search issues","readOnlyHint":true},"inputSchema":{"type":"object","properties":{"query":{"type":"string","pattern":"^[a-z]+$"}}}},
        {"name":"delete_issue","description":"Delete an issue for good.","annotations":{"readOnlyHint":false},"inputSchema":{"type":"object","properties":{"id":{"type":"string"}}}}
    ])
}

/// An agent and its own conversation, with the link a test wants.
fn conn_session(st: &AppState, name: &str, link: Value) -> String {
    let agent = st.db.agent_create(&json!({"name": name, "runtimeId": "rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    let session = format!("conn-{}", Uuid::new_v4().simple());
    st.db.create_session_for_agent(&session, "rt-test", "codex", ".", "Connector test", Some(&agent)).unwrap();
    st.db.set_session_link(&session, &link).unwrap();
    session
}

async fn conn_call(st: &AppState, session: &str, tool: &str, arguments: Value) -> Result<Value, DispatchError> {
    team::mcp_rpc(st, session, "mcp.tools.call", &json!({"name": "connector_call", "arguments": {"connector": "Fake", "tool": tool, "arguments": arguments}})).await
}

async fn conn_wait_card(st: &AppState, session: &str) -> Value {
    for _ in 0..300 {
        if let Some(row) = st.db.permissions_pending(session).unwrap().into_iter().next() {
            return row;
        }
        tokio::time::sleep(ConnDuration::from_millis(10)).await;
    }
    panic!("no connector card appeared for {session}");
}

fn conn_text(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap_or("").to_owned()
}

#[test]
fn connector_address_is_https_or_loopback_without_a_sign_in() {
    assert!(connectors::parse_url("https://mcp.example.com/mcp").is_ok());
    assert!(connectors::parse_url("http://127.0.0.1:8765/mcp").is_ok());
    assert!(connectors::parse_url("http://localhost:8765/mcp").is_ok());
    let plain = connectors::parse_url("http://mcp.example.com/mcp").unwrap_err();
    assert!(plain.contains("https://"), "{plain}");
    let sign_in = connectors::parse_url("https://user:pass@mcp.example.com/mcp").unwrap_err();
    assert!(sign_in.contains("sign-in"), "{sign_in}");
    let fragment = connectors::parse_url("https://mcp.example.com/mcp#x").unwrap_err();
    assert!(fragment.contains("fragment"), "{fragment}");
    assert!(connectors::parse_url("ftp://mcp.example.com/mcp").is_err());
}

#[tokio::test]
async fn connector_check_lists_tools_and_saves_schemas_without_the_token() {
    let (st, _) = state();
    let (url, seen) = conn_fake_server(conn_tools(), CONN_REPLY).await;
    let added = dispatch(&st, "connector.add", json!({"name": "Fake", "url": url, "token": CONN_TOKEN})).await.unwrap();
    let connector = &added["connector"];
    assert_eq!(connector["status"], "ok");
    assert_eq!(connector["toolCount"], 2);
    assert_eq!(connector["auth"], "token");
    assert_eq!(connector["unattended"], "off");
    assert_eq!(connector["enabled"], true);
    let id = connector["id"].as_str().unwrap().to_owned();
    // The token went to the credential store and the Authorization header, and
    // nowhere a caller can read it back.
    assert_eq!(connectors::credential(&id).unwrap().as_deref(), Some(CONN_TOKEN));
    assert!(seen.lock().unwrap().iter().any(|auth| auth == &format!("Bearer {CONN_TOKEN}")));
    let listed = dispatch(&st, "connector.list", json!({})).await.unwrap();
    let text = listed.to_string();
    assert!(!text.contains(CONN_TOKEN), "the token is never in a reply");
    assert!(listed["catalogue"].as_array().unwrap().len() >= 5);
    // Schemas were stored without regex keys, and read back with their rule.
    let tools = dispatch(&st, "connector.tools", json!({"id": id})).await.unwrap()["tools"].as_array().unwrap().clone();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["readOnly"], true);
    assert_eq!(tools[0]["title"], "Search issues");
    assert_eq!(tools[0]["rule"], "ask");
    assert!(tools[0]["inputSchema"]["properties"]["query"].get("pattern").is_none(), "the schema pattern is stripped");
    // One line per enabled connector reaches the bot's instructions.
    let instructions = connectors::instructions(&st).unwrap();
    assert!(instructions.contains("- Fake: 2 tools"), "{instructions}");
}

#[tokio::test]
async fn connector_call_asks_the_owner_then_runs_and_masks_a_returned_token() {
    let (st, _) = state();
    let (url, seen) = conn_fake_server(conn_tools(), "Your token is totok_live_9f2c1b7a4d6e8ken please keep it").await;
    let id = dispatch(&st, "connector.add", json!({"name": "Fake", "url": url, "token": CONN_TOKEN})).await.unwrap()["connector"]["id"].as_str().unwrap().to_owned();
    let session = conn_session(&st, "Connector caller", json!({"kind": "main"}));
    let caller = st.clone();
    let session_call = session.clone();
    let call = tokio::spawn(async move {
        team::mcp_rpc(&caller, &session_call, "mcp.tools.call", &json!({"name": "connector_call", "arguments": {"connector": id, "tool": "search_issues", "arguments": {"query": "bug"}}})).await
    });
    let card = conn_wait_card(&st, &session).await;
    assert_eq!(card["title"], "Fake: search_issues");
    assert_eq!(card["choices"], json!(["allow_once", "allow", "deny"]));
    let replied = dispatch(&st, "permission.reply", json!({"permissionId": card["id"], "choice": "allow_once"})).await.unwrap();
    assert_eq!(replied["allowed"], true);
    let result = call.await.unwrap().unwrap();
    assert_eq!(conn_text(&result), "Your token is to[hidden]ken please keep it");
    assert!(!conn_text(&result).contains(CONN_TOKEN));
    assert!(seen.lock().unwrap().iter().filter(|auth| !auth.is_empty()).all(|auth| auth == &format!("Bearer {CONN_TOKEN}")));
}

#[tokio::test]
async fn connector_denied_tool_is_refused_and_allowed_tool_skips_the_card() {
    let (st, _) = state();
    let (url, _) = conn_fake_server(conn_tools(), CONN_REPLY).await;
    let id = dispatch(&st, "connector.add", json!({"name": "Fake", "url": url, "token": CONN_TOKEN})).await.unwrap()["connector"]["id"].as_str().unwrap().to_owned();
    let session = conn_session(&st, "Connector rules", json!({"kind": "main"}));
    dispatch(&st, "connector.tool.rule", json!({"id": id, "tool": "search_issues", "rule": "deny"})).await.unwrap();
    let refused = conn_call(&st, &session, "search_issues", json!({"query": "bug"})).await.unwrap();
    assert_eq!(refused["isError"], true);
    assert!(conn_text(&refused).contains("denied"), "{}", conn_text(&refused));
    assert!(st.db.permissions_pending(&session).unwrap().is_empty(), "a denied tool never shows a card");
    dispatch(&st, "connector.tool.rule", json!({"id": id, "tool": "search_issues", "rule": "allow"})).await.unwrap();
    let allowed = conn_call(&st, &session, "search_issues", json!({"query": "bug"})).await.unwrap();
    assert_eq!(conn_text(&allowed), CONN_REPLY);
    assert!(st.db.permissions_pending(&session).unwrap().is_empty(), "an allowed tool asks nothing");
    let tools = dispatch(&st, "connector.tools", json!({"id": id})).await.unwrap()["tools"].as_array().unwrap().clone();
    assert_eq!(tools[0]["rule"], "allow");
}

#[tokio::test]
async fn connector_unattended_run_calls_only_read_only_tools() {
    let (st, _) = state();
    let (url, _) = conn_fake_server(conn_tools(), CONN_REPLY).await;
    let id = dispatch(&st, "connector.add", json!({"name": "Fake", "url": url, "token": CONN_TOKEN})).await.unwrap()["connector"]["id"].as_str().unwrap().to_owned();
    let session = conn_session(&st, "Connector unattended", json!({"kind": "task", "unattended": true}));
    // The connector's switch is still off: nothing may run.
    let off = conn_call(&st, &session, "search_issues", json!({"query": "bug"})).await.unwrap();
    assert_eq!(off["isError"], true);
    assert!(conn_text(&off).contains("unattended"), "{}", conn_text(&off));
    dispatch(&st, "connector.update", json!({"id": id, "unattended": "readOnly"})).await.unwrap();
    // A read-only tool runs, with no card: nobody is there to answer one.
    let read = conn_call(&st, &session, "search_issues", json!({"query": "bug"})).await.unwrap();
    assert_eq!(conn_text(&read), CONN_REPLY);
    assert!(st.db.permissions_pending(&session).unwrap().is_empty());
    // A write tool is refused even with an allow rule.
    dispatch(&st, "connector.tool.rule", json!({"id": id, "tool": "delete_issue", "rule": "allow"})).await.unwrap();
    let write = conn_call(&st, &session, "delete_issue", json!({"id": "ENG-42"})).await.unwrap();
    assert_eq!(write["isError"], true);
    assert!(conn_text(&write).contains("read-only"), "{}", conn_text(&write));
}

#[tokio::test]
async fn connector_answers_over_a_server_sent_event_stream() {
    let (st, _) = state();
    let (url, _) = conn_fake_server_with(conn_tools(), CONN_REPLY, true).await;
    let id = dispatch(&st, "connector.add", json!({"name": "Streaming", "url": url})).await.unwrap()["connector"]["id"].as_str().unwrap().to_owned();
    let connector = dispatch(&st, "connector.list", json!({})).await.unwrap()["connectors"].as_array().unwrap().clone();
    assert_eq!(connector[0]["status"], "ok");
    assert_eq!(connector[0]["toolCount"], 2);
    dispatch(&st, "connector.tool.rule", json!({"id": id, "tool": "search_issues", "rule": "allow"})).await.unwrap();
    let session = conn_session(&st, "Connector stream", json!({"kind": "main"}));
    let call = team::mcp_rpc(&st, &session, "mcp.tools.call", &json!({"name": "connector_call", "arguments": {"connector": "Streaming", "tool": "search_issues", "arguments": {"query": "bug"}}})).await.unwrap();
    assert_eq!(conn_text(&call), CONN_REPLY);
}

#[tokio::test]
async fn connector_missing_token_is_refused_and_remove_deletes_the_row() {
    let (st, _) = state();
    let (url, _) = conn_fake_server(conn_tools(), CONN_REPLY).await;
    let added = dispatch(&st, "connector.add", json!({"name": "Fake", "url": url, "token": CONN_TOKEN})).await.unwrap();
    let id = added["connector"]["id"].as_str().unwrap().to_owned();
    let session = conn_session(&st, "Connector missing token", json!({"kind": "main"}));
    dispatch(&st, "connector.tool.rule", json!({"id": id, "tool": "search_issues", "rule": "allow"})).await.unwrap();
    // The token is gone from the store: a call says so instead of reaching the server.
    let _ = secrets::store::delete(&format!("connector/{id}"));
    let call = conn_call(&st, &session, "search_issues", json!({"query": "bug"})).await.unwrap();
    assert_eq!(call["isError"], true);
    assert!(conn_text(&call).contains("token"), "{}", conn_text(&call));
    let removed = dispatch(&st, "connector.remove", json!({"id": id, "confirm": true})).await.unwrap();
    assert_eq!(removed["removed"], true);
    assert!(st.db.connector_get(&id).unwrap().is_none());
    assert!(st.db.connector_rules(&id).unwrap().is_empty());
    assert!(connectors::credential(&id).unwrap().is_none());
    assert!(dispatch(&st, "connector.remove", json!({"id": id, "confirm": true})).await.is_ok());
    let bad = dispatch(&st, "connector.remove", json!({"id": id})).await.unwrap_err();
    assert_eq!(bad.1.code, "invalid_argument");
}
