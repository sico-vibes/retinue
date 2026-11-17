// The vault (16 November 2026). Included into `phase2a_tests` after
// `browser_tests.rs`, so it shares that module's fixtures: `state`, `dispatch`,
// `json!`, `Value`, `Uuid`, `Arc`, `fake_browser_harbor`, `link_directly`,
// `browser_session`, `browser_call`, `reply_of` and `HARBOR_TOKEN`. Test builds
// keep credential values in memory (`secrets::store`), so no test touches the
// real store.

use std::time::Duration as VaultDuration;

/// A password no reply, event or ledger row may ever contain.
const VAULT_SENTINEL: &str = "vault-SENTINEL-pw-9c1f";
const VAULT_TOTP: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

async fn vault_login(st: &AppState, origin: &str) -> Value {
    dispatch(
        st,
        "vault.create",
        json!({
            "kind": "login",
            "title": "Work email",
            "origin": origin,
            "username": "ada@example.com",
            "secrets": {"password": VAULT_SENTINEL, "totp": VAULT_TOTP}
        }),
    )
    .await
    .unwrap()["item"]
    .clone()
}

async fn vault_open_uses(st: &AppState) -> Vec<Value> {
    dispatch(st, "vault.use.list", json!({})).await.unwrap()["requests"].as_array().unwrap().clone()
}

async fn vault_wait_for_use(st: &AppState) -> Value {
    for _ in 0..100 {
        if let Some(row) = vault_open_uses(st).await.into_iter().next() {
            return row;
        }
        tokio::time::sleep(VaultDuration::from_millis(20)).await;
    }
    panic!("no vault use appeared");
}

/// A conversation the owner can answer in, with a running turn so a fill can be
/// told when it ends.
async fn vault_asking(st: &AppState, id: &str) -> String {
    browser_session(st, id);
    st.active_turns.lock().await.insert(id.to_owned(), format!("turn-{id}"));
    id.to_owned()
}

fn vault_events(st: &AppState) -> String {
    st.db.replay_events(0, 100_000).map(|events| json!(events).to_string()).unwrap_or_default()
}

#[tokio::test]
async fn no_reply_or_ledger_row_holds_a_value() {
    let (st, _) = state();
    let created = dispatch(
        &st,
        "vault.create",
        json!({
            "kind": "login",
            "title": "Work email",
            "origin": "https://mail.example.com/",
            "username": "ada@example.com",
            "secrets": {"password": VAULT_SENTINEL, "totp": VAULT_TOTP}
        }),
    )
    .await
    .unwrap();
    let item = created["item"].clone();
    let id = item["id"].as_str().unwrap().to_owned();
    assert_eq!(item["origin"], "https://mail.example.com", "the origin is stored as an origin");
    assert_eq!(item["kind"], "login");
    assert_eq!(item["hasTotp"], true);
    assert_eq!(item["useCount"], 0);
    assert!(!created.to_string().contains(VAULT_SENTINEL), "the reply never holds the value");

    let listed = dispatch(&st, "vault.list", json!({})).await.unwrap();
    assert!(!listed.to_string().contains(VAULT_SENTINEL));
    let bot_view = crate::vault::tool_list(&st, "any", &json!({})).await.unwrap();
    assert!(bot_view.contains(&id), "the bot sees the id: {bot_view}");
    assert!(bot_view.contains("https://mail.example.com"), "{bot_view}");
    assert!(!bot_view.contains(VAULT_SENTINEL));
    assert!(!bot_view.contains("ada@example.com"), "the bot's list carries no user name: {bot_view}");

    // Reveal is the one way back, and it is audited.
    let revealed = dispatch(&st, "vault.reveal", json!({"id": id, "field": "password"})).await.unwrap();
    assert_eq!(revealed["value"], VAULT_SENTINEL);
    let code = dispatch(&st, "vault.totp", json!({"id": id})).await.unwrap();
    assert_eq!(code["code"].as_str().unwrap().len(), 6);
    assert!(code["expiresInSecs"].as_i64().unwrap() > 0);

    let rows = dispatch(&st, "vault.activity", json!({"itemId": id})).await.unwrap()["rows"].clone();
    let actions: Vec<&str> = rows.as_array().unwrap().iter().filter_map(|row| row["action"].as_str()).collect();
    assert_eq!(actions, ["reveal", "reveal", "create"], "newest first");
    assert_eq!(rows[0]["where"], "desktop");
    assert!(!rows.to_string().contains(VAULT_SENTINEL), "the ledger is content-free: {rows}");
    assert!(!vault_events(&st).contains(VAULT_SENTINEL));

    // An update that leaves the password out keeps it; an empty one is refused.
    dispatch(&st, "vault.update", json!({"id": id, "title": "Work mail"})).await.unwrap();
    assert_eq!(dispatch(&st, "vault.reveal", json!({"id": id, "field": "password"})).await.unwrap()["value"], VAULT_SENTINEL);
    assert_eq!(dispatch(&st, "vault.update", json!({"id": id, "secrets": {"password": ""}})).await.unwrap_err().1.code, "invalid_argument");

    // Delete takes the credentials with it.
    assert_eq!(dispatch(&st, "vault.delete", json!({"id": id})).await.unwrap_err().1.code, "invalid_argument", "delete needs confirm");
    dispatch(&st, "vault.delete", json!({"id": id, "confirm": true})).await.unwrap();
    assert!(st.db.vault_item(&id).unwrap().is_none());
    assert!(secrets::store::read(&format!("vault/{id}/password")).unwrap().is_none(), "the credential is gone too");
    assert!(secrets::store::read(&format!("vault/{id}/totp")).unwrap().is_none());
}

