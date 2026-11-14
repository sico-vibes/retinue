// The browser on the linked VPS computer. Included into `phase2a_tests` after
// `computer_tests.rs`, so it shares `state`, `json!`, `Value`, `Arc`, `Uuid`,
// `fake_harbor`, `harbor_body`, `link_directly` and `HARBOR_TOKEN`. A refusal
// that never reached the container is proved by the fake recording no action.

/// A 1x1 PNG: the smallest picture the media layer accepts.
const PNG_1X1: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

/// The JSON request the daemon encoded into a `computer_exec` command.
fn decode_action(command: &str) -> Option<Value> {
    let start = command.find("printf '%s' '")? + "printf '%s' '".len();
    let end = command[start..].find("' | base64 -d")? + start;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&command[start..end]).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// A fake browser server inside a fake container. `readiness` is what the
/// readiness probe prints (`BROWSER_RUNNING` or `MISSING_PLAYWRIGHT`), and
/// `script` answers one decoded action. Every command the daemon ran inside the
/// container is recorded, so a test can prove what did and did not reach it.
async fn fake_browser_harbor(
    readiness: &'static str,
    script: impl Fn(&Value) -> Value + Send + Sync + 'static,
) -> (String, Arc<TestMutex<Vec<String>>>) {
    let ran: Arc<TestMutex<Vec<String>>> = Arc::new(TestMutex::new(Vec::new()));
    let recorder = ran.clone();
    let script = Arc::new(script);
    let missing = readiness == "MISSING_PLAYWRIGHT";
    let address = fake_harbor(move |_, path, body| {
        if !path.ends_with("/exec") {
            return (200, harbor_body("running", None));
        }
        let command = body["command"].as_str().unwrap_or("").to_owned();
        // The bootstrap and the probe name their own markers; an action carries
        // its request as base64.
        if command.contains("BROWSER_READY") {
            recorder.lock().unwrap().push(command);
            if missing {
                // No Playwright in the container: Node never starts.
                return (200, json!({"ok": false, "code": 1, "output": "Cannot find package 'playwright'"}));
            }
            return (200, json!({"ok": true, "code": 0, "output": "BROWSER_READY"}));
        }
        if command.contains("MISSING_PLAYWRIGHT") {
            recorder.lock().unwrap().push(command);
            return (200, json!({"ok": true, "code": 0, "output": readiness}));
        }
        if missing {
            recorder.lock().unwrap().push(command);
            return (200, json!({"ok": false, "code": 7, "output": "curl: (7) Failed to connect"}));
        }
        let request = decode_action(&command);
        recorder.lock().unwrap().push(command);
        let reply = match request {
            Some(request) => script(&request),
            None => json!({"ok": false, "error": "no request"}),
        };
        (200, json!({"ok": true, "code": 0, "output": reply.to_string()}))
    })
    .await;
    (address, ran)
}

fn browser_session(st: &AppState, id: &str) {
    st.db.create_session(id, "rt-test", "codex", ".", "Chat").unwrap();
    st.db.update_session_state(id, "working").unwrap();
}

/// One `mcp.browser.call`, as the bridge makes it.
async fn browser_call(st: &AppState, session: &str, name: &str, args: Value) -> Value {
    browser::mcp_rpc(st, session, "mcp.browser.call", &json!({"name": name, "arguments": args})).await.unwrap()
}

/// The tool's text, parsed back into JSON.
fn reply_of(reply: &Value) -> Value {
    let text = reply["content"][0]["text"].as_str().unwrap_or("");
    serde_json::from_str(text).unwrap_or_else(|_| panic!("the tool reply is JSON: {text}"))
}

fn whole_page(url: &str, nodes: Value) -> Value {
    json!({"ok": true, "url": url, "title": "A page", "nodes": nodes, "tree": "- heading \"A page\""})
}

