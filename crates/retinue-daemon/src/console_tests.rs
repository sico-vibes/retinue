// 17 November 2026 (multi-device piece 8): the console of a host with no window.
//
// Every method is called through `main_rpc`, which is where the token is checked
// and the route is taken: calling `console::route` alone would prove the body but
// not that the app can reach it (wave 2's `task.checkBack` lesson).

use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

/// One console call through the real router, returning the status and the body.
async fn console_post(st: &AppState, bearer: Option<&str>, method: &str, params: Value) -> (u16, Value) {
    let mut headers = HeaderMap::new();
    if let Some(bearer) = bearer {
        headers.insert(header::AUTHORIZATION, format!("Bearer {bearer}").parse().unwrap());
    }
    let request = RpcRequest { v: 1, id: "console-test".into(), method: method.into(), params };
    let reply = crate::main_rpc(State(st.clone()), ConnectInfo("127.0.0.1:5555".parse().unwrap()), headers, Json(request)).await.into_response();
    let status = reply.status().as_u16();
    let body = axum::body::to_bytes(reply.into_body(), 1 << 20).await.unwrap_or_else(|_| Bytes::new());
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

/// A state whose console is live, as a headless start makes it. No file is
/// written: the daemon's own `console::startup` is exercised by the live host.
fn console_state() -> (AppState, String) {
    let (st, _) = state();
    let token = "console-token-for-tests".to_string();
    st.console.start_in_memory(&token);
    (st, token)
}

#[tokio::test]
async fn console_methods_need_the_console_token_and_never_the_capability() {
    let (st, token) = console_state();
    assert!(st.console.is_live());
    assert_eq!(console_post(&st, None, "console.status", json!({})).await.0, 401, "no token");
    assert_eq!(console_post(&st, Some("not-the-token"), "console.status", json!({})).await.0, 401, "a wrong token");
    assert_eq!(console_post(&st, Some(st.token.as_str()), "console.status", json!({})).await.0, 401, "the daemon capability is not the console token");
    assert_eq!(console_post(&st, Some(&token), "console.status", json!({})).await.0, 200, "the console token");
    assert_eq!(console_post(&st, Some(&token), "console.nope", json!({})).await.0, 404, "only the console methods");
    // A desktop-launched daemon has no console at all: the same call is refused.
    let (plain, _) = state();
    assert!(!plain.console.is_live());
    assert_eq!(console_post(&plain, Some(&token), "console.status", json!({})).await.0, 401, "nothing accepts a console token without a headless start");
}

#[tokio::test]
async fn console_status_reports_the_folder_the_store_and_the_gateway_without_a_secret() {
    let (st, token) = console_state();
    let dir = std::env::temp_dir().join(format!("retinue-console-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    let (status, body) = console_post(&st, Some(&token), "console.status", json!({})).await;
    assert_eq!(status, 200);
    let result = &body["result"];
    assert_eq!(result["dataFolder"].as_str().unwrap(), dir.display().to_string());
    assert!(result["database"].as_str().unwrap().ends_with("retinue.db"));
    assert_eq!(result["secrets"]["backend"].as_str().unwrap(), crate::secrets::store::backend_name());
    assert!(result["secrets"]["folder"].as_str().unwrap().ends_with("secrets"));
    assert_eq!(result["remote"]["enabled"], json!(false), "Remote starts off");
    assert_eq!(result["platform"].as_str().unwrap(), std::env::consts::OS);
    assert!(result["version"].as_str().is_some());
    // Nothing in the reply names a secret or the capability.
    let text = body.to_string();
    assert!(!text.contains(st.token.as_str()), "the daemon capability never reaches a console reply");
    assert!(!text.contains(&token), "the console token is not echoed back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_waiting_request_is_listed_and_confirmed_with_a_tier_from_the_console() {
    let (st, token) = console_state();
    // Nothing waiting yet.
    let (status, body) = console_post(&st, Some(&token), "console.pair.pending", json!({})).await;
    assert_eq!(status, 200);
    assert!(body["result"]["pending"].is_null());
    assert_eq!(
        console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true, "tier": "chat"})).await.0,
        409,
        "approving nothing is a conflict, not a silent success"
    );
    // A Native sign-in waiting for its tier, as the gateway makes one.
    st.remote.pairing.lock().unwrap().begin_native("LAPTOP-7", "machine", "linux", chrono::Utc::now());
    let (status, body) = console_post(&st, Some(&token), "console.pair.pending", json!({})).await;
    assert_eq!(status, 200);
    let pending = &body["result"]["pending"];
    assert_eq!(pending["name"], json!("LAPTOP-7"));
    assert_eq!(pending["kind"], json!("machine"));
    assert_eq!(pending["platform"], json!("linux"));
    assert_eq!(pending["method"], json!("native"));
    assert!(pending["pairingId"].as_str().is_some_and(|id| !id.is_empty()));
    // Approving without a tier is refused with the window's own sentence.
    let (status, body) = console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true})).await;
    assert_eq!(status, 400);
    assert!(body["error"]["message"].as_str().unwrap().contains("Choose what this device may do"));
    // With a tier it is approved, the device is stored, and the ledger says so.
    let (status, body) = console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true, "tier": "chat"})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["approved"], json!(true));
    let device_id = body["result"]["device"]["id"].as_str().unwrap().to_owned();
    assert_eq!(body["result"]["device"]["tier"], json!("chat"));
    assert_eq!(body["result"]["device"]["platform"], json!("linux"), "the request keeps the platform it reported");
    let stored = st.db.remote_device_get(&device_id).unwrap().expect("the device is stored");
    assert_eq!(stored["tier"], json!("chat"));
    let ledger = st.db.remote_activity_list(20).unwrap();
    assert!(ledger.iter().any(|entry| entry["kind"] == json!("paired")), "the approval is in the activity list");
    // The offer is spent: a second approval is refused, and nothing is waiting.
    assert_eq!(console_post(&st, Some(&token), "console.pair.confirm", json!({"approve": true, "tier": "chat"})).await.0, 409);
    let (_, body) = console_post(&st, Some(&token), "console.pair.pending", json!({})).await;
    assert!(body["result"]["pending"].is_null(), "a decided request is not offered again");
}