#[tokio::test]
async fn a_card_is_checked_and_its_brand_and_last_four_are_derived() {
    let (st, _) = state();
    let created = dispatch(
        &st,
        "vault.create",
        json!({"kind": "card", "title": "Ada's card", "holder": "Ada Lovelace", "expiry": "08/28", "secrets": {"number": "4242 4242 4242 4242", "cvc": "123"}}),
    )
    .await
    .unwrap();
    assert_eq!(created["item"]["brand"], "Visa");
    assert_eq!(created["item"]["last4"], "4242");
    assert!(!created.to_string().contains("4242424242424242"), "the number never comes back: {created}");
    let refused = dispatch(&st, "vault.create", json!({"kind": "card", "title": "Bad", "secrets": {"number": "4242424242424241"}})).await.unwrap_err();
    assert_eq!(refused.1.code, "invalid_argument");
    assert!(refused.1.message.contains("Luhn"), "{}", refused.1.message);
    let expiry = dispatch(&st, "vault.create", json!({"kind": "card", "title": "Bad date", "expiry": "13/28", "secrets": {"number": "4242424242424242"}})).await.unwrap_err();
    assert_eq!(expiry.1.code, "invalid_argument");
    // A login needs an origin; without one it is refused rather than guessed.
    assert_eq!(dispatch(&st, "vault.create", json!({"kind": "login", "title": "No site", "secrets": {"password": "x"}})).await.unwrap_err().1.code, "invalid_argument");
}

