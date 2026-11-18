//! 17 November 2026: a paired phone working through its host's machine links.
//!
//! The phone pairs with one host and reaches the machines that host is linked to, at the
//! lower of the phone's tier and the link's tier. Two entry points are proven here:
//! `dispatch` with the gateway's own tier stamp (what a phone's call becomes once the
//! gateway has decided it), and the real gateway over HTTP and WebSocket as a seeded
//! device, so the kind check and the event relay are exercised where a phone meets them.
//!
//! The far machine is the raw TLS fake from `machines_tests`; nothing here touches a real
//! second machine, and nothing about another machine is stored.

use crate::machines_tests::{far_gateway, linked_to, local, ok_body, raw_server, rpc_calls, sent, FAR_ID};
use crate::remote::pairing::DEVICE_COOKIE;
use crate::{dispatch, AppState};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message as ClientMessage;

/// The parameter the gateway stamps a phone's own tier into; the daemon's own name for it.
const STAMP: &str = "remoteTier";

/// A call as the phone's gateway makes it: the caller's parameters plus the tier stamp.
fn as_phone(params: Value, tier: &str) -> Value {
    let mut params = params;
    params[STAMP] = json!(tier);
    params
}

/// Enables the gateway on a state and answers the origin it accepts, so a seeded device
/// can call it the way a phone would.
async fn gateway_origin(st: &AppState) -> String {
    let fake = Arc::new(crate::remote::tailscale::tests::FakeCli::new(
        crate::remote::tailscale::tests::RUNNING_STATUS,
        crate::remote::tailscale::tests::EMPTY_SERVE,
    ));
    st.remote.set_cli(fake);
    st.db.set_setting("remote.port", &json!(0)).unwrap();
    crate::remote::enable(st).await.unwrap();
    st.remote.serve_origin().expect("the fake tailnet's origin")
}

/// One paired device of a given kind and tier, returning its cookie token.
fn seed_device(st: &AppState, id: &str, name: &str, kind: &str, tier: &str) -> String {
    let token = crate::remote::pairing::generate_token();
    let hash = crate::remote::pairing::hash_token(&token);
    let device = json!({
        "id": id,
        "name": name,
        "kind": kind,
        "platform": "test",
        "tier": tier,
        "login": null,
        "pairedAt": crate::remote::now_rfc3339(),
    });
    st.db.remote_device_create(&device, &hash).unwrap();
    token
}

/// One `/api/rpc` call as a paired device, with the `(status, body)` it answered.
async fn device_rpc(port: u16, origin: &str, token: &str, method: &str, params: Value) -> (u16, Value) {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let response = client
        .post(format!("http://127.0.0.1:{port}/api/rpc"))
        .header("Origin", origin)
        .header("Cookie", format!("{DEVICE_COOKIE}={token}"))
        .header("Content-Type", "application/json")
        .json(&json!({"v": 1, "id": "req_1", "method": method, "params": params}))
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    (status, body)
}

// ---------------------------------------------------------------------------
// The two ceilings

