use super::*;

async fn fixture() -> (AppState, PathBuf) {
    let (st, _) = crate::phase2a_tests::state();
    let root = std::env::temp_dir().join(format!("retinue-external-clients-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(root.clone());
    *st.team.address.lock().unwrap() = Some("127.0.0.1:1".into());
    let app = root.join("cursor-app");
    fs::write(&app, "").unwrap();
    *st.external.client_fixture.lock().unwrap() = Some(Environment {
        home: Some(root.clone()),
        cursor_app: Some(app),
        ..Environment::default()
    });
    super::super::rpc(&st, "external.enable", &json!({"confirm": true})).await.unwrap();
    (st, root)
}

async fn call(st: &AppState, method: &str, p: Value) -> Result<Value, DispatchError> {
    crate::dispatch(st, method, p).await
}

async fn cursor(st: &AppState, connect: bool) -> Value {
    let method = if connect { "external.clients.connect" } else { "external.clients.disconnect" };
    call(st, method, json!({"id":"cursor", "confirm":true})).await.unwrap()["client"].clone()
}

#[tokio::test]
async fn external_clients_list_and_refusals() {
    let (st, root) = fixture().await;
    st.external.client_fixture.lock().unwrap().as_mut().unwrap().cursor_app = None;
    let listed = call(&st, "external.clients.list", json!({})).await.unwrap();
    let rows = listed["clients"].as_array().unwrap();
    assert_eq!(rows.len(), CLIENTS.len());
    assert!(rows.iter().all(|row| row["installed"] == false && row["state"] == "not_connected"));
    let connect = "external.clients.connect";
    assert_eq!(call(&st, connect, json!({"id":"cursor"})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(call(&st, connect, json!({"id":"unknown","confirm":true})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(call(&st, connect, json!({"id":"cursor","confirm":true})).await.unwrap_err().1.code, "conflict");
    call(&st, "external.disable", json!({})).await.unwrap();
    assert_eq!(call(&st, connect, json!({"id":"cursor","confirm":true})).await.unwrap_err().1.message, "Turn the Retinue MCP on first.");
    assert!(call(&st, "external.clients.list", json!({})).await.is_ok());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn external_clients_cursor_edits_only_its_entry() {
    let (st, root) = fixture().await;
    let path = root.join(".cursor/mcp.json");
    let backup = path.with_file_name("mcp.json.retinue-backup");
    let original = b"{\r\n \"other\": [1,2],\r\n \"mcpServers\": { \"existing\": {\"command\":\"elsewhere\"} }\r\n}\r\n";
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, original).unwrap();
    let connected = cursor(&st, true).await;
    assert_eq!(connected["state"], "connected");
    assert_eq!(connected["addedByRetinue"], true);
    let (bytes, doc) = read_json(&path).unwrap();
    assert_eq!(doc["mcpServers"]["retinue"], Client::Cursor.expected(&registration(&st).unwrap()));
    assert_eq!(doc["mcpServers"]["existing"]["command"], "elsewhere");
    let token = st.external.current_token().unwrap();
    assert!(!String::from_utf8(bytes.unwrap()).unwrap().contains(&token));
    assert_eq!(fs::read(&backup).unwrap(), original);
    call(&st, "external.disable", json!({})).await.unwrap();
    assert_eq!(cursor(&st, false).await["state"], "not_connected");
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn external_clients_refuse_malformed_and_leave_foreign_entries() {
    let (st, root) = fixture().await;
    let path = root.join(".cursor/mcp.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let malformed = b"{ \"mcpServers\": {";
    fs::write(&path, malformed).unwrap();
    assert_eq!(cursor(&st, true).await["error"], "Cursor's mcp.json could not be read.");
    assert_eq!(fs::read(&path).unwrap(), malformed);
    assert!(!path.with_file_name("mcp.json.retinue-backup").exists());
    fs::write(&path, "{}").unwrap();
    assert_eq!(cursor(&st, true).await["addedByRetinue"], true);
    let different = br#"{"mcpServers":{"retinue":{"command":"different","args":[]}}}"#;
    fs::write(&path, different).unwrap();
    let row = cursor(&st, false).await;
    assert_eq!(row["state"], "different");
    assert_eq!(row["addedByRetinue"], false);
    assert_eq!(fs::read(&path).unwrap(), different);
    assert_eq!(cursor(&st, true).await["state"], "different");
    assert_eq!(fs::read(&path).unwrap(), different);
    fs::remove_dir_all(root).unwrap();
}

fn fake_claude(client: Client, path: &Path, args: &[String]) -> Result<Vec<u8>, ()> {
    assert!(client == Client::Claude);
    let root = path.parent().unwrap();
    let (_, mut doc) = read_json(path).unwrap();
    match args[1].as_str() {
        "add" => {
            assert_eq!(&args[..6], ["mcp", "add", "--scope", "user", "retinue", "--"]);
            assert_eq!(args[6], root.join(BRIDGE_FILE).to_str().unwrap());
            assert_eq!(&args[7..], ["mcp-server", "--endpoint", root.join(ENDPOINT_FILE).to_str().unwrap()]);
            doc["mcpServers"]["retinue"] = json!({"type":"stdio","command":args[6],"args":args[7..]});
        }
        "remove" => {
            assert_eq!(args, ["mcp", "remove", "--scope", "user", "retinue"]);
            doc["mcpServers"].as_object_mut().unwrap().remove("retinue").unwrap();
        }
        other => panic!("Unexpected CLI command {other}"),
    }
    fs::write(path, doc.to_string()).unwrap();
    Ok(Vec::new())
}

#[tokio::test]
async fn external_clients_disable_removes_owned_entries_through_cli_and_file() {
    let (st, root) = fixture().await;
    st.external.client_fixture.lock().unwrap().as_mut().unwrap().runner = Some(fake_claude);
    let p = json!({"id":"claude-code", "confirm":true});
    assert_eq!(call(&st, "external.clients.connect", p).await.unwrap()["client"]["addedByRetinue"], true);
    assert_eq!(cursor(&st, true).await["addedByRetinue"], true);
    let disabled = call(&st, "external.disable", json!({"removeClients":true})).await.unwrap();
    assert_eq!(disabled["enabled"], false);
    assert!(!root.join(ENDPOINT_FILE).exists());
    assert!(ownership(&st).unwrap().as_object().unwrap().is_empty());
    let listed = call(&st, "external.clients.list", json!({})).await.unwrap();
    assert!(listed["clients"].as_array().unwrap().iter().all(|row| row["state"] == "not_connected"));
    fs::remove_dir_all(root).unwrap();
}

fn fake_opencode(client: Client, path: &Path, args: &[String]) -> Result<Vec<u8>, ()> {
    assert!(client == Client::OpenCode);
    assert_eq!(&args[..5], ["mcp", "add", "--global", "retinue", "--"]);
    assert_eq!(&args[6..8], ["mcp-server", "--endpoint"]);
    let (_, mut doc) = read_json(path).unwrap();
    doc["mcp"]["servers"]["retinue"] = json!({"type":"local","command":args[5..]});
    fs::write(path, doc.to_string()).unwrap();
    Ok(Vec::new())
}

#[tokio::test]
async fn external_clients_opencode_uses_global_cli_add_and_guarded_removal() {
    let (st, root) = fixture().await;
    {
        let mut environment = st.external.client_fixture.lock().unwrap();
        let env = environment.as_mut().unwrap();
        env.opencode_dir = Some(root.clone());
        env.runner = Some(fake_opencode);
    }
    let path = root.join("opencode.json");
    let original = json!({"model":"unchanged","mcp":{"servers":{"other":{"type":"local","command":["other"]}}}});
    fs::write(&path, original.to_string()).unwrap();
    let p = json!({"id":"opencode", "confirm":true});
    let connected = call(&st, "external.clients.connect", p.clone()).await.unwrap();
    assert_eq!(connected["client"]["addedByRetinue"], true);
    let doc = read_json(&path).unwrap().1;
    assert_eq!(doc["mcp"]["servers"]["retinue"], Client::OpenCode.expected(&registration(&st).unwrap()));
    assert!(!doc.to_string().contains(&st.external.current_token().unwrap()));
    let disconnected = call(&st, "external.clients.disconnect", p).await.unwrap();
    assert_eq!(disconnected["client"]["state"], "not_connected");
    assert_eq!(read_json(&path).unwrap().1, original);
    assert!(path.with_file_name("opencode.json.retinue-backup").is_file());
    fs::remove_dir_all(root).unwrap();
}
