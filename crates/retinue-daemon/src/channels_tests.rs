// 16 November 2026: the chat channels to the CEO.
//
// The channel is one per kind, like the product's, and the credential store is
// shared by the whole test process, so these tests take turns behind one lock.
// Each one points the Telegram and Graph clients at a local server of its own
// with the test-only base override, which is why nothing here can reach the
// real Telegram or Meta.

/// One test at a time: both the channel row and the test credential store are
/// process-wide, like the product's one-channel-per-kind. An async lock, so the
/// guard is not a standard guard held across an await.
async fn channel_test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    LOCK.lock().await
}

#[derive(Clone)]
struct FakeTelegramState {
    sent: Arc<std::sync::Mutex<Vec<Value>>>,
    answered: Arc<std::sync::Mutex<Vec<Value>>>,
    updates: Arc<std::sync::Mutex<Vec<Value>>>,
    me: Value,
}

struct FakeTelegram {
    base: String,
    state: FakeTelegramState,
}

impl FakeTelegram {
    fn queue(&self, update: Value) { self.state.updates.lock().unwrap().push(update); }

    /// Every `sendMessage` the daemon made, in order.
    fn sent(&self) -> Vec<Value> { self.state.sent.lock().unwrap().clone() }

    fn sent_texts(&self) -> Vec<String> {
        self.sent().iter().filter_map(|body| body["text"].as_str().map(str::to_owned)).collect()
    }
}

async fn fake_telegram(me: Value) -> FakeTelegram {
    async fn handler(axum::extract::State(state): axum::extract::State<FakeTelegramState>, request: axum::extract::Request) -> axum::Json<Value> {
        let method = request.uri().path().rsplit('/').next().unwrap_or("").to_owned();
        let body: Value = axum::body::to_bytes(request.into_body(), 1 << 20).await.ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
        match method.as_str() {
            "getMe" => axum::Json(json!({"ok": true, "result": state.me})),
            "sendMessage" => {
                state.sent.lock().unwrap().push(body);
                axum::Json(json!({"ok": true, "result": {"message_id": 1}}))
            }
            "answerCallbackQuery" => {
                state.answered.lock().unwrap().push(body);
                axum::Json(json!({"ok": true, "result": true}))
            }
            // A real long poll waits for the bot's timeout; this one waits just
            // long enough that the poller is not a busy loop.
            "getUpdates" => {
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                let queued: Vec<Value> = std::mem::take(&mut *state.updates.lock().unwrap());
                axum::Json(json!({"ok": true, "result": queued}))
            }
            _ => axum::Json(json!({"ok": false, "description": "unknown method"})),
        }
    }
    let state = FakeTelegramState {
        sent: Arc::new(std::sync::Mutex::new(Vec::new())),
        answered: Arc::new(std::sync::Mutex::new(Vec::new())),
        updates: Arc::new(std::sync::Mutex::new(Vec::new())),
        me,
    };
    let app = axum::Router::new().fallback(handler).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { let _ = axum::serve(listener, app).await; });
    FakeTelegram { base, state }
}

fn point_telegram_at(base: &str) { *channels::TEST_API_BASE.lock().unwrap_or_else(|error| error.into_inner()) = Some(base.to_owned()); }
fn forget_telegram_base() { *channels::TEST_API_BASE.lock().unwrap_or_else(|error| error.into_inner()) = None; }
fn point_cloud_at(base: &str) { *channels::TEST_CLOUD_API_BASE.lock().unwrap_or_else(|error| error.into_inner()) = Some(base.to_owned()); }
fn forget_cloud_base() { *channels::TEST_CLOUD_API_BASE.lock().unwrap_or_else(|error| error.into_inner()) = None; }

