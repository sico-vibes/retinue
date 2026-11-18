// Wave 3, release safety (`docs/plans/nod-adoption-plan.md`): the bounded
// conversation read.
//
// `session.get` gained an optional `beforeMessageId` (idea 8). The window a
// paired device receives is applied where a phone's reply is redacted
// (`remote::policy::window_session`, pinned by the shared goldens); the paging
// itself is dispatch's, because it narrows what is read before redaction. The
// desktop never sends the cursor, so its read is the whole conversation, and
// that is what the first test proves.

#[tokio::test]
async fn session_get_pages_backwards_from_a_cursor_and_leaves_the_desktop_read_whole() {
    let (st, _) = state();
    st.db.create_session("page-rpc", "rt-test", "codex", ".", "Long one").unwrap();
    st.db.create_turn("page-rpc-turn", "page-rpc").unwrap();
    let mut ids = Vec::new();
    for index in 0..5 {
        let message = st.db.append_message("page-rpc", "page-rpc-turn", "user", &format!("message {index}")).unwrap();
        ids.push(message["id"].as_str().unwrap().to_owned());
    }

    // No cursor: the whole conversation, exactly as before, with no page markers.
    let whole = dispatch(&st, "session.get", json!({"sessionId": "page-rpc"})).await.unwrap();
    assert_eq!(whole["messages"].as_array().unwrap().len(), 5);
    assert!(whole.get("messagesTruncated").is_none(), "the desktop read carries no page marker: {whole}");
    assert!(whole["turns"].as_array().is_some(), "the desktop read still carries its turns: {whole}");

    // The page before a message: its messages only, and the working detail is
    // left on the newest page with markers saying so.
    let page = dispatch(&st, "session.get", json!({"sessionId": "page-rpc", "beforeMessageId": ids[3]})).await.unwrap();
    assert_eq!(page["messages"].as_array().unwrap().len(), 3, "{page}");
    assert_eq!(page["messages"][2]["id"], json!(ids[2]));
    assert!(page["messages"].as_array().unwrap().iter().all(|message| message["id"] != json!(ids[3])));
    assert_eq!(page["turnsTruncated"], json!(true));
    assert_eq!(page["toolsTruncated"], json!(true));
    assert_eq!(page["filesTruncated"], json!(true));
    assert!(page["turns"].as_array().unwrap().is_empty());

    // The window is then applied on the way to a device, and its cursor is what
    // the phone sends back: the two together reach every message.
    let windowed = crate::remote::policy::redact_result(
        "session.get",
        &dispatch(&st, "session.get", json!({"sessionId": "page-rpc"})).await.unwrap(),
        &crate::remote::policy::RedactOptions::default(),
    );
    assert!(windowed["messages"].as_array().is_some(), "a device gets the narrowed session: {windowed}");

    // A cursor that is not in the conversation, and one that is not an id, are refused.
    let missing = dispatch(&st, "session.get", json!({"sessionId": "page-rpc", "beforeMessageId": "not-a-message"})).await.unwrap_err();
    assert_eq!(missing.1.code, "invalid_argument", "{missing:?}");
    let long = dispatch(&st, "session.get", json!({"sessionId": "page-rpc", "beforeMessageId": "x".repeat(200)})).await.unwrap_err();
    assert_eq!(long.1.code, "invalid_argument", "{long:?}");
    // An unknown conversation is still `not_found`, cursor or not.
    let unknown = dispatch(&st, "session.get", json!({"sessionId": "no-such-conversation", "beforeMessageId": ids[1]})).await.unwrap_err();
    assert_eq!(unknown.1.code, "not_found", "{unknown:?}");
}
