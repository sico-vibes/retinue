// Section D, wave 3: MCP OAuth for connectors. Included into `phase2a_tests`
// next to `connectors_tests`, so it shares that module's fixtures (`state`,
// `dispatch`, `json!`, `Value`, `Aff`, `Uuid`, `AppState`). Each test starts its
// own fake authorization server and MCP server on 127.0.0.1 (plain HTTP is
// allowed for a server on this PC, and the callback listener is loopback only),
// so tests running in one process never meet.

use std::sync::Mutex as OAuthMutex;

use sha2::Digest as _;

/// What the fake service issues and how it answers.
#[derive(Clone, Copy)]
struct OAuthConfig {
    /// Send the `WWW-Authenticate` hint naming the protected-resource metadata.
    hint: bool,
    /// `expires_in` of an issued access token.
    expires_in: i64,
    /// `expires_in` of a refreshed access token.
    refresh_expires_in: i64,
    /// Answer a refresh with `invalid_grant`.
    fail_refresh: bool,
    /// Length of the random-looking part of each token, so a bundle can be made
    /// to exceed the 2,048-byte credential cap.
    token_len: usize,
}

impl Default for OAuthConfig {
    fn default() -> Self {
        Self { hint: true, expires_in: 3_600, refresh_expires_in: 3_600, fail_refresh: false, token_len: 1_200 }
    }
}

#[derive(Default)]
struct OAuthFake {
    challenge: Option<String>,
    state: Option<String>,
    code: Option<String>,
    access: Option<String>,
    refresh: Option<String>,
    issued: usize,
    refreshes: usize,
    revoked: usize,
    bearers: Vec<String>,
}

type OAuthSeen = Arc<OAuthMutex<OAuthFake>>;

fn oauth_find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

async fn oauth_read_request(stream: &mut tokio::net::TcpStream) -> Option<(String, Vec<u8>)> {
    use tokio::io::AsyncReadExt;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = oauth_find(&buffer, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..at]).to_string();
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
            let mut body = buffer[at + 4..].to_vec();
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

fn oauth_header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn oauth_request_line(head: &str) -> (String, String) {
    let mut pieces = head.lines().next().unwrap_or("").split_whitespace();
    (pieces.next().unwrap_or("").to_owned(), pieces.next().unwrap_or("").to_owned())
}