#[tokio::test]
async fn a_fill_waits_for_the_owner_and_types_the_value_into_the_page() {
    let script = |request: &Value| match request["action"].as_str() {
        Some("describe") => json!({"ok": true, "target": {"tag": "input", "type": "password", "role": "textbox", "name": "password field"}, "url": "https://mail.example.com/login"}),
        Some("fill") => json!({"ok": true, "filled": true}),
        _ => json!({"ok": true, "url": "https://mail.example.com/login", "title": "Sign in", "nodes": [], "tree": ""}),
    };
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", script).await;
    let (st, mut events) = state();
    let session = vault_asking(&st, "vault-page").await;
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);
    let item = vault_login(&st, "https://mail.example.com").await;
    let id = item["id"].as_str().unwrap().to_owned();

    let calling = st.clone();
    let call = tokio::spawn(async move {
        crate::vault::tool_fill_with(&calling, "vault-page", &json!({"itemId": id, "field": "password", "target": {"surface": "browser", "ref": "e2"}}), VaultDuration::from_millis(1_500)).await
    });
    let open = vault_wait_for_use(&st).await;
    assert_eq!(open["sessionId"], session);
    assert_eq!(open["itemTitle"], "Work email");
    assert_eq!(open["kind"], "login");
    assert_eq!(open["origin"], "https://mail.example.com");
    assert_eq!(open["surface"], "browser");
    assert_eq!(open["field"], "password");
    assert!(!open.to_string().contains(VAULT_SENTINEL));
    let mut requested = false;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "vault.use.requested" {
            assert!(!event.payload.to_string().contains(VAULT_SENTINEL));
            requested = true;
        }
    }
    assert!(requested, "the owner is told a use is waiting");

    dispatch(&st, "vault.use.answer", json!({"requestId": open["requestId"], "decision": "allow"})).await.unwrap();
    let result = call.await.unwrap().unwrap();
    assert_eq!(result, "filled", "the tool says only what happened");

    let sent = ran.lock().unwrap().iter().filter_map(|command| decode_action(command)).find(|request| request["action"] == "fill").expect("the fill reached the container");
    assert_eq!(sent["text"], json!(VAULT_SENTINEL), "the value went into the field, and only there");
    assert_eq!(sent["ref"], json!("e2"));
    let reply_text = serde_json::to_string(&st.db.session_detail(&session).unwrap()).unwrap();
    assert!(!reply_text.contains(VAULT_SENTINEL), "nothing of the value is in the conversation");

    let rows = dispatch(&st, "vault.activity", json!({"itemId": item["id"]})).await.unwrap()["rows"].clone();
    assert_eq!(rows[0]["action"], "fill");
    assert_eq!(rows[0]["where"], "browser");
    assert_eq!(rows[0]["bot"], Value::Null, "a conversation with no bot has no bot name");
    assert_eq!(rows[0]["sessionId"], session);
    assert!(!rows.to_string().contains(VAULT_SENTINEL));
    let used = st.db.vault_item(item["id"].as_str().unwrap()).unwrap().unwrap();
    assert_eq!(used["useCount"], 1);
    assert!(used["lastUsedAt"].as_str().is_some());
    // The owner's answer is announced first ("allowed"), then the outcome ("fill").
    let mut statuses = Vec::new();
    while let Ok(event) = events.try_recv() {
        if event.event_type == "vault.use.updated" {
            statuses.push(event.payload["status"].as_str().unwrap_or("").to_owned());
        }
    }
    assert_eq!(statuses, vec!["allowed".to_owned(), "fill".to_owned()]);
}

#[tokio::test]
async fn a_page_on_another_origin_is_refused_and_recorded_as_denied() {
    let script = |request: &Value| match request["action"].as_str() {
        Some("describe") => json!({"ok": true, "target": {"tag": "input", "type": "password", "role": "textbox", "name": "password field"}, "url": "https://elsewhere.example.net/login"}),
        _ => json!({"ok": true, "filled": true}),
    };
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", script).await;
    let (st, _) = state();
    vault_asking(&st, "vault-wrong-origin").await;
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);
    let item = vault_login(&st, "https://mail.example.com").await;
    let id = item["id"].as_str().unwrap().to_owned();

    let calling = st.clone();
    let call = tokio::spawn(async move {
        crate::vault::tool_fill_with(&calling, "vault-wrong-origin", &json!({"itemId": id, "field": "password", "target": {"surface": "browser", "ref": "e2"}}), VaultDuration::from_millis(1_500)).await
    });
    let open = vault_wait_for_use(&st).await;
    dispatch(&st, "vault.use.answer", json!({"requestId": open["requestId"], "decision": "allow"})).await.unwrap();
    assert_eq!(call.await.unwrap().unwrap(), "denied");
    let fills = ran.lock().unwrap().iter().filter_map(|command| decode_action(command)).filter(|request| request["action"] == "fill").count();
    assert_eq!(fills, 0, "a page on another origin never receives the value");
    let rows = dispatch(&st, "vault.activity", json!({"itemId": item["id"]})).await.unwrap()["rows"].clone();
    assert_eq!(rows[0]["action"], "denied");
    assert_eq!(rows[0]["where"], "browser");
}