/// Polls a condition for up to about five seconds. These tests wait on a real
/// poller and a real turn, so a fixed sleep would be either slow or flaky.
async fn channel_wait(done: impl Fn() -> bool) -> bool {
    for _ in 0..300 {
        if done() { return true }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    false
}

fn private_update(update_id: i64, message_id: i64, sender: i64, text: &str) -> Value {
    json!({
        "update_id": update_id,
        "message": {
            "message_id": message_id,
            "date": 1_760_000_000,
            "chat": {"id": sender, "type": "private"},
            "from": {"id": sender, "is_bot": false, "first_name": "Vex", "username": "vex"},
            "text": text,
        }
    })
}

/// A state with a CEO, an online `rt-test` runtime and a private data folder.
async fn channel_state() -> (AppState, Arc<TeamAdapter>, std::path::PathBuf) {
    let (st, adapter) = online_team_state().await;
    let data_dir = std::env::temp_dir().join(format!("retinue-channel-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(data_dir.clone());
    dispatch(&st, "manager.ensure", json!({"name": "Chief", "runtimeId": "rt-test"})).await.unwrap();
    (st, adapter, data_dir)
}

#[tokio::test]
async fn every_channel_kind_is_listed_with_its_defaults() {
    let st = { let (st, _) = state(); st };
    let listed = dispatch(&st, "channel.list", json!({})).await.unwrap();
    let channels = listed["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 3);
    assert_eq!(channels.iter().map(|row| row["kind"].as_str().unwrap()).collect::<Vec<_>>(), vec!["telegram", "whatsapp-web", "whatsapp-cloud"]);
    for row in channels {
        assert_eq!(row["status"], "off", "{row}");
        assert_eq!(row["approvals"], true, "approval buttons are on by default");
        assert!(row["qrDataUrl"].is_null());
        assert!(row["peer"].is_null());
        assert!(row["pairing"].is_null());
    }
}

#[tokio::test]
async fn a_channel_needs_a_ceo_first() {
    let _turn = channel_test_lock().await;
    let (st, _) = state();
    let refused = dispatch(&st, "channel.telegram.connect", json!({"botToken": "123:abc"})).await.unwrap_err();
    assert_eq!(refused.1.message, "Hire your CEO first.");
    let cloud = dispatch(&st, "channel.whatsappCloud.connect", json!({"phoneNumberId": "1", "accessToken": "t", "appSecret": "0123456789abcdef"})).await.unwrap_err();
    assert_eq!(cloud.1.message, "Hire your CEO first.");
}

#[tokio::test]
async fn whatsapp_web_is_refused_and_says_why() {
    let _turn = channel_test_lock().await;
    let (st, _) = state();
    let refused = dispatch(&st, "channel.whatsappWeb.start", json!({})).await.unwrap_err();
    assert_eq!(refused.1.code, "unsupported");
    assert!(refused.1.message.contains("doesn't ship"), "{}", refused.1.message);
    assert!(st.db.channel_get("whatsapp-web").unwrap().is_none(), "nothing is stored for a channel that cannot run");
}

#[tokio::test]
async fn telegram_connect_checks_the_token_and_opens_one_pairing_window() {
    let _turn = channel_test_lock().await;
    let fake = fake_telegram(json!({"id": 4242, "is_bot": true, "username": "retinue_test_bot"})).await;
    point_telegram_at(&fake.base);
    let (st, _, data_dir) = channel_state().await;

    let connected = dispatch(&st, "channel.telegram.connect", json!({"botToken": "4242:AA-token"})).await.unwrap();
    assert_eq!(connected["channel"]["status"], "pairing");
    assert_eq!(connected["channel"]["account"], "@retinue_test_bot");
    let link = connected["pairing"]["link"].as_str().unwrap();
    let code = connected["pairing"]["code"].as_str().unwrap().to_owned();
    assert_eq!(code.len(), 8);
    assert_eq!(link, format!("https://t.me/retinue_test_bot?start={code}"));
    assert!(connected["pairing"]["expiresAt"].as_str().unwrap() > chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true).as_str());
    // The token is in the credential store and nowhere near the database.
    let row = st.db.channel_get("telegram").unwrap().unwrap();
    assert!(!row.to_string().contains("AA-token"), "{row}");
    assert!(channels::read_secret("telegram", "token").is_some());

    // A token that is not a bot's is refused, and the page says so.
    let not_a_bot = fake_telegram(json!({"id": 1, "is_bot": false, "username": "person"})).await;
    point_telegram_at(&not_a_bot.base);
    let refused = dispatch(&st, "channel.telegram.connect", json!({"botToken": "1:other"})).await.unwrap_err();
    assert_eq!(refused.1.code, "invalid_argument");
    let after_refusal = st.db.channel_get("telegram").unwrap().unwrap();
    assert_eq!(after_refusal["account"], "@retinue_test_bot", "a refused token leaves the working channel alone");
    assert_ne!(after_refusal["status"], "connecting", "the page never stays on `connecting` for a refusal");

    // `channel.list` still carries the open window, so a reopened page can show
    // the code again.
    let listed = dispatch(&st, "channel.list", json!({})).await.unwrap();
    let telegram = listed["channels"].as_array().unwrap().iter().find(|row| row["kind"] == "telegram").cloned().unwrap();
    assert_eq!(telegram["pairing"]["code"], code.as_str());

    dispatch(&st, "channel.disconnect", json!({"kind": "telegram"})).await.unwrap();
    assert!(channels::read_secret("telegram", "token").is_none(), "disconnect removes the token");
    assert_eq!(dispatch(&st, "channel.list", json!({})).await.unwrap()["channels"][0]["status"], "off");
    forget_telegram_base();
    let _ = std::fs::remove_dir_all(data_dir);
}

#[tokio::test]
async fn a_stranger_who_guesses_wrongly_never_pairs_and_five_tries_burn_the_code() {
    let _turn = channel_test_lock().await;
    let fake = fake_telegram(json!({"id": 7, "is_bot": true, "username": "retinue_test_bot"})).await;
    point_telegram_at(&fake.base);
    let (st, _, data_dir) = channel_state().await;
    let connected = dispatch(&st, "channel.telegram.connect", json!({"botToken": "7:token"})).await.unwrap();
    let code = connected["pairing"]["code"].as_str().unwrap().to_owned();

    let api = channels::Telegram::new(&fake.base, "7:token").unwrap();
    // A message with no code, from anyone, does nothing at all.
    channels::handle_update(&st, &api, &private_update(1, 1, 999, "hello?")).await;
    assert!(st.db.channel_get("telegram").unwrap().unwrap()["candidate"].is_null());

    // A group is refused before anything is read.
    let mut group = private_update(2, 2, 999, &format!("/start {code}"));
    group["message"]["chat"]["type"] = json!("group");
    channels::handle_update(&st, &api, &group).await;
    assert!(st.db.channel_get("telegram").unwrap().unwrap()["candidate"].is_null(), "a group never reaches the CEO");

    // Five wrong codes close the window for good.
    for attempt in 0..5 {
        channels::handle_update(&st, &api, &private_update(10 + attempt, 10 + attempt, 999, "/start WRONG123")).await;
    }
    let after = dispatch(&st, "channel.list", json!({})).await.unwrap();
    let telegram = after["channels"].as_array().unwrap().iter().find(|row| row["kind"] == "telegram").cloned().unwrap();
    assert!(telegram["pairing"].is_null(), "the window is closed: {telegram}");

    // Even the right code does nothing now: the sender has to start again.
    channels::handle_update(&st, &api, &private_update(20, 20, 999, &format!("/start {code}"))).await;
    assert!(st.db.channel_get("telegram").unwrap().unwrap()["candidate"].is_null());
    forget_telegram_base();
    let _ = std::fs::remove_dir_all(data_dir);
}

#[tokio::test]
async fn the_right_code_makes_a_candidate_that_only_the_desktop_can_approve() {
    let _turn = channel_test_lock().await;
    let fake = fake_telegram(json!({"id": 8, "is_bot": true, "username": "retinue_test_bot"})).await;
    point_telegram_at(&fake.base);
    let (st, _adapter, data_dir) = channel_state().await;
    let mut events = st.events.subscribe();
    let connected = dispatch(&st, "channel.telegram.connect", json!({"botToken": "8:token"})).await.unwrap();
    let code = connected["pairing"]["code"].as_str().unwrap().to_owned();

    let api = channels::Telegram::new(&fake.base, "8:token").unwrap();
    channels::handle_update(&st, &api, &private_update(1, 1, 555, &format!("/start {code}"))).await;
    let row = st.db.channel_get("telegram").unwrap().unwrap();
    assert_eq!(row["status"], "pairing");
    assert!(row["peer"].is_null(), "a candidate is not a paired sender");
    assert_eq!(row["candidate"]["name"], "Vex");
    assert_eq!(row["candidate"]["handle"], "@vex");
    // The message that carried the code starts no turn at all.
    assert!(row["sessionId"].is_null());
    let mut announced = false;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "channel.pairing.candidate" && event.payload["handle"] == "@vex" { announced = true; }
    }
    assert!(announced, "the page is told there is a candidate");

    let approved = dispatch(&st, "channel.pairing.answer", json!({"kind": "telegram", "approve": true})).await.unwrap();
    assert_eq!(approved["channel"]["status"], "connected");
    assert_eq!(approved["channel"]["peer"], json!({"name": "Vex", "handle": "@vex"}));
    assert!(approved["channel"]["candidate"].is_null());
    assert!(channel_wait(|| fake.sent_texts().iter().any(|text| text.contains("Retinue is connected"))).await, "the chat is told it worked: {:?}", fake.sent_texts());

    // The code is spent: a second sender cannot take the channel over.
    channels::handle_update(&st, &api, &private_update(2, 2, 777, "/start 23456789")).await;
    assert!(st.db.channel_get("telegram").unwrap().unwrap()["candidate"].is_null());
    forget_telegram_base();
    let _ = std::fs::remove_dir_all(data_dir);
}

#[tokio::test]
async fn a_paired_message_starts_one_turn_and_only_the_ceo_reply_comes_back() {
    let _turn = channel_test_lock().await;
    let fake = fake_telegram(json!({"id": 9, "is_bot": true, "username": "retinue_test_bot"})).await;
    point_telegram_at(&fake.base);
    let (st, adapter, data_dir) = channel_state().await;
    let connected = dispatch(&st, "channel.telegram.connect", json!({"botToken": "9:token"})).await.unwrap();
    let code = connected["pairing"]["code"].as_str().unwrap().to_owned();
    let api = channels::Telegram::new(&fake.base, "9:token").unwrap();
    channels::handle_update(&st, &api, &private_update(1, 1, 555, &format!("/start {code}"))).await;
    dispatch(&st, "channel.pairing.answer", json!({"kind": "telegram", "approve": true})).await.unwrap();

    // The message arrives over the wire, through the running poller.
    fake.queue(private_update(2, 42, 555, "Summarise the build"));
    assert!(channel_wait(|| st.db.channel_get("telegram").unwrap().unwrap()["sessionId"].is_string()).await, "the CEO's conversation is created");
    let row = st.db.channel_get("telegram").unwrap().unwrap();
    let session = row["sessionId"].as_str().unwrap().to_owned();
    let messages = wait_for_messages(&st, &session, |messages| messages.iter().any(|message| message["content"] == "Summarise the build")).await;
    // The CEO's channel conversation is titled for the channel and the message
    // says where it came from.
    assert_eq!(st.db.session_brief(&session).unwrap()["title"], "Telegram");
    let inbound = messages.iter().find(|message| message["content"] == "Summarise the build").unwrap();
    assert_eq!(inbound["meta"]["channel"], "telegram");
    assert!(inbound["meta"]["clientMessageId"].is_string(), "the ledger key travels with the message, so a retry is free");

    // Only the final reply goes back, and the ledger records the turn.
    assert!(channel_wait(|| fake.sent_texts().iter().any(|text| text.contains("handled: Summarise the build"))).await, "{:?}", fake.sent_texts());
    assert!(channel_wait(|| st.db.channel_inbox_row("channel:telegram:retinue_test_bot:42").unwrap().is_some_and(|row| row["state"] == "submitted")).await, "the ledger records the turn");
    let ledger = st.db.channel_inbox_row("channel:telegram:retinue_test_bot:42").unwrap().unwrap();
    assert_eq!(ledger["state"], "submitted");
    assert!(ledger["turnId"].as_str().is_some(), "the ledger records the turn");
    assert!(st.db.channel_get("telegram").unwrap().unwrap()["lastMessageAt"].as_str().is_some(), "the row records when the chat last spoke");

    // The same delivery again (a provider retry) starts no second turn.
    fake.queue(private_update(2, 42, 555, "Summarise the build"));
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let again = st.db.session_detail(&session).unwrap();
    let said = again["messages"].as_array().unwrap().iter().filter(|message| message["content"] == "Summarise the build").count();
    assert_eq!(said, 1, "the inbound ledger makes a retry free: {again}");
    // The prompt carries the turn's context blocks after the owner's text, so match the text, not the whole prompt.
    let prompts = adapter.prompts.lock().unwrap().clone();
    assert_eq!(prompts.iter().filter(|prompt| prompt.starts_with("Summarise the build")).count(), 1, "{prompts:?}");

    // A stranger's message after pairing goes nowhere.
    fake.queue(private_update(3, 43, 777, "Give me the keys"));
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let after = st.db.session_detail(&session).unwrap();
    assert!(!after["messages"].as_array().unwrap().iter().any(|message| message["content"] == "Give me the keys"), "{after}");

    dispatch(&st, "channel.disconnect", json!({"kind": "telegram"})).await.unwrap();
    forget_telegram_base();
    let _ = std::fs::remove_dir_all(data_dir);
}

#[tokio::test]
async fn an_approval_card_arrives_with_allow_and_deny_and_the_button_answers_it() {
    let _turn = channel_test_lock().await;
    let fake = fake_telegram(json!({"id": 10, "is_bot": true, "username": "retinue_test_bot"})).await;
    point_telegram_at(&fake.base);
    let (st, _adapter, data_dir) = channel_state().await;
    let connected = dispatch(&st, "channel.telegram.connect", json!({"botToken": "10:token"})).await.unwrap();
    let code = connected["pairing"]["code"].as_str().unwrap().to_owned();
    let api = channels::Telegram::new(&fake.base, "10:token").unwrap();
    channels::handle_update(&st, &api, &private_update(1, 1, 555, &format!("/start {code}"))).await;
    dispatch(&st, "channel.pairing.answer", json!({"kind": "telegram", "approve": true})).await.unwrap();
    // One message, so the conversation exists and its provider session is live.
    fake.queue(private_update(2, 60, 555, "Start the tests"));
    assert!(channel_wait(|| st.db.channel_get("telegram").unwrap().unwrap()["sessionId"].is_string()).await);
    let session = st.db.channel_get("telegram").unwrap().unwrap()["sessionId"].as_str().unwrap().to_owned();
    wait_for_messages(&st, &session, |messages| messages.iter().any(|message| message["role"] == "assistant")).await;

    st.db.insert_permission("channel-card", &session, "provider-card-1", "Run the test suite", Some("npm test in C:\\work\\app"), &["allow".to_string(), "deny".to_string()], &json!({}), &json!({"kind": "command"})).unwrap();
    st.emit("permission.requested", json!({"id": "channel-card", "sessionId": session, "title": "Run the test suite", "detail": "npm test in C:\\work\\app", "choices": ["allow", "deny"], "status": "pending", "kind": "command"})).await;

    assert!(channel_wait(|| fake.sent().iter().any(|body| body["reply_markup"]["inline_keyboard"].is_array())).await, "{:?}", fake.sent());
    let card = fake.sent().into_iter().find(|body| body["reply_markup"]["inline_keyboard"].is_array()).unwrap();
    let text = card["text"].as_str().unwrap();
    assert!(text.contains("Run the test suite"), "{text}");
    assert!(!text.contains("C:\\work\\app"), "a local path never reaches the chat: {text}");
    let buttons = card["reply_markup"]["inline_keyboard"][0].as_array().unwrap().clone();
    assert_eq!(buttons.len(), 2);
    assert_eq!(buttons[0]["text"], "Allow");
    assert_eq!(buttons[1]["text"], "Deny");
    let allow = buttons[0]["callback_data"].as_str().unwrap().to_owned();
    assert!(allow.ends_with(":allow") && allow.len() <= 64, "Telegram's callback_data limit: {allow}");

    // The button answers the same card the desktop would.
    fake.queue(json!({
        "update_id": 3,
        "callback_query": {
            "id": "cb-1",
            "from": {"id": 555, "is_bot": false, "first_name": "Vex", "username": "vex"},
            "message": {"message_id": 61, "chat": {"id": 555, "type": "private"}},
            "data": allow,
        }
    }));
    assert!(channel_wait(|| st.db.permission_status("channel-card").unwrap().as_deref() == Some("resolved")).await, "the card is answered");
    assert!(channel_wait(|| fake.state.answered.lock().unwrap().iter().any(|body| body["text"] == "Allowed.")).await, "{:?}", fake.state.answered.lock().unwrap());

    // The same button cannot be used twice.
    let second = json!({"update_id": 4, "callback_query": {"id": "cb-2", "from": {"id": 555, "is_bot": false}, "message": {"message_id": 62, "chat": {"id": 555, "type": "private"}}, "data": allow}});
    fake.queue(second);
    assert!(channel_wait(|| fake.state.answered.lock().unwrap().iter().any(|body| body["text"].as_str().unwrap_or("").contains("expired"))).await);

    dispatch(&st, "channel.disconnect", json!({"kind": "telegram"})).await.unwrap();
    forget_telegram_base();
    let _ = std::fs::remove_dir_all(data_dir);
}

#[tokio::test]
async fn only_the_ceo_s_own_tool_cards_reach_the_chat() {
    let _turn = channel_test_lock().await;
    let fake = fake_telegram(json!({"id": 11, "is_bot": true, "username": "retinue_test_bot"})).await;
    point_telegram_at(&fake.base);
    let (st, _adapter, data_dir) = channel_state().await;
    let connected = dispatch(&st, "channel.telegram.connect", json!({"botToken": "11:token"})).await.unwrap();
    let code = connected["pairing"]["code"].as_str().unwrap().to_owned();
    let api = channels::Telegram::new(&fake.base, "11:token").unwrap();
    channels::handle_update(&st, &api, &private_update(1, 1, 555, &format!("/start {code}"))).await;
    dispatch(&st, "channel.pairing.answer", json!({"kind": "telegram", "approve": true})).await.unwrap();
    fake.queue(private_update(2, 70, 555, "Plan the week"));
    assert!(channel_wait(|| st.db.channel_get("telegram").unwrap().unwrap()["sessionId"].is_string()).await);
    let session = st.db.channel_get("telegram").unwrap().unwrap()["sessionId"].as_str().unwrap().to_owned();
    wait_for_messages(&st, &session, |messages| messages.iter().any(|message| message["role"] == "assistant")).await;

    // A proposal, a question and the owner's computer consent keep their cards
    // in the app, where the detail they need is shown.
    for (id, kind) in [("card-proposal", "proposal"), ("card-question", "question"), ("card-owner", "owner_computer")] {
        st.db.insert_permission(id, &session, "provider-x", &format!("{kind} card"), None, &["allow".to_string(), "deny".to_string()], &json!({}), &json!({"kind": kind})).unwrap();
        st.emit("permission.requested", json!({"id": id, "sessionId": session, "title": format!("{kind} card"), "choices": ["allow", "deny"], "status": "pending"})).await;
    }
    // And a card about another conversation's work stays there too.
    st.emit("permission.requested", json!({"id": "elsewhere", "sessionId": "another-conversation", "title": "Elsewhere", "choices": ["allow", "deny"], "status": "pending"})).await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let sent = fake.sent_texts();
    for kind in ["proposal", "question", "owner_computer"] {
        assert!(!sent.iter().any(|text| text.contains(&format!("{kind} card"))), "{kind} reached the chat: {sent:?}");
    }
    assert!(!sent.iter().any(|text| text.contains("Elsewhere")), "{sent:?}");

    // Turning approval buttons off stops even the ordinary card.
    dispatch(&st, "channel.update", json!({"kind": "telegram", "approvals": false})).await.unwrap();
    st.db.insert_permission("card-command", &session, "provider-y", "Ordinary command", None, &["allow".to_string(), "deny".to_string()], &json!({}), &json!({"kind": "command"})).unwrap();
    st.emit("permission.requested", json!({"id": "card-command", "sessionId": session, "title": "Ordinary command", "choices": ["allow", "deny"], "status": "pending"})).await;
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(!fake.sent_texts().iter().any(|text| text.contains("Ordinary command")), "buttons are off");

    dispatch(&st, "channel.disconnect", json!({"kind": "telegram"})).await.unwrap();
    forget_telegram_base();
    let _ = std::fs::remove_dir_all(data_dir);
}

#[tokio::test]
async fn whatsapp_cloud_connects_and_sends_the_outbound_half() {
    let _turn = channel_test_lock().await;
    async fn graph(axum::extract::State(captured): axum::extract::State<Captured>, request: axum::extract::Request) -> axum::Json<Value> {
        let authorization = request.headers().get("authorization").and_then(|value| value.to_str().ok()).unwrap_or("").to_owned();
        let path = request.uri().path().to_owned();
        let body: Value = axum::body::to_bytes(request.into_body(), 1 << 20).await.ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
        captured.lock().unwrap().push((format!("{path} {authorization}"), body.clone()));
        if path.ends_with("/messages") { axum::Json(json!({"messages": [{"id": "wamid.1"}]})) }
        else { axum::Json(json!({"display_phone_number": "+44 7700 900123", "verified_name": "Retinue"})) }
    }
    type Captured = Arc<std::sync::Mutex<Vec<(String, Value)>>>;
    let captured: Captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let app = axum::Router::new().fallback(graph).with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { let _ = axum::serve(listener, app).await; });
    point_cloud_at(&base);
    let (st, _, data_dir) = channel_state().await;

    let connected = dispatch(&st, "channel.whatsappCloud.connect", json!({
        "phoneNumberId": "109876543210", "accessToken": "EAAG-test-token", "appSecret": "0123456789abcdef0123456789abcdef"
    })).await.unwrap();
    assert_eq!(connected["channel"]["account"], "+44 •••• 0123", "the number is shown masked");
    assert_eq!(connected["channel"]["status"], "pairing");
    assert!(connected["webhook"]["url"].is_null(), "no public address exists, so there is no URL to give Meta");
    let verify = connected["webhook"]["verifyToken"].as_str().unwrap();
    assert_eq!(verify.len(), 32);
    let first = captured.lock().unwrap()[0].clone();
    assert!(first.0.contains("109876543210"), "{}", first.0);
    assert_eq!(first.0.split_once(' ').map(|(_, auth)| auth), Some("Bearer EAAG-test-token"));

    // The outbound half: one text message, the shape Meta documents.
    let id = channels::cloud_send_text(&base, "109876543210", "EAAG-test-token", "447700900999", "Retinue says hello.").await.unwrap();
    assert_eq!(id, "wamid.1");
    let sent = captured.lock().unwrap().last().cloned().unwrap();
    assert_eq!(sent.1["messaging_product"], "whatsapp");
    assert_eq!(sent.1["to"], "447700900999");
    assert_eq!(sent.1["type"], "text");
    assert_eq!(sent.1["text"]["body"], "Retinue says hello.");

    // No sender is paired, so a test send says exactly that.
    let refused = dispatch(&st, "channel.test", json!({"kind": "whatsapp-cloud"})).await.unwrap_err();
    assert_eq!(refused.1.code, "conflict");
    assert!(refused.1.message.contains("No sender is paired"), "{}", refused.1.message);

    dispatch(&st, "channel.disconnect", json!({"kind": "whatsapp-cloud"})).await.unwrap();
    for field in ["accessToken", "appSecret", "verifyToken", "phoneNumberId"] {
        assert!(channels::read_secret("whatsapp-cloud", field).is_none(), "{field} survived a disconnect");
    }
    forget_cloud_base();
    let _ = std::fs::remove_dir_all(data_dir);
}