async fn oauth_write(stream: &mut tokio::net::TcpStream, status: &str, content_type: &str, extra: &str, body: &str) {
    use tokio::io::AsyncWriteExt;
    let answer = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(answer.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn oauth_token(fake: &mut OAuthFake, config: OAuthConfig, prefix: &str) -> (String, String) {
    let filler = "a".repeat(config.token_len);
    fake.issued += 1;
    let access = format!("{prefix}-access-{}-{filler}", fake.issued);
    let refresh = format!("{prefix}-refresh-{}-{filler}", fake.issued);
    fake.access = Some(access.clone());
    fake.refresh = Some(refresh.clone());
    (access, refresh)
}

/// A fake authorization server and MCP server on one loopback port:
/// `/.well-known/oauth-protected-resource`, `/.well-known/oauth-authorization-server`,
/// `/register`, `/authorize`, `/token`, `/revoke` and `/mcp`.
async fn oauth_fake(config: OAuthConfig) -> (String, String, OAuthSeen) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}");
    let seen: OAuthSeen = Arc::new(OAuthMutex::new(OAuthFake::default()));
    let log = seen.clone();
    let resource_url = format!("{base}/mcp");
    let resource_for_task = resource_url.clone();
    let root = base.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { break };
            let log = log.clone();
            let root = root.clone();
            let resource_url = resource_for_task.clone();
            tokio::spawn(async move {
                let Some((head, body)) = oauth_read_request(&mut stream).await else { return };
                let (method, target) = oauth_request_line(&head);
                let (path, query) = match target.split_once('?') {
                    Some((path, query)) => (path.to_owned(), query.to_owned()),
                    None => (target.clone(), String::new()),
                };
                match (method.as_str(), path.as_str()) {
                    ("GET", "/.well-known/oauth-protected-resource") => {
                        let body = json!({"resource": resource_url, "authorization_servers": [root], "scopes_supported": ["read"]}).to_string();
                        oauth_write(&mut stream, "200 OK", "application/json", "", &body).await;
                    }
                    ("GET", "/.well-known/oauth-authorization-server") => {
                        let body = json!({
                            "issuer": root,
                            "authorization_endpoint": format!("{root}/authorize"),
                            "token_endpoint": format!("{root}/token"),
                            "registration_endpoint": format!("{root}/register"),
                            "revocation_endpoint": format!("{root}/revoke"),
                            "code_challenge_methods_supported": ["S256"],
                            "scopes_supported": ["read", "write"]
                        })
                        .to_string();
                        oauth_write(&mut stream, "200 OK", "application/json", "", &body).await;
                    }
                    ("POST", "/register") => {
                        let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                        let redirect = request["redirect_uris"][0].as_str().unwrap_or("");
                        assert!(redirect.starts_with("http://127.0.0.1:"), "the callback is loopback only: {redirect}");
                        let body = json!({"client_id": "retinue-test-client", "token_endpoint_auth_method": "none"}).to_string();
                        oauth_write(&mut stream, "201 Created", "application/json", "", &body).await;
                    }
                    ("GET", "/authorize") => {
                        let state = oauth_query(&query, "state").unwrap_or_default();
                        let challenge = oauth_query(&query, "code_challenge").unwrap_or_default();
                        let method = oauth_query(&query, "code_challenge_method").unwrap_or_default();
                        let redirect = oauth_query(&query, "redirect_uri").unwrap_or_default();
                        assert_eq!(method, "S256", "PKCE must be S256");
                        assert!(!state.is_empty(), "a random state is sent");
                        assert!(!challenge.is_empty(), "a PKCE challenge is sent");
                        let code = format!("code-{}", Uuid::new_v4().simple());
                        {
                            let mut fake = log.lock().unwrap();
                            fake.state = Some(state.clone());
                            fake.challenge = Some(challenge);
                            fake.code = Some(code.clone());
                        }
                        let location = format!("{redirect}?code={code}&state={state}");
                        oauth_write(&mut stream, "302 Found", "text/plain", &format!("Location: {location}\r\n"), "").await;
                    }
                    ("POST", "/token") => {
                        let form = String::from_utf8_lossy(&body).to_string();
                        let grant = oauth_query(&form, "grant_type").unwrap_or_default();
                        let renew = grant == "refresh_token";
                        if renew {
                            let mut failed = false;
                            let issued = {
                                let mut fake = log.lock().unwrap();
                                fake.refreshes += 1;
                                if config.fail_refresh {
                                    failed = true;
                                    None
                                } else {
                                    Some(oauth_token(&mut fake, config, "renewed"))
                                }
                            };
                            if failed {
                                let body = json!({"error": "invalid_grant"}).to_string();
                                oauth_write(&mut stream, "400 Bad Request", "application/json", "", &body).await;
                                return;
                            }
                            let issued = issued.expect("a refreshed token");
                            let body = json!({"access_token": issued.0, "refresh_token": issued.1, "token_type": "Bearer", "expires_in": config.refresh_expires_in}).to_string();
                            oauth_write(&mut stream, "200 OK", "application/json", "", &body).await;
                            return;
                        }
                        let code = oauth_query(&form, "code").unwrap_or_default();
                        let verifier = oauth_query(&form, "code_verifier").unwrap_or_default();
                        let expected = {
                            let fake = log.lock().unwrap();
                            assert_eq!(fake.code.as_deref(), Some(code.as_str()), "the code the daemon sends is the one it was given");
                            fake.challenge.clone()
                        };
                        let computed = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
                        if expected.as_deref() != Some(computed.as_str()) {
                            let body = json!({"error": "invalid_grant", "error_description": "PKCE failed"}).to_string();
                            oauth_write(&mut stream, "400 Bad Request", "application/json", "", &body).await;
                            return;
                        }
                        let (access, refresh) = {
                            let mut fake = log.lock().unwrap();
                            oauth_token(&mut fake, config, "first")
                        };
                        let body = json!({"access_token": access, "refresh_token": refresh, "token_type": "Bearer", "expires_in": config.expires_in, "scope": "read"}).to_string();
                        oauth_write(&mut stream, "200 OK", "application/json", "", &body).await;
                    }
                    ("POST", "/revoke") => {
                        log.lock().unwrap().revoked += 1;
                        oauth_write(&mut stream, "200 OK", "application/json", "", "{}").await;
                    }
                    ("POST", "/mcp") => {
                        let bearer = oauth_header(&head, "authorization").unwrap_or("");
                        if bearer.is_empty() {
                            if config.hint {
                                let extra = format!("WWW-Authenticate: Bearer resource_metadata=\"{root}/.well-known/oauth-protected-resource\"\r\n");
                                oauth_write(&mut stream, "401 Unauthorized", "application/json", &extra, "{}").await;
                            } else {
                                oauth_write(&mut stream, "401 Unauthorized", "application/json", "", "{}").await;
                            }
                            return;
                        }
                        log.lock().unwrap().bearers.push(bearer.to_owned());
                        let message: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                        let id = message["id"].clone();
                        let answer = match message["method"].as_str().unwrap_or("") {
                            "initialize" => json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1"}}}),
                            "tools/list" => json!({"jsonrpc":"2.0","id":id,"result":{"tools":[
                                {"name":"search_issues","description":"Search issues.","annotations":{"readOnlyHint":true},"inputSchema":{"type":"object"}}
                            ]}}),
                            _ if id.is_null() => Value::Null,
                            _ => json!({"jsonrpc":"2.0","id":id,"result":{}}),
                        };
                        if answer.is_null() {
                            oauth_write(&mut stream, "202 Accepted", "application/json", "", "").await;
                        } else {
                            oauth_write(&mut stream, "200 OK", "application/json", "", &answer.to_string()).await;
                        }
                    }
                    _ => oauth_write(&mut stream, "404 Not Found", "text/plain", "", "not found").await,
                }
            });
        }
    });
    (resource_url, base, seen)
}