#[tokio::test]
async fn the_console_refuses_denies_and_cancels_with_the_window_s_own_sentences() {
    let (st, token) = console_state();
    st.remote.pairing.lock().unwrap().begin_native("PHONE", "phone", "android", chrono::Utc::now());
    let (status, body) = console_post(&st, Some(&token), "console.pair.deny", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["approved"], json!(false));
    assert_eq!(st.db.remote_devices().unwrap().len(), 0, "a refusal stores nothing");
    // Cancelling after a decision is what the window's own `remote.pair.cancel`
    // answers (the offer object is still there, so it says it cancelled), and the
    // console does not invent a third answer. "Nothing to cancel" is a state with
    // no offer at all, below.
    let (status, body) = console_post(&st, Some(&token), "console.pair.cancel", json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["result"]["cancelled"], json!(true), "the decided offer is thrown away the same way the window does it");
    let (fresh, fresh_token) = console_state();
    let (status, body) = console_post(&fresh, Some(&fresh_token), "console.pair.cancel", json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(body["result"]["cancelled"], json!(false), "with no offer at all there is nothing to cancel");
    // Pairing a code needs Remote on, and the sentence is the window's.
    let (status, body) = console_post(&st, Some(&token), "console.pair.begin", json!({})).await;
    assert_eq!(status, 409);
    assert!(body["error"]["message"].as_str().unwrap().contains("Turn Remote on before pairing a device"));
    // Turning Remote on is a console decision too, and a bad method is refused.
    let (status, _) = console_post(&st, Some(&token), "console.remote.enable", json!({"method": "carrier-pigeon"})).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn a_live_offer_expires_out_of_the_console_list() {
    // The console shows only what can still be approved: an expired request is
    // not listed, because approving it would only be refused.
    let mut pairing = crate::remote::pairing::PairingState::default();
    let past = chrono::Utc::now() - chrono::Duration::minutes(10);
    pairing.begin_native("LATE", "machine", "linux", past);
    assert!(pairing.live_pending().is_none(), "an expired request is not offered");
    pairing.begin_native("NOW", "machine", "linux", chrono::Utc::now());
    assert_eq!(pairing.live_pending().unwrap()["name"], json!("NOW"));
}