#[tokio::test]
async fn a_denial_and_an_unanswered_use_are_recorded_and_nothing_is_typed() {
    let script = |request: &Value| match request["action"].as_str() {
        Some("describe") => json!({"ok": true, "target": {"tag": "input", "type": "password", "role": "textbox", "name": "password field"}, "url": "https://mail.example.com/login"}),
        _ => json!({"ok": true, "filled": true}),
    };
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", script).await;
    let (st, _) = state();
    vault_asking(&st, "vault-denied").await;
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);
    let item = vault_login(&st, "https://mail.example.com").await;
    let id = item["id"].as_str().unwrap().to_owned();
    let args = json!({"itemId": id, "field": "password", "target": {"surface": "browser", "ref": "e2"}});

    let calling = st.clone();
    let args_for_call = args.clone();
    let call = tokio::spawn(async move { crate::vault::tool_fill_with(&calling, "vault-denied", &args_for_call, VaultDuration::from_millis(1_500)).await });
    let open = vault_wait_for_use(&st).await;
    dispatch(&st, "vault.use.answer", json!({"requestId": open["requestId"], "decision": "deny"})).await.unwrap();
    assert_eq!(call.await.unwrap().unwrap(), "denied");
    assert_eq!(st.db.vault_activity(Some(&id), 10).unwrap()[0]["action"], "denied");

    // Nobody answers: the use expires, and the bot is told so.
    let expired = crate::vault::tool_fill_with(&st, "vault-denied", &args, VaultDuration::from_millis(250)).await.unwrap();
    assert_eq!(expired, "expired");
    assert_eq!(st.db.vault_activity(Some(&id), 10).unwrap()[0]["action"], "expired");
    let fills = ran.lock().unwrap().iter().filter_map(|command| decode_action(command)).filter(|request| request["action"] == "fill").count();
    assert_eq!(fills, 0, "nothing was typed");
}

#[tokio::test]
async fn full_access_and_unattended_runs_get_no_vault_use() {
    let (st, _) = state();
    let agent = st.db.agent_create(&json!({"name": "Ada", "runtimeId": "rt-test", "approvalMode": "bypass"})).unwrap().0;
    st.db.create_session_for_agent("vault-full", "rt-test", "codex", ".", "Chat", agent["id"].as_str()).unwrap();
    st.db.update_session_state("vault-full", "idle").unwrap();
    let item = vault_login(&st, "https://mail.example.com").await;
    let id = item["id"].as_str().unwrap().to_owned();
    let full = crate::vault::tool_fill_with(&st, "vault-full", &json!({"itemId": id, "field": "password", "target": {"surface": "cloud"}}), VaultDuration::from_millis(200)).await;
    assert!(full.unwrap_err().contains("Full access"));

    st.db.create_session("vault-task", "rt-test", "codex", ".", "Task").unwrap();
    st.db.set_session_link("vault-task", &json!({"kind": "task", "unattended": true})).unwrap();
    st.db.update_session_state("vault-task", "idle").unwrap();
    let unattended = crate::vault::tool_fill_with(&st, "vault-task", &json!({"itemId": id, "field": "password", "target": {"surface": "cloud"}}), VaultDuration::from_millis(200)).await;
    let refusal = unattended.unwrap_err();
    assert!(refusal.contains("nobody is watching"), "{refusal}");
    assert!(vault_open_uses(&st).await.is_empty(), "no card was raised");
}

#[tokio::test]
async fn a_filled_value_is_never_echoed_by_a_later_snapshot() {
    let page = json!({
        "ok": true,
        "url": "https://mail.example.com/inbox?token=secret",
        "title": format!("Welcome {VAULT_SENTINEL}"),
        "nodes": [{"ref": "e1", "tag": "input", "role": "textbox", "type": "text", "name": VAULT_SENTINEL, "visible": true}],
        "tree": format!("- textbox \"{VAULT_SENTINEL}\""),
    });
    let (address, ran) = fake_browser_harbor("BROWSER_RUNNING", move |_| page.clone()).await;
    let (st, _) = state();
    vault_asking(&st, "vault-redact").await;
    link_directly(&st, &format!("http://{address}"), HARBOR_TOKEN, true);
    // The daemon knows the value because the vault typed it into e1.
    st.browser.remember("vault-redact", VAULT_SENTINEL, Some("e1"));

    let reply = browser_call(&st, "vault-redact", "browser_snapshot", json!({})).await;
    let text = reply["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains(VAULT_SENTINEL), "the snapshot never repeats a filled value: {text}");
    assert!(text.contains("[redacted]"), "{text}");
    let page = reply_of(&reply);
    assert!(page["elements"][0]["name"].as_str().unwrap().contains("[redacted]"), "{page}");
    // The page was told which reference to mask, so it prints "(filled)" itself.
    let sent = ran.lock().unwrap().iter().filter_map(|command| decode_action(command)).find(|request| request["action"] == "snapshot").expect("a snapshot reached the container");
    assert_eq!(sent["mask"], json!(["e1"]));

    // A context that closes forgets the value.
    st.browser.forget_for_tests("vault-redact");
    assert_eq!(st.browser.redact("vault-redact", VAULT_SENTINEL), VAULT_SENTINEL);
}
