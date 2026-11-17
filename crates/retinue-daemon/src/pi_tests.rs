// Section C (16 November 2026): Pi's provider list and the keys Retinue holds for
// it. Included into `phase2a_tests`, so `state()` and `dispatch` are in scope.

async fn pi_call(st: &AppState, method: &str, p: Value) -> Result<Value, DispatchError> {
    dispatch(st, method, p).await
}

#[tokio::test]
async fn pi_providers_lists_every_provider_without_ever_returning_a_value() {
    let (st, _events) = state();
    let reply = pi_call(&st, "pi.providers", json!({})).await.unwrap();
    // The shape the Settings page is built against.
    assert!(reply["installed"].is_boolean(), "installed is a boolean: {reply}");
    assert!(reply["version"].is_null() || reply["version"].is_string());
    let providers = reply["providers"].as_array().expect("providers is an array");
    assert!(providers.len() >= 20, "the page lists every provider a key alone can set up");
    for row in providers {
        assert!(row["id"].is_string() && row["name"].is_string(), "{row}");
        assert!(row["keyEnv"].as_str().is_some_and(|env| !env.is_empty()), "{row}");
        assert!(row["hasKey"].is_boolean(), "{row}");
        // `models` is a count once the catalogue is warm, and null until then -
        // never 0 as a stand-in for unknown.
        assert!(row["models"].is_null() || row["models"].is_u64(), "{row}");
        assert!(row["docsUrl"].as_str().is_some_and(|url| url.starts_with("https://")), "{row}");
        assert!(row.get("key").is_none() && row.get("value").is_none(), "no value is ever returned: {row}");
    }
    let ids: Vec<&str> = providers.iter().filter_map(|row| row["id"].as_str()).collect();
    for expected in ["anthropic", "openai", "deepseek", "openrouter", "opencode-go", "google"] {
        assert!(ids.contains(&expected), "{expected} is offered");
    }
    // A key stored for a bot secret is not a Pi key, and the Pi page says nothing
    // about it.
    let reply_text = reply.to_string();
    assert!(!reply_text.contains("sk-"), "no key text appears in the reply: {reply_text}");
}

#[tokio::test]
async fn a_pi_key_is_stored_removed_and_never_echoed() {
    let (st, _events) = state();
    let secret = "sk-fixture-pi-deepseek-0123456789";
    let set = pi_call(&st, "pi.key.set", json!({"provider": "deepseek", "key": secret})).await.unwrap();
    assert_eq!(set, json!({"ok": true}));
    assert!(!set.to_string().contains(secret), "the reply never carries the key");
    let listed = pi_call(&st, "pi.providers", json!({})).await.unwrap();
    let row = listed["providers"].as_array().unwrap().iter().find(|row| row["id"] == "deepseek").cloned().unwrap();
    assert_eq!(row["hasKey"], true);
    assert!(!listed.to_string().contains(secret), "the list never carries the key");
    // A key that shadows another provider's variable is refused, naming both.
    assert_eq!(pi_call(&st, "pi.key.set", json!({"provider": "opencode", "key": "sk-fixture-zen"})).await.unwrap(), json!({"ok": true}));
    let conflict = pi_call(&st, "pi.key.set", json!({"provider": "opencode-go", "key": "sk-fixture-go"})).await.unwrap_err();
    assert_eq!(conflict.1.code, "conflict");
    assert!(conflict.1.message.contains("OPENCODE_API_KEY"), "{}", conflict.1.message);
    // Delete removes it, and the next set is accepted.
    assert_eq!(pi_call(&st, "pi.key.delete", json!({"provider": "opencode"})).await.unwrap(), json!({"ok": true}));
    pi_call(&st, "pi.key.set", json!({"provider": "opencode-go", "key": "sk-fixture-go"})).await.unwrap();
    // Clean up, so another test's count is not affected.
    pi_call(&st, "pi.key.delete", json!({"provider": "opencode-go"})).await.unwrap();
    pi_call(&st, "pi.key.delete", json!({"provider": "deepseek"})).await.unwrap();
    let after = pi_call(&st, "pi.providers", json!({})).await.unwrap();
    let row = after["providers"].as_array().unwrap().iter().find(|row| row["id"] == "deepseek").cloned().unwrap();
    assert_eq!(row["hasKey"], false);
}

#[tokio::test]
async fn a_pi_conversation_gets_the_stored_keys_and_no_other_provider_does() {
    let (st, _events) = state();
    let secret = "sk-fixture-injected-key-0123456789";
    pi_call(&st, "pi.key.set", json!({"provider": "deepseek", "key": secret})).await.unwrap();
    st.db.upsert_runtime(&json!({"id":"rt_pi_1","provider":"pi","status":"online","executablePath":"x","launchArgs":[]})).unwrap();
    st.db.upsert_runtime(&json!({"id":"rt_codex_1","provider":"codex","status":"online","executablePath":"x","launchArgs":[]})).unwrap();
    st.db.create_session("ses-pi-keys", "rt_pi_1", "pi", ".", "Pi chat").unwrap();
    st.db.create_session("ses-codex-keys", "rt_codex_1", "codex", ".", "Codex chat").unwrap();
    // The turn's own options: the key is in a Pi conversation's environment...
    let pi = crate::exec_options_for_session(&st, &st.db.session_detail("ses-pi-keys").unwrap()).unwrap();
    assert_eq!(pi.env.get("DEEPSEEK_API_KEY").map(String::as_str), Some(secret));
    // ...and nowhere near another provider's child.
    let codex = crate::exec_options_for_session(&st, &st.db.session_detail("ses-codex-keys").unwrap()).unwrap();
    assert!(!codex.env.contains_key("DEEPSEEK_API_KEY"), "a Codex child never carries a Pi key");
    assert!(!codex.env.values().any(|value| value == secret));
    pi_call(&st, "pi.key.delete", json!({"provider": "deepseek"})).await.unwrap();
}

#[tokio::test]
async fn an_unknown_provider_or_an_empty_key_is_an_invalid_argument() {
    let (st, _events) = state();
    let unknown = pi_call(&st, "pi.key.set", json!({"provider": "not-a-provider", "key": "sk-x"})).await.unwrap_err();
    assert_eq!(unknown.1.code, "invalid_argument");
    let empty = pi_call(&st, "pi.key.set", json!({"provider": "groq", "key": "   "})).await.unwrap_err();
    assert_eq!(empty.1.code, "invalid_argument");
    let missing = pi_call(&st, "pi.key.delete", json!({})).await.unwrap_err();
    assert_eq!(missing.1.code, "invalid_argument");
    // A key test with nothing stored answers without starting Pi at all.
    let tested = pi_call(&st, "pi.key.test", json!({"provider": "groq"})).await.unwrap();
    assert_eq!(tested["ok"], false);
    assert!(tested["detail"].as_str().is_some_and(|detail| detail.contains("No key is stored")), "{tested}");
}