#[tokio::test]
async fn browser_changed_is_announced_only_when_the_readiness_flips() {
    let mode = Arc::new(TestMutex::new("MISSING_PLAYWRIGHT".to_owned()));
    let shared = mode.clone();
    let address = fake_harbor(move |_, path, body| {
        if !path.ends_with("/exec") {
            return (200, harbor_body("running", None));
        }
        let command = body["command"].as_str().unwrap_or("");
        // The probe names its own markers; every other command is the bootstrap.
        if command.contains("MISSING_PLAYWRIGHT") {
            return (200, json!({"ok": true, "code": 0, "output": shared.lock().unwrap().clone()}));
        }
        (200, json!({"ok": true, "code": 0, "output": "BROWSER_READY"}))
    })
    .await;
    let (st, mut events) = state();
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);
    let body = harbor_body("running", None);

    let mut first = computer::map_status(&format!("http://{address}"), true, &body);
    browser::add_status(&st, &mut first).await;
    assert_eq!(first["browser"]["ready"], false);
    assert!(events.try_recv().is_err(), "the first read only records the value the reply already carries");

    // Playwright arrives: the next read differs, and the window hears about it.
    *mode.lock().unwrap() = "BROWSER_RUNNING".to_owned();
    st.browser.reset_readiness_for_tests();
    let mut second = computer::map_status(&format!("http://{address}"), true, &body);
    browser::add_status(&st, &mut second).await;
    assert_eq!(second["browser"]["ready"], true);
    let event = events.recv().await.unwrap();
    assert_eq!(event.event_type, "browser.changed");
    assert_eq!(event.payload["ready"], true);
    assert_eq!(event.payload["missing"], Value::Null);

    // Unchanged again: nothing more.
    st.browser.reset_readiness_for_tests();
    let mut third = computer::map_status(&format!("http://{address}"), true, &body);
    browser::add_status(&st, &mut third).await;
    assert!(events.try_recv().is_err(), "no event for an unchanged read");
}