#[tokio::test]
async fn a_phone_works_at_the_lower_of_its_tier_and_the_link_tier() {
    let server = raw_server(far_gateway(|method, _| match method {
        "agent.conversation.create" => ok_body(json!({"session": {"id": "far-new", "agentId": "far-bot"}})),
        "session.prompt" => ok_body(json!({"turnId": "t1", "accepted": true, "state": "working"})),
        _ => ok_body(json!({"ok": true})),
    }))
    .await;
    let (me, dir) = local("phone-ceiling");
    linked_to(&me, &server, "chat").await;

    // A phone at operator is still capped by a chat link: the work goes, and no more.
    let started = dispatch(&me, "machine.start", as_phone(json!({"machineId": FAR_ID, "agentId": "far-bot"}), "operator")).await.unwrap();
    assert_eq!(started, json!({"sessionId": "far-new", "machineId": FAR_ID}));
    let prompted = dispatch(&me, "machine.prompt", as_phone(json!({"machineId": FAR_ID, "sessionId": "s", "text": "go on"}), "operator")).await.unwrap();
    assert_eq!(prompted, json!({"turnId": "t1", "accepted": true, "state": "working"}));

    // A phone at viewer is capped by itself, whatever the link allows. Nothing is sent.
    let before = rpc_calls(&server, 0).len();
    let refused = dispatch(&me, "machine.prompt", as_phone(json!({"machineId": FAR_ID, "sessionId": "s", "text": "hi"}), "viewer")).await.unwrap_err();
    assert_eq!(refused.1.code, "tier_required");
    assert_eq!(
        refused.1.message,
        "Sending a message on Far needs Chat there. This device is Viewer and this computer is linked as Chat, so it works as Viewer. Change the tier in Far's Remote settings."
    );
    assert_eq!(rpc_calls(&server, 0).len(), before, "a refused call sends nothing");

    // Reading is allowed at viewer, so the same phone may see the machines and their bots.
    let listed = dispatch(&me, "machine.list", as_phone(json!({}), "viewer")).await.unwrap();
    assert_eq!(listed["machines"][0]["name"], "Far");

    // An approval needs operator on both: a chat link refuses it even for an operator phone.
    let refused = dispatch(&me, "machine.permission.reply", as_phone(json!({"machineId": FAR_ID, "permissionId": "p", "choice": "allow"}), "operator")).await.unwrap_err();
    assert_eq!(refused.1.code, "tier_required");
    assert!(
        refused.1.message.contains("This device is Operator") && refused.1.message.contains("linked as Chat"),
        "{}",
        refused.1.message
    );
    // With both at operator the far machine answers, and its own switch decides.
    me.db.machine_link_set_tier(FAR_ID, "operator").unwrap();
    let answered = dispatch(&me, "machine.permission.reply", as_phone(json!({"machineId": FAR_ID, "permissionId": "p", "choice": "allow"}), "operator")).await.unwrap();
    assert_eq!(answered["ok"], json!(true));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_chat_phone_and_an_operator_link_still_cannot_approve() {
    let server = raw_server(far_gateway(|_, _| ok_body(json!({"ok": true})))).await;
    let (me, dir) = local("phone-approve-chat");
    linked_to(&me, &server, "operator").await;
    let refused = dispatch(&me, "machine.permission.reply", as_phone(json!({"machineId": FAR_ID, "permissionId": "p", "choice": "allow"}), "chat")).await.unwrap_err();
    assert_eq!(refused.1.code, "tier_required");
    assert!(
        refused.1.message.contains("needs Operator there") && refused.1.message.contains("linked as Operator"),
        "{}",
        refused.1.message
    );
    assert!(rpc_calls(&server, 0).is_empty(), "nothing was asked of the far side");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_window_keeps_the_links_own_tier_and_its_own_sentence() {
    let server = raw_server(far_gateway(|_, _| ok_body(json!({"accepted": true})))).await;
    let (me, dir) = local("window-tier");
    linked_to(&me, &server, "viewer").await;
    // No stamp: the desktop, which is the owner and has no tier of its own.
    let refused = dispatch(&me, "machine.prompt", json!({"machineId": FAR_ID, "sessionId": "s", "text": "hi"})).await.unwrap_err();
    assert_eq!(refused.1.code, "tier_required");
    assert_eq!(
        refused.1.message,
        "Sending a message on Far needs Chat there, and this computer is linked as Viewer. Change this computer's tier in Far's Remote settings."
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_tier_stamp_is_the_gateways_and_never_reaches_the_other_machine() {
    let server = raw_server(far_gateway(|_, _| ok_body(json!({"accepted": true})))).await;
    let (me, dir) = local("stamp-strip");
    linked_to(&me, &server, "chat").await;
    dispatch(&me, "machine.prompt", as_phone(json!({"machineId": FAR_ID, "sessionId": "s", "text": "hi"}), "chat")).await.unwrap();
    // Exactly the caller's own parameters: no machine id, and no stamp for the far side to read.
    assert_eq!(sent(&server, 0), ("session.prompt".to_string(), json!({"sessionId": "s", "text": "hi"})));
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// What a phone may see of a machine

#[tokio::test]
async fn a_phone_listing_the_machines_gets_names_without_the_address_or_fingerprint() {
    let server = raw_server(far_gateway(|_, _| ok_body(json!({"ok": true})))).await;
    let (me, dir) = local("phone-list");
    linked_to(&me, &server, "chat").await;
    // The desktop still reads the row it needs to manage the link.
    let desk = dispatch(&me, "machine.list", json!({})).await.unwrap();
    for key in ["url", "fingerprint", "auth", "pairedAt"] {
        assert!(desk["machines"][0].get(key).is_some(), "the window keeps {key}");
    }
    // A phone gets the machine, its state and its bots, and none of those four.
    let phone = dispatch(&me, "machine.list", as_phone(json!({}), "viewer")).await.unwrap();
    let row = &phone["machines"][0];
    for key in ["url", "fingerprint", "auth", "pairedAt"] {
        assert!(row.get(key).is_none(), "a phone never sees {key}: {row}");
    }
    assert_eq!(row["id"], json!(FAR_ID));
    assert_eq!(row["name"], json!("Far"));
    assert_eq!(row["state"], json!("online"));
    assert_eq!(row["tier"], json!("chat"), "the phone has to know the ceiling it works under");
    assert!(row["bots"].is_array(), "{row}");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// The gateway: which device may ask, and what it is told

#[tokio::test]
async fn the_gateway_refuses_the_phone_surface_to_a_machine_and_the_linked_computer() {
    let server = raw_server(far_gateway(|_, _| ok_body(json!({"ok": true})))).await;
    let (me, dir) = local("gateway-kinds");
    linked_to(&me, &server, "operator").await;
    let origin = gateway_origin(&me).await;
    let port = me.remote.bound_address().expect("bound").port();
    let machine = seed_device(&me, "dev-machine", "Laptop", "machine", "operator");
    let computer = seed_device(&me, "dev-computer", "Harbor", "computer", "operator");
    for token in [&machine, &computer] {
        for (method, params) in [
            ("machine.list", json!({})),
            ("machine.prompt", json!({"machineId": FAR_ID, "sessionId": "s", "text": "hi"})),
            ("machine.start", json!({"machineId": FAR_ID, "agentId": "far-bot"})),
        ] {
            let (status, body) = device_rpc(port, &origin, token, method, params).await;
            assert_eq!(status, 403, "{method}: {body}");
            assert_eq!(body["error"]["code"], json!("forbidden_remote"), "{method}: {body}");
            assert_eq!(body["error"]["message"], json!("Only a paired phone, tablet or browser can do this through its host."), "{method}");
        }
    }
    assert!(rpc_calls(&server, 0).is_empty(), "nothing reaches the far side through a refused device");
    // Moving a conversation stays the other way round: a phone cannot, a machine can.
    let phone = seed_device(&me, "dev-phone", "Pixel", "phone", "operator");
    let (status, body) = device_rpc(port, &origin, &phone, "handoff.preview", json!({"sessionId": "s"})).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["error"]["message"], json!("Only a linked Retinue machine can move a conversation."));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_phone_reaches_its_hosts_links_through_the_gateway() {
    let server = raw_server(far_gateway(|method, _| match method {
        "agent.conversation.create" => ok_body(json!({"session": {"id": "far-new", "agentId": "far-bot"}})),
        "session.get" => ok_body(json!({"id": "far-s1", "title": "Invoice parser", "agentId": "far-bot"})),
        _ => ok_body(json!({"accepted": true, "state": "working"})),
    }))
    .await;
    let (me, dir) = local("gateway-phone");
    linked_to(&me, &server, "chat").await;
    let origin = gateway_origin(&me).await;
    let port = me.remote.bound_address().expect("bound").port();
    let pixel = seed_device(&me, "dev-pixel", "Pixel", "phone", "operator");
    let viewer = seed_device(&me, "dev-viewer", "Tablet", "tablet", "viewer");

    // The phone's own screen: the host's links, with no address on them.
    let (status, body) = device_rpc(port, &origin, &pixel, "machine.list", json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["machines"][0]["name"], json!("Far"));
    assert!(body["result"]["machines"][0].get("url").is_none(), "{body}");

    // Work on another machine, at the lower of the two tiers.
    let (status, body) = device_rpc(port, &origin, &pixel, "machine.start", json!({"machineId": FAR_ID, "agentId": "far-bot"})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"], json!({"sessionId": "far-new", "machineId": FAR_ID}));
    let (status, body) = device_rpc(port, &origin, &pixel, "machine.prompt", json!({"machineId": FAR_ID, "sessionId": "far-s1", "text": "go on"})).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = device_rpc(port, &origin, &pixel, "machine.steer", json!({"machineId": FAR_ID, "sessionId": "far-s1", "text": "faster"})).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = device_rpc(port, &origin, &pixel, "machine.cancel", json!({"machineId": FAR_ID, "sessionId": "far-s1"})).await;
    assert_eq!(status, 200, "{body}");
    // One conversation, read through the link.
    let (status, body) = device_rpc(port, &origin, &pixel, "machine.conversation", json!({"machineId": FAR_ID, "sessionId": "far-s1"})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["result"]["session"]["title"], json!("Invoice parser"));

    // The host's own activity list says a phone prompted a machine, in two content-free words.
    let activity = dispatch(&me, "remote.activity.list", json!({})).await.unwrap();
    let entries = activity["entries"].as_array().cloned().unwrap_or_default();
    let line = entries
        .iter()
        .find(|entry| entry["summary"].as_str().is_some_and(|summary| summary.contains("machine.prompt")))
        .unwrap_or_else(|| panic!("no activity line for a prompt through a link: {activity}"));
    assert_eq!(line["kind"], json!("prompt"));
    assert_eq!(line["deviceName"], json!("Pixel"));

    // The stamp is the gateway's: a client's own value is replaced, in both directions.
    let mut lower = json!({"machineId": FAR_ID, "sessionId": "far-s1", "text": "hi"});
    lower[STAMP] = json!("viewer");
    let (status, body) = device_rpc(port, &origin, &pixel, "machine.prompt", lower).await;
    assert_eq!(status, 200, "the phone's own claim of viewer must not cap it below its real tier: {body}");
    let mut higher = json!({"machineId": FAR_ID, "sessionId": "far-s1", "text": "hi"});
    higher[STAMP] = json!("operator");
    let (status, body) = device_rpc(port, &origin, &viewer, "machine.prompt", higher).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["error"]["code"], json!("tier_required"), "a viewer phone cannot lift itself with a parameter");

    // A viewer may still look.
    let (status, body) = device_rpc(port, &origin, &viewer, "machine.list", json!({})).await;
    assert_eq!(status, 200, "{body}");

    // Everything about links stays the desktop's, and everything about devices stays refused.
    for (method, params) in [
        ("machine.pair.begin", json!({"url": "https://far.example", "code": "A", "confirm": true})),
        ("machine.rename", json!({"id": FAR_ID, "name": "Mine"})),
        ("machine.remove", json!({"id": FAR_ID, "confirm": true})),
        ("machine.takeOver", json!({"sessionId": "s", "machineId": FAR_ID})),
        ("machine.projects", json!({"machineId": FAR_ID})),
        ("remote.status", json!({})),
        ("remote.devices.list", json!({})),
        ("settings.set", json!({"key": "remote.enabled", "value": true})),
    ] {
        let (status, body) = device_rpc(port, &origin, &pixel, method, params).await;
        assert_eq!(status, 403, "{method}: {body}");
        assert_eq!(body["error"]["code"], json!("forbidden_remote"), "{method}: {body}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// The relay

/// A device's live socket, after its cursor, as the frames it is sent.
type DeviceSocket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn open_socket(port: u16, origin: &str, token: &str) -> DeviceSocket {
    let mut request = format!("ws://127.0.0.1:{port}/api/events").into_client_request().unwrap();
    request.headers_mut().insert("Origin", origin.parse().unwrap());
    request.headers_mut().insert("Cookie", format!("{DEVICE_COOKIE}={token}").parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.expect("the upgrade succeeds");
    socket.send(ClientMessage::Text(json!({"v": 1, "afterSequence": 0}).to_string().into())).await.unwrap();
    let ready = next_frame(&mut socket).await;
    assert_eq!(ready["type"], json!("events.replay.complete"), "{ready}");
    socket
}

async fn next_frame(socket: &mut DeviceSocket) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("a frame arrives")
            .expect("the socket is open")
            .expect("the frame is valid");
        if let ClientMessage::Text(text) = message {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

#[tokio::test]
async fn a_relays_change_reaches_a_phones_socket_and_never_a_machines() {
    let (me, dir) = local("relay");
    let origin = gateway_origin(&me).await;
    let port = me.remote.bound_address().expect("bound").port();
    let phone = seed_device(&me, "dev-phone", "Pixel", "phone", "viewer");
    let laptop = seed_device(&me, "dev-machine", "Laptop", "machine", "operator");
    let mut phone_socket = open_socket(port, &origin, &phone).await;
    let mut laptop_socket = open_socket(port, &origin, &laptop).await;

    // What the live lane emits for a linked machine's conversation, then a sentinel every
    // device already receives.
    me.emit(
        "machine.session.changed",
        json!({"machineId": "m-remote", "sessionId": "s-remote", "revision": 9}),
    )
    .await;
    me.emit("session.changed", json!({"sessionId": "sentinel"})).await;

    let relay = next_frame(&mut phone_socket).await;
    assert_eq!(relay["type"], json!("machine.session.changed"));
    assert_eq!(relay["payload"], json!({"machineId": "m-remote", "sessionId": "s-remote", "revision": 9}));
    let after = next_frame(&mut phone_socket).await;
    assert_eq!(after["type"], json!("session.changed"), "{after}");

    // The link's own daemon gets the sentinel and nothing of the relay.
    loop {
        let frame = next_frame(&mut laptop_socket).await;
        assert_ne!(frame["type"], json!("machine.session.changed"), "a machine link received a relay: {frame}");
        if frame["type"] == json!("session.changed") {
            break;
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}