fn oauth_query(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        if oauth_decode(key) == name {
            return Some(oauth_decode(value));
        }
    }
    None
}

fn oauth_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("zz"), 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `(status, Location)` of one HTTP request, over a raw socket, so a redirect is
/// never followed for us.
async fn oauth_fetch(url: &str) -> (u16, Option<String>) {
    use tokio::io::AsyncWriteExt;
    let without_scheme = url.strip_prefix("http://").expect("the fake service is loopback http");
    let (authority, path) = match without_scheme.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (without_scheme, "/".to_owned()),
    };
    let mut stream = tokio::net::TcpStream::connect(authority).await.expect("the fake service answers");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    use tokio::io::AsyncReadExt;
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer).await;
    let text = String::from_utf8_lossy(&answer).to_string();
    let status = text.split_whitespace().nth(1).and_then(|code| code.parse().ok()).unwrap_or(0);
    let location = text.lines().find_map(|line| line.strip_prefix("Location:")).map(|value| value.trim().to_owned());
    (status, location)
}

/// One OAuth test at a time: the record of opened URLs is process-wide, and a
/// driver test must not steal another test's address.
fn oauth_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn oauth_add(st: &AppState, url: String) -> String {
    let added = dispatch(st, "connector.add", json!({"name": "OAuth Fake", "url": url, "auth": "oauth"})).await.unwrap();
    let connector = &added["connector"];
    assert_eq!(connector["auth"], "oauth");
    assert_eq!(connector["status"], "unchecked");
    assert_eq!(connector["detail"], connectors_oauth::SIGN_IN_NEEDED);
    connector["id"].as_str().unwrap().to_owned()
}

async fn oauth_start(st: &AppState, id: &str) -> Value {
    dispatch(st, "connector.oauth.start", json!({"id": id})).await.unwrap()
}