#[tokio::test]
async fn browser_refuses_a_page_that_redirects_into_a_local_or_metadata_address() {
    // The address the bot asked for is public; the page lands somewhere else.
    let (address, _) = fake_browser_harbor("BROWSER_RUNNING", |_| whole_page("http://169.254.169.254/latest/meta-data", json!([]))).await;
    let (st, _) = state();
    browser_session(&st, "browser-redirect");
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    let reply = browser_call(&st, "browser-redirect", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(reply["isError"], true, "{reply}");
    let text = reply["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("local or scripted address"), "{text}");
    assert!(!text.contains("latest/meta-data"), "the page is not described: {text}");
    assert_eq!(st.browser.contexts_for_tests(), 1, "the container's context is recorded, so the sweep closes it");

    // The same for a picture: the bytes never reach the media folder.
    let shot = json!({"ok": true, "mediaType": "image/png", "base64": PNG_1X1, "url": "file:///etc/passwd"});
    let (second, _) = fake_browser_harbor("BROWSER_RUNNING", move |_| shot.clone()).await;
    let (st, _) = state();
    browser_session(&st, "browser-redirect-shot");
    link_directly(&st, &format!("http://{second}"), HARBOR_TOKEN, true);
    let reply = browser_call(&st, "browser-redirect-shot", "browser_screenshot", json!({})).await;
    assert_eq!(reply["isError"], true, "{reply}");
    assert!(reply["content"][0]["text"].as_str().unwrap().contains("local or scripted address"), "{reply}");
}

#[test]
fn browser_url_rules_refuse_scripted_pages_files_and_private_addresses() {
    for url in ["https://example.com", "https://example.com/docs?q=1", "http://example.com/", "https://sub.example.co.uk/a/b", "https://example.com:8443/x"] {
        assert!(browser::validate_url(url).is_ok(), "{url} is a page");
    }
    for (url, why) in [
        ("file:///etc/passwd", "a file is not a page"),
        ("FILE:///C:/Windows/win.ini", "case does not make it a page"),
        ("javascript:alert(1)", "script must never run"),
        ("data:text/html,<h1>x</h1>", "a data URL is not a page"),
        ("about:blank", "an internal page"),
        ("chrome://settings", "a browser's own page"),
        ("ftp://example.com/file", "not http"),
        ("http://127.0.0.1:8080/admin", "loopback"),
        ("http://localhost/", "loopback by name"),
        ("http://[::1]/", "IPv6 loopback"),
        ("http://10.0.0.5/", "private range"),
        ("http://172.20.1.1/", "private range"),
        ("http://192.168.1.1/", "private range"),
        ("http://169.254.169.254/latest/meta-data", "cloud metadata"),
        ("http://100.64.1.2/", "a tailnet address"),
        ("http://intranet/", "a name on the local network"),
        ("http://2130706433/", "a decimal address"),
        ("https://user:pass@example.com/", "credentials in the address"),
        ("https://example.com/#frag ment", "a space"),
        ("", "nothing"),
        ("example.com", "no scheme"),
    ] {
        assert!(browser::validate_url(url).is_err(), "{url} must be refused ({why})");
    }
}

#[tokio::test]
async fn browser_tools_are_refused_when_the_computer_is_off_unattended_or_a_teammate_woke_the_turn() {
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", |_| whole_page("https://example.com/", json!([]))).await;
    let (st, _) = state();
    browser_session(&st, "browser-main");

    let unlinked = browser_call(&st, "browser-main", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(unlinked["isError"], true);
    assert!(unlinked["content"][0]["text"].as_str().unwrap().contains("No computer is linked"), "{unlinked}");

    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, false);
    let off = browser_call(&st, "browser-main", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(off["isError"], true);
    assert!(off["content"][0]["text"].as_str().unwrap().contains("switched off"), "{off}");
    assert!(ran.lock().unwrap().is_empty(), "a refused call never reaches the container");
    assert!(!off.to_string().contains(HARBOR_TOKEN));

    st.db.computer_link_set_enabled(true).unwrap();
    // The address rules hold before anything is sent, too.
    let scripted = browser_call(&st, "browser-main", "browser_open", json!({"url": "file:///etc/passwd"})).await;
    assert_eq!(scripted["isError"], true);
    assert!(ran.lock().unwrap().is_empty(), "a refused address never reaches the container");

    // A scheduled run.
    st.db.set_session_link("browser-main", &json!({"kind": "automation", "automationId": "auto-1"})).unwrap();
    let scheduled = browser_call(&st, "browser-main", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(scheduled["isError"], true);
    assert!(scheduled["content"][0]["text"].as_str().unwrap().contains("nobody watching"), "{scheduled}");

    // A side conversation, whose every turn is woken by a teammate's message.
    st.db.set_session_link("browser-main", &json!({"kind": "side", "peerAgentId": "peer", "originSessionId": "browser-origin"})).unwrap();
    let side = browser_call(&st, "browser-main", "browser_snapshot", json!({})).await;
    assert_eq!(side["isError"], true);
    assert!(side["content"][0]["text"].as_str().unwrap().contains("nobody watching"), "{side}");
    assert!(ran.lock().unwrap().is_empty(), "a teammate-woken turn never reaches the container");

    // Watched again: the same call works.
    st.db.set_session_link("browser-main", &json!({})).unwrap();
    let opened = browser_call(&st, "browser-main", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(opened["isError"], Value::Null, "{opened}");
    assert!(!ran.lock().unwrap().is_empty(), "the watched call reached the container");
}

#[tokio::test]
async fn browser_snapshot_pauses_on_a_sign_in_form_and_offers_no_page() {
    let sign_in = json!([
        {"ref": "e0", "tag": "input", "type": "email", "role": "textbox", "name": "Email", "autocomplete": "username", "visible": true},
        {"ref": "e1", "tag": "input", "type": "password", "role": "textbox", "name": "Password", "autocomplete": "current-password", "visible": true},
        {"ref": "e2", "tag": "button", "role": "button", "name": "Sign in", "type": "submit", "visible": true},
    ]);
    let (address, _) = fake_browser_harbor("BROWSER_RUNNING", move |_| whole_page("https://example.com/login", sign_in.clone())).await;
    let (st, _) = state();
    browser_session(&st, "browser-signin");
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    let reply = browser_call(&st, "browser-signin", "browser_snapshot", json!({})).await;
    assert_eq!(reply["isError"], Value::Null, "{reply}");
    let page = reply_of(&reply);
    assert_eq!(page["needs_owner"], true, "{page}");
    assert!(page["reason"].as_str().unwrap().contains("password"), "{page}");
    assert!(page["elements"].is_null(), "an owner-only page is not described to the bot");
}

#[tokio::test]
async fn browser_pauses_on_a_publish_button_and_on_a_verification_code() {
    let post = json!([
        {"ref": "e0", "tag": "textarea", "role": "textbox", "name": "Write a comment", "visible": true},
        {"ref": "e1", "tag": "button", "role": "button", "name": "Post comment", "type": "submit", "visible": true},
    ]);
    assert_eq!(browser::pause_reason(&browser::elements_from(&post)).as_deref(), Some("This page would post or submit something."));
    let code = json!([{"ref": "e0", "tag": "input", "role": "textbox", "name": "Enter the code we sent", "autocomplete": "one-time-code", "visible": true}]);
    assert_eq!(browser::pause_reason(&browser::elements_from(&code)).as_deref(), Some("This page asks for a verification code."));
    let card = json!([{"ref": "e0", "tag": "input", "role": "textbox", "name": "Card number", "autocomplete": "cc-number", "visible": true}]);
    assert_eq!(browser::pause_reason(&browser::elements_from(&card)).as_deref(), Some("This page asks for payment details."));
    // A search page is not a form to sign in with.
    let search = json!([
        {"ref": "e0", "tag": "input", "type": "search", "role": "textbox", "name": "Search", "visible": true},
        {"ref": "e1", "tag": "button", "role": "button", "name": "Search", "type": "submit", "visible": true},
        {"ref": "e2", "tag": "a", "role": "link", "name": "Results", "visible": true},
    ]);
    assert_eq!(browser::pause_reason(&browser::elements_from(&search)), None);
}

#[tokio::test]
async fn browser_click_pauses_on_a_publish_button_before_it_clicks() {
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", |request| match request["action"].as_str() {
        Some("describe") => json!({"ok": true, "target": {"tag": "button", "role": "button", "type": "submit", "name": "Post comment"}}),
        _ => whole_page("https://example.com/thread", json!([])),
    })
    .await;
    let (st, _) = state();
    browser_session(&st, "browser-click");
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    let reply = browser_call(&st, "browser-click", "browser_click", json!({"ref": "e4"})).await;
    assert_eq!(reply["isError"], Value::Null, "{reply}");
    let page = reply_of(&reply);
    assert_eq!(page["needs_owner"], true, "{page}");
    assert!(page["reason"].as_str().unwrap().contains("post"), "{page}");
    let actions: Vec<String> = ran.lock().unwrap().iter().filter_map(|command| decode_action(command)).filter_map(|request| request["action"].as_str().map(str::to_owned)).collect();
    assert!(actions.contains(&"describe".to_owned()), "the target was read first: {actions:?}");
    assert!(!actions.contains(&"click".to_owned()), "nothing was clicked: {actions:?}");
}

#[tokio::test]
async fn browser_typed_text_is_sent_encoded_and_only_its_length_is_reported() {
    let typed = json!({"ok": true, "target": {"tag": "input", "type": "text", "role": "textbox", "name": "Search"}});
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", move |_| typed.clone()).await;
    let (st, _) = state();
    browser_session(&st, "browser-type");
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    // An apostrophe would break a command that carried the text as words.
    let secret = "Ada's private note";
    let reply = browser_call(&st, "browser-type", "browser_type", json!({"ref": "e3", "text": secret})).await;
    assert_eq!(reply["isError"], Value::Null, "{reply}");
    let text = reply["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains(secret), "the reply never repeats what was typed: {text}");
    let page = reply_of(&reply);
    assert_eq!(page["typed"], json!(secret.chars().count()));
    assert!(!text.contains("couldn't"), "no echo of the text in any wording");
    for command in ran.lock().unwrap().iter() {
        assert!(!command.contains(secret), "the command carries the text encoded, never as words");
        assert!(!command.contains("Ada"), "not even a fragment of it");
    }
    let sent = ran.lock().unwrap().iter().filter_map(|command| decode_action(command)).find(|request| request["action"] == "type").expect("the typing action");
    assert_eq!(sent["text"], json!(secret), "the container did receive it, and only there");
}

#[tokio::test]
async fn browser_screenshot_is_stored_as_media_and_never_in_the_reply() {
    let shot = json!({"ok": true, "mediaType": "image/png", "base64": PNG_1X1, "url": "https://example.com/dashboard"});
    let (address, _) = fake_browser_harbor("BROWSER_RUNNING", move |_| shot.clone()).await;
    let (st, _) = state();
    let dir = std::env::temp_dir().join(format!("retinue-browser-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    browser_session(&st, "browser-shot");
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    let reply = browser_call(&st, "browser-shot", "browser_screenshot", json!({})).await;
    assert_eq!(reply["isError"], Value::Null, "{reply}");
    let text = reply["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains(PNG_1X1), "the bytes are never in the tool reply");
    let page = reply_of(&reply);
    let media_id = page["mediaId"].as_str().expect("a media id");
    assert_eq!(page["caption"], "Screenshot of example.com");
    let row = st.db.media_by_id(media_id).unwrap().expect("one media row");
    assert_eq!(row["status"], "ready");
    assert_eq!(row["mediaType"], "image/png");
    assert!(row["bytes"].as_i64().unwrap() > 0);
    assert_eq!(row["provider"], "browser");
    let detail = st.db.session_detail("browser-shot").unwrap();
    let notice = detail["messages"].as_array().unwrap().iter().find(|message| message["meta"]["kind"] == "generated_image").expect("the notice that carries the image");
    assert_eq!(notice["meta"]["mediaId"], media_id);
    assert!(notice["meta"]["detail"].is_null(), "no path or error text in the meta: {}", notice["meta"]);
    assert!(!st.db.media_by_id(media_id).unwrap().unwrap()["name"].as_str().unwrap().contains(&dir.to_string_lossy().to_string()));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn browser_contexts_close_when_the_turn_ends_and_after_ten_idle_minutes() {
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", |_| whole_page("https://example.com/", json!([]))).await;
    let (st, _) = state();
    browser_session(&st, "browser-sweep");
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);

    browser_call(&st, "browser-sweep", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(st.browser.contexts_for_tests(), 1);
    // The turn ends: the next sweep closes the context.
    st.db.update_session_state("browser-sweep", "idle").unwrap();
    browser::sweep_once(&st).await;
    assert_eq!(st.browser.contexts_for_tests(), 0, "a finished turn's context is closed");
    assert!(ran.lock().unwrap().iter().filter_map(|command| decode_action(command)).any(|request| request["action"] == "close"), "the container was told to close it");

    // Ten idle minutes do the same while the turn is still live.
    st.db.update_session_state("browser-sweep", "working").unwrap();
    browser_call(&st, "browser-sweep", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(st.browser.contexts_for_tests(), 1);
    st.browser.age_for_tests("browser-sweep", std::time::Duration::from_secs(601));
    browser::sweep_once(&st).await;
    assert_eq!(st.browser.contexts_for_tests(), 0, "an idle context is closed");
}

#[tokio::test]
async fn computer_status_gains_the_browser_readiness_and_names_the_install_command_when_playwright_is_missing() {
    let (running, _) = fake_browser_harbor("BROWSER_RUNNING", |_| whole_page("https://example.com/", json!([]))).await;
    let (st, _) = state();
    link_directly(&st, &format!("http://{running}"), HARBOR_TOKEN, true);
    let mut status = computer::map_status(&format!("http://{running}"), true, &harbor_body("running", None));
    browser::add_status(&st, &mut status).await;
    assert_eq!(status["browser"]["ready"], true);
    assert_eq!(status["browser"]["missing"], Value::Null);

    // Asleep: not ready, and no install prompt for a container that is not up.
    let mut asleep = computer::map_status(&format!("http://{running}"), true, &harbor_body("exited", None));
    browser::add_status(&st, &mut asleep).await;
    assert_eq!(asleep["browser"]["ready"], false);
    assert_eq!(asleep["browser"]["missing"], Value::Null);

    let (missing, _) = fake_browser_harbor("MISSING_PLAYWRIGHT", |_| json!({"ok": true})).await;
    let (st, _) = state();
    link_directly(&st, &format!("http://{missing}"), HARBOR_TOKEN, true);
    let mut fresh = computer::map_status(&format!("http://{missing}"), true, &harbor_body("running", None));
    browser::add_status(&st, &mut fresh).await;
    assert_eq!(fresh["browser"]["ready"], false);
    let install = fresh["browser"]["missing"].as_str().expect("the install command");
    assert!(install.contains("npm i playwright"), "{install}");
    assert!(install.contains("playwright install"), "{install}");

    // A tool call says the same thing in words, not a bare failure.
    browser_session(&st, "browser-missing");
    st.db.update_session_state("browser-missing", "working").unwrap();
    let reply = browser_call(&st, "browser-missing", "browser_open", json!({"url": "https://example.com"})).await;
    assert_eq!(reply["isError"], true, "{reply}");
    let text = reply["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("npm i playwright"), "{text}");
}

#[tokio::test]
async fn browser_mcp_server_is_offered_only_while_linked_and_enabled_and_carries_no_token() {
    let (st, _) = state();
    let dir = std::env::temp_dir().join(format!("retinue-browser-endpoint-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    *st.team.address.lock().unwrap() = Some("127.0.0.1:9".to_owned());

    assert!(browser::mcp_server_for(&st, "session-one").is_none(), "nothing linked, no browser");
    link_directly(&st, "http://127.0.0.1:1", HARBOR_TOKEN, false);
    assert!(browser::mcp_server_for(&st, "session-one").is_none(), "linked but switched off, no browser");
    st.db.computer_link_set_enabled(true).unwrap();

    let spec = browser::mcp_server_for(&st, "session-one").expect("a linked, enabled computer is offered a browser");
    assert_eq!(spec.name, "vps-browser");
    assert!(spec.env.is_empty(), "no token in the environment");
    assert_eq!(spec.args.first().map(String::as_str), Some("browser-server"));
    assert_eq!(spec.args.get(1).map(String::as_str), Some("--endpoint"));
    let endpoint = std::path::PathBuf::from(spec.args.get(2).expect("an endpoint path"));
    assert!(endpoint.starts_with(dir.join("browser")), "its own endpoint folder: {endpoint:?}");
    let written = std::fs::read_to_string(&endpoint).unwrap();
    assert!(written.contains("\"address\":\"127.0.0.1:9\""), "{written}");
    assert!(!written.contains(HARBOR_TOKEN), "the endpoint file names the daemon, not Harbor");

    browser::sweep_endpoints(&dir);
    assert!(!endpoint.exists(), "a restart sweeps the endpoint file");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn browser_tools_share_the_one_permission_category_and_always_ask() {
    let names: Vec<String> = browser::tool_schemas().iter().map(|tool| tool["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(names, vec!["browser_open", "browser_snapshot", "browser_click", "browser_type", "browser_press", "browser_back", "browser_screenshot"]);
    for name in &names {
        // The provider's prefixed spelling is what the permission request carries.
        let classified = retinue_agent_core::classify_permission(&format!("mcp__vps-browser__{name}"), &json!({"ref": "e1", "text": "x"}), std::path::Path::new("."));
        assert_eq!(classified.category, "BROWSER", "{name}");
        assert!(!classified.allowed, "{name} asks rather than being approved");
    }
    // The computer's own tools are not the browser.
    assert_ne!(retinue_agent_core::classify_permission("computer_exec", &json!({"command": "id"}), std::path::Path::new(".")).category, "BROWSER");
}

#[test]
fn browser_refs_keys_and_text_are_bounded() {
    assert_eq!(browser::ref_of("e12").as_deref(), Ok("e12"));
    for bad in ["", "12", "e", "e12x", "x12", "e1234567"] {
        assert!(browser::ref_of(bad).is_err(), "{bad} is not a reference");
    }
    assert_eq!(browser::key_of("Control+A").as_deref(), Ok("Control+A"));
    for bad in ["", "Enter; rm -rf /", "a'b", &"k".repeat(41)] {
        assert!(browser::key_of(bad).is_err(), "{bad} is not one key");
    }
    assert!(browser::text_of(&"x".repeat(2_001)).is_err(), "typing is capped");
    assert!(browser::text_of("   ").is_err(), "whitespace is not text");
    assert!(browser::text_of("Ada\u{0}").is_err(), "no NUL");
    assert_eq!(browser::text_of("Ada").as_deref(), Ok("Ada"));
}