async fn oauth_poll(st: &AppState, id: &str, wanted: &str) -> Value {
    let mut last = Value::Null;
    for _ in 0..600 {
        last = dispatch(st, "connector.oauth.status", json!({"id": id})).await.unwrap();
        if last["status"] == wanted {
            return last;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the sign-in never became {wanted}: {last}");
}

/// Drives the browser's half of the flow: the consent page redirects to the
/// loopback callback, and the callback answers the page.
async fn oauth_complete(authorization_url: &str) -> (u16, Option<String>) {
    let (status, location) = oauth_fetch(authorization_url).await;
    assert_eq!(status, 302, "the consent page redirects to the callback");
    let location = location.expect("a redirect turned up");
    let (callback_status, _) = oauth_fetch(&location).await;
    (callback_status, Some(location))
}

async fn oauth_sign_in(st: &AppState, id: &str) -> String {
    let started = oauth_start(st, id).await;
    let _ = connectors_oauth::take_opened_urls();
    let (status, _) = oauth_complete(started["authorizationUrl"].as_str().unwrap()).await;
    assert_eq!(status, 200, "the callback page answers");
    oauth_poll(st, id, "authorized").await;
    started["authorizationUrl"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn oauth_sign_in_discovers_registers_pkce_and_stores_split_tokens() {
    let (st, _) = state();
    // The opened-URL record is shared by every test in this process, so these
    // tests take one gate; they take milliseconds each.
    let _gate = oauth_gate().lock().await;
    let (url, _, seen) = oauth_fake(OAuthConfig::default()).await;
    let id = oauth_add(&st, url).await;
    let before = dispatch(&st, "connector.oauth.status", json!({"id": id})).await.unwrap();
    assert_eq!(before["status"], "idle");
    assert_eq!(before["auth"], "oauth");

    let started = oauth_start(&st, &id).await;
    assert_eq!(started["status"], "pending");
    let consent = started["authorizationUrl"].as_str().unwrap().to_owned();
    assert!(consent.contains("code_challenge_method=S256"), "{consent}");
    assert!(consent.contains("code_challenge="), "{consent}");
    assert!(consent.contains("state="), "{consent}");
    assert!(consent.contains("resource="), "the RFC 8707 resource is sent");
    // The daemon opened the owner's browser at exactly the address it returned.
    assert_eq!(connectors_oauth::take_opened_urls(), vec![consent.clone()]);

    let (status, _) = oauth_complete(&consent).await;
    assert_eq!(status, 200);
    let authorized = oauth_poll(&st, &id, "authorized").await;
    assert_eq!(authorized["detail"], Value::Null);

    // The bundle reached Credential Manager and was split over the cap.
    let saved = connectors_oauth::read_value(&id).unwrap().expect("a saved bundle");
    assert!(saved.len() > 2_048, "two long tokens exceed the cap: {}", saved.len());
    let bundle: Value = serde_json::from_str(&saved).unwrap();
    let access = bundle["access_token"].as_str().unwrap();
    assert!(access.starts_with("first-access-1-"), "{}", &access[..20]);
    assert!(crate::secrets::store::read(&format!("connector/{id}.1")).unwrap().is_some(), "the bundle continues in a numbered entry");
    // No reply carries a token.
    let replies = dispatch(&st, "connector.oauth.status", json!({"id": id})).await.unwrap().to_string();
    assert!(!replies.contains(access));

    // A check uses the access token, and the server saw it.
    let checked = dispatch(&st, "connector.test", json!({"id": id})).await.unwrap();
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["toolCount"], 1);
    assert!(seen.lock().unwrap().bearers.iter().any(|bearer| bearer == &format!("Bearer {access}")));
    // The PKCE verifier that the server checked was the one the daemon kept.
    assert_eq!(seen.lock().unwrap().issued, 1, "no refresh was needed");
}

#[tokio::test]
async fn oauth_expired_access_token_is_refreshed_before_a_check() {
    let (st, _) = state();
    // The opened-URL record is shared by every test in this process, so these
    // tests take one gate; they take milliseconds each.
    let _gate = oauth_gate().lock().await;
    let (url, _, seen) = oauth_fake(OAuthConfig { expires_in: 0, ..OAuthConfig::default() }).await;
    let id = oauth_add(&st, url).await;
    oauth_sign_in(&st, &id).await;

    // `expires_in: 0` makes the token stale at once, so the next call renews it.
    let checked = dispatch(&st, "connector.test", json!({"id": id})).await.unwrap();
    assert_eq!(checked["ok"], true, "{checked}");
    let (refreshes, bearer) = {
        let fake = seen.lock().unwrap();
        (fake.refreshes, fake.bearers.last().cloned().unwrap_or_default())
    };
    assert_eq!(refreshes, 1, "one refresh, not one per call");
    assert!(bearer.contains("renewed-access-"), "{bearer}");
    let status = dispatch(&st, "connector.oauth.status", json!({"id": id})).await.unwrap();
    assert_eq!(status["status"], "authorized");
}

#[tokio::test]
async fn oauth_wrong_state_is_refused_and_cancel_closes_the_listener() {
    let (st, _) = state();
    // The opened-URL record is shared by every test in this process, so these
    // tests take one gate; they take milliseconds each.
    let _gate = oauth_gate().lock().await;
    let (url, _, _) = oauth_fake(OAuthConfig::default()).await;
    let id = oauth_add(&st, url).await;
    let started = oauth_start(&st, &id).await;
    let consent = started["authorizationUrl"].as_str().unwrap().to_owned();
    let _ = connectors_oauth::take_opened_urls();
    let redirect = oauth_query(consent.split_once('?').unwrap().1, "redirect_uri").unwrap();
    let state_value = oauth_query(consent.split_once('?').unwrap().1, "state").unwrap();

    // A wrong state grants nothing, and the flow keeps waiting for the real one.
    let (wrong_status, _) = oauth_fetch(&format!("{redirect}?code=stolen&state=wrong")).await;
    assert_eq!(wrong_status, 400);
    let still = dispatch(&st, "connector.oauth.status", json!({"id": id})).await.unwrap();
    assert_eq!(still["status"], "pending");

    let cancelled = dispatch(&st, "connector.oauth.cancel", json!({"id": id})).await.unwrap();
    assert_eq!(cancelled["cancelled"], true);
    let idle = dispatch(&st, "connector.oauth.status", json!({"id": id})).await.unwrap();
    assert_eq!(idle["status"], "idle");
    assert_eq!(dispatch(&st, "connector.oauth.cancel", json!({"id": id})).await.unwrap()["cancelled"], false);

    // The listener is closed: even the right state cannot arrive any more.
    let authority = redirect.strip_prefix("http://").unwrap().split('/').next().unwrap().to_owned();
    assert!(tokio::net::TcpStream::connect(&authority).await.is_err(), "the callback listener is closed");
    assert!(!state_value.is_empty());
}

#[tokio::test]
async fn oauth_start_refuses_a_token_connector_and_unknown_ids() {
    let (st, _) = state();
    // The opened-URL record is shared by every test in this process, so these
    // tests take one gate; they take milliseconds each.
    let _gate = oauth_gate().lock().await;
    let (url, _, _) = oauth_fake(OAuthConfig::default()).await;
    let token_id = dispatch(&st, "connector.add", json!({"name": "Token kind", "url": url, "token": "tok_live_123456"})).await.unwrap()["connector"]["id"].as_str().unwrap().to_owned();
    let refused = dispatch(&st, "connector.oauth.start", json!({"id": token_id})).await.unwrap_err();
    assert_eq!(refused.1.code, "conflict");
    assert!(refused.1.message.contains("token"), "{}", refused.1.message);

    let missing = dispatch(&st, "connector.oauth.start", json!({"id": "no-such-connector"})).await.unwrap_err();
    assert_eq!(missing.1.code, "not_found");
    let status = dispatch(&st, "connector.oauth.status", json!({"id": "no-such-connector"})).await.unwrap_err();
    assert_eq!(status.1.code, "not_found");
    let cancel = dispatch(&st, "connector.oauth.cancel", json!({"id": "no-such-connector"})).await.unwrap_err();
    assert_eq!(cancel.1.code, "not_found");
    // A token and the OAuth kind in one call is refused.
    let both = dispatch(&st, "connector.add", json!({"name": "Both", "url": "https://mcp.example.com/mcp", "auth": "oauth", "token": "x"})).await.unwrap_err();
    assert_eq!(both.1.code, "invalid_argument");
    let token_without_value = dispatch(&st, "connector.add", json!({"name": "Empty", "url": "https://mcp.example.com/mcp", "auth": "token"})).await.unwrap_err();
    assert_eq!(token_without_value.1.code, "invalid_argument");
}

#[tokio::test]
async fn oauth_remove_revokes_the_token_and_deletes_every_part() {
    let (st, _) = state();
    // The opened-URL record is shared by every test in this process, so these
    // tests take one gate; they take milliseconds each.
    let _gate = oauth_gate().lock().await;
    let (url, _, seen) = oauth_fake(OAuthConfig::default()).await;
    let id = oauth_add(&st, url.clone()).await;
    oauth_sign_in(&st, &id).await;
    assert!(crate::secrets::store::read(&format!("connector/{id}.1")).unwrap().is_some());

    let removed = dispatch(&st, "connector.remove", json!({"id": id, "confirm": true})).await.unwrap();
    assert_eq!(removed["removed"], true);
    assert_eq!(seen.lock().unwrap().revoked, 1, "the service was told to revoke");
    assert!(crate::secrets::store::read(&format!("connector/{id}")).unwrap().is_none());
    assert!(crate::secrets::store::read(&format!("connector/{id}.1")).unwrap().is_none());
    // Clearing the sign-in through an update does the same, and the row's auth drops back.
    let second = oauth_add(&st, format!("{url}?again=1")).await;
    oauth_sign_in(&st, &second).await;
    let cleared = dispatch(&st, "connector.update", json!({"id": second, "token": ""})).await.unwrap();
    assert_eq!(cleared["connector"]["auth"], "none");
    assert!(connectors_oauth::read_value(&second).unwrap().is_none());
    assert_eq!(seen.lock().unwrap().revoked, 2);
}

#[tokio::test]
async fn oauth_a_failed_refresh_says_to_connect_again() {
    let (st, _) = state();
    // The opened-URL record is shared by every test in this process, so these
    // tests take one gate; they take milliseconds each.
    let _gate = oauth_gate().lock().await;
    let (url, _, _) = oauth_fake(OAuthConfig { expires_in: 0, fail_refresh: true, ..OAuthConfig::default() }).await;
    let id = oauth_add(&st, url).await;
    oauth_sign_in(&st, &id).await;

    let checked = dispatch(&st, "connector.test", json!({"id": id})).await.unwrap();
    assert_eq!(checked["ok"], false);
    assert!(checked["detail"].as_str().unwrap().contains("Connect"), "{}", checked["detail"]);
    let status = dispatch(&st, "connector.oauth.status", json!({"id": id})).await.unwrap();
    assert_eq!(status["status"], "error");
    assert!(status["detail"].as_str().unwrap().to_lowercase().contains("renewed"), "{}", status["detail"]);
}

#[tokio::test]
async fn oauth_discovery_falls_back_to_the_well_known_address() {
    let (st, _) = state();
    // The opened-URL record is shared by every test in this process, so these
    // tests take one gate; they take milliseconds each.
    let _gate = oauth_gate().lock().await;
    let (url, _, _) = oauth_fake(OAuthConfig { hint: false, ..OAuthConfig::default() }).await;
    let id = oauth_add(&st, url).await;
    let started = oauth_start(&st, &id).await;
    assert_eq!(started["status"], "pending", "the well-known document alone is enough");
    let _ = connectors_oauth::take_opened_urls();
    dispatch(&st, "connector.oauth.cancel", json!({"id": id})).await.unwrap();
}

#[test]
fn oauth_credential_value_over_the_cap_is_split_and_joined() {
    let id = format!("chunk-{}", Uuid::new_v4().simple());
    let long = "x".repeat(5_000);
    connectors_oauth::write_value(&id, &long).unwrap();
    assert_eq!(connectors_oauth::read_value(&id).unwrap().as_deref(), Some(long.as_str()));
    let first = crate::secrets::store::read(&format!("connector/{id}")).unwrap().expect("part 0");
    let second = crate::secrets::store::read(&format!("connector/{id}.1")).unwrap().expect("part 1");
    let third = crate::secrets::store::read(&format!("connector/{id}.2")).unwrap().expect("part 2");
    assert_eq!(first.as_str().len(), 2_048);
    assert_eq!(second.as_str().len(), 2_048);
    assert_eq!(third.as_str().len(), 904);
    assert!(crate::secrets::store::read(&format!("connector/{id}.3")).unwrap().is_none());

    // A value inside the cap stays one entry, and an older long value is gone.
    connectors_oauth::write_value(&id, "short").unwrap();
    assert_eq!(connectors_oauth::read_value(&id).unwrap().as_deref(), Some("short"));
    assert!(crate::secrets::store::read(&format!("connector/{id}.1")).unwrap().is_none());

    // A value beyond eight parts is refused rather than silently truncated.
    assert!(connectors_oauth::write_value(&id, &"y".repeat(2_048 * 8 + 1)).is_err());
    connectors_oauth::delete_value(&id);
    assert!(connectors_oauth::read_value(&id).unwrap().is_none());
}

/// Every new method is dispatched, not only present: this is the wave-1 and
/// wave-2 lesson (`task.checkBack` could not be reached from the app).
// (Rebuilt on 17 November 2026 while the OAuth lane verified the listener's close.)
#[test]
fn oauth_methods_are_reachable_through_dispatch() {
    let source = include_str!("connectors_oauth.rs");
    for method in ["connector.oauth.start", "connector.oauth.status", "connector.oauth.cancel"] {
        assert!(source.contains(&format!("\"{method}\"")), "{method} is routed in its module");
    }
    let main = include_str!("main.rs");
    assert!(main.contains("\"connector.oauth.start\" | \"connector.oauth.status\" | \"connector.oauth.cancel\" => connectors_oauth::rpc"), "the dispatch arm names all three");
    assert!(main.contains("mod connectors_oauth;"));
}
