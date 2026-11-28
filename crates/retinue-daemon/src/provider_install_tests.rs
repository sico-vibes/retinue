//! Installing and updating the coding-agent CLIs (21 November 2026).
//!
//! Everything here runs against a local stub origin (`agent_install::TEST_ORIGIN`,
//! the `GH_STUB`/`TEST_ROOTS` pattern this repository already uses): the publisher
//! addresses are replaced by a loopback server this file starts, so **no test
//! touches the network** and none downloads a real build. What the suite proves is
//! the module's rules, not a publisher's current release: a good install, a wrong
//! digest, a truncated download, an entry that escapes, a symlink, a damaged
//! receipt, an in-use previous version, a cancel, a refusal for a paired device,
//! and the per-provider table.

use crate::agent_install::{self, Kind};
use crate::remote::policy::{self, MethodClass};
use crate::AppState;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The stub origin is one process-wide seam, so the tests that use it take turns
/// (the `import_tests` lesson).
static ORIGIN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn lock() -> tokio::sync::MutexGuard<'static, ()> {
    ORIGIN_LOCK.lock().await
}

/// Points the module at the stub for as long as it lives.
struct Origin;

impl Origin {
    fn set(origin: &str) -> Origin {
        *agent_install::TEST_ORIGIN.lock().unwrap() = Some(origin.to_owned());
        Origin
    }
}

impl Drop for Origin {
    fn drop(&mut self) {
        *agent_install::TEST_ORIGIN.lock().unwrap() = None;
    }
}

// ---------------------------------------------------------------------------
// The stub origin

#[derive(Clone)]
struct Reply {
    status: u16,
    body: Vec<u8>,
    /// Write only this many bytes of the body, then close: a truncated download.
    send: Option<usize>,
    /// Wait this long before answering, so a cancel has somewhere to land.
    delay: Option<Duration>,
}

impl Reply {
    fn ok(body: Vec<u8>) -> Reply {
        Reply { status: 200, body, send: None, delay: None }
    }
    fn text(body: &str) -> Reply {
        Reply::ok(body.as_bytes().to_vec())
    }
    fn truncated(body: Vec<u8>, send: usize) -> Reply {
        Reply { status: 200, body, send: Some(send), delay: None }
    }
    fn slow(body: Vec<u8>) -> Reply {
        Reply { status: 200, body, send: None, delay: Some(Duration::from_millis(800)) }
    }
    fn missing() -> Reply {
        Reply { status: 404, body: b"not found".to_vec(), send: None, delay: None }
    }
}

/// Binds a port and answers the routes the builder makes for it. Bind first, then
/// build the routes, so a publisher document can name this stub's own address.
async fn serve_with(build: impl FnOnce(&str) -> Vec<(String, Reply)>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let origin = format!("http://127.0.0.1:{port}");
    let routes: Arc<HashMap<String, Reply>> = Arc::new(build(&origin).into_iter().collect());
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { break };
            let routes = routes.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                let mut head = String::new();
                let mut buffer = [0u8; 4096];
                while !head.contains("\r\n\r\n") {
                    match socket.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => head.push_str(&String::from_utf8_lossy(&buffer[..read])),
                    }
                }
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let reply = routes.get(&path).cloned().unwrap_or_else(Reply::missing);
                if let Some(delay) = reply.delay {
                    tokio::time::sleep(delay).await;
                }
                let head = format!("HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.status, reply.body.len());
                let _ = socket.write_all(head.as_bytes()).await;
                let body = match reply.send {
                    Some(send) => &reply.body[..send.min(reply.body.len())],
                    None => &reply.body[..],
                };
                let _ = socket.write_all(body).await;
                let _ = socket.flush().await;
            });
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("retinue-install-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// One tar entry per tuple with the given typeflag, gzip'd. A stub archive: the
/// module reads the name, the size and the type, and refuses a link.
fn tar_gz(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (name, typeflag, bytes) in entries {
        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        header[..name_bytes.len().min(100)].copy_from_slice(&name_bytes[..name_bytes.len().min(100)]);
        header[100..107].copy_from_slice(b"0000755");
        header[108..115].copy_from_slice(b"0000000");
        header[116..123].copy_from_slice(b"0000000");
        header[124..135].copy_from_slice(format!("{:011o}", bytes.len()).as_bytes());
        header[136..147].copy_from_slice(b"00000000000");
        header[156] = *typeflag;
        header[257..262].copy_from_slice(b"ustar");
        header[263..265].copy_from_slice(b"00");
        tar.extend_from_slice(&header);
        tar.extend_from_slice(bytes);
        tar.extend(std::iter::repeat_n(0u8, (512 - bytes.len() % 512) % 512));
    }
    tar.extend(std::iter::repeat_n(0u8, 1024));
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&tar).unwrap();
    encoder.finish().unwrap()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    agent_install::hex_encode(&sha2::Sha256::digest(bytes))
}

fn sha512_integrity(bytes: &[u8]) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(bytes)))
}

/// A state whose data folder is a temp folder, so `agents/<provider>` lands there.
fn state_with_data(tag: &str) -> (AppState, tokio::sync::broadcast::Receiver<retinue_protocol::EventEnvelope>, PathBuf) {
    let (st, rx) = crate::phase2a_tests::state();
    let data = temp(tag);
    *st.team.data_dir.lock().unwrap() = Some(data.clone());
    (st, rx, data)
}

/// The Pi routes: a GitHub release with a zip asset and its `SHA256SUMS`.
fn pi_routes(origin: &str, zip: &[u8], checksum: &str) -> Vec<(String, Reply)> {
    let release = json!({
        "tag_name": "v1.6.0",
        "assets": [
            { "name": "pi-windows-x64.zip", "size": zip.len(), "browser_download_url": format!("{origin}/asset/pi.zip") },
            { "name": "SHA256SUMS", "size": 80, "browser_download_url": format!("{origin}/asset/SHA256SUMS") },
        ]
    });
    vec![
        ("/repos/earendil-works/pi/releases/latest".to_owned(), Reply::text(&release.to_string())),
        ("/asset/pi.zip".to_owned(), Reply::ok(zip.to_vec())),
        ("/asset/SHA256SUMS".to_owned(), Reply::text(&format!("{checksum}  pi-windows-x64.zip\n"))),
    ]
}

async fn wait_for_job(st: &AppState, job: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(row) = st.agent_install.with(job, |row| row.value()) {
            if matches!(row["phase"].as_str(), Some("done" | "failed" | "cancelled")) {
                return row;
            }
        }
        assert!(Instant::now() < deadline, "the install did not finish: {job}");
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
}

/// Starts a manifest-mode Pi install against the stub and answers the job's end.
async fn run_pi_install(tag: &str, zip: Vec<u8>, checksum: String, slow: bool) -> (String, PathBuf, Vec<String>) {
    let (st, mut rx, data) = state_with_data(tag);
    let origin = serve_with(|origin| {
        let mut routes = pi_routes(origin, &zip, &checksum);
        if slow {
            routes[1].1 = Reply::slow(zip.clone());
        }
        routes
    })
    .await;
    let _guard = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    let plan = agent_install::plan_reply(&st, "pi").await.unwrap()["plan"].clone();
    let started = agent_install::rpc(&st, "runtime.install", &json!({
        "provider": "pi", "confirm": true, "version": plan["version"], "digestPrefix": plan["digestPrefix"],
    })).await.unwrap();
    let finished = wait_for_job(&st, started["jobId"].as_str().unwrap()).await;
    let mut phases = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if event.event_type == agent_install::EVENT {
            phases.push(event.payload["phase"].as_str().unwrap_or("").to_owned());
        }
    }
    (finished["error"].as_str().unwrap_or("").to_owned(), data, phases)
}

// ---------------------------------------------------------------------------
// The per-provider table

#[test]
fn the_provider_table_is_what_the_design_says() {
    // A golden: a provider added without a source, or a page that moved, is a
    // deliberate change to this line, not a silent one.
    let golden: [(&str, &str, &str, bool, &str); 8] = [
        ("claude", "Claude Code", "claude", true, "https://docs.claude.com/en/docs/claude-code/setup"),
        ("codex", "Codex", "codex", true, "https://github.com/openai/codex"),
        ("opencode", "OpenCode", "opencode", true, "https://github.com/sst/opencode"),
        ("pi", "Pi", "pi", true, "https://pi.dev"),
        ("cursor", "Cursor CLI", "agent", false, "https://cursor.com/cli"),
        ("grok", "Grok CLI", "grok", false, "https://x.ai/cli"),
        ("antigravity", "Antigravity", "agy", false, "https://antigravity.google/docs/getting-started"),
        ("qwen", "Qwen Code", "qwen", false, "https://github.com/QwenLM/qwen-code"),
    ];
    assert_eq!(agent_install::ROWS.len(), golden.len());
    for (row, (provider, display, command, one_click, page)) in agent_install::ROWS.iter().zip(golden) {
        assert_eq!(row.provider, provider);
        assert_eq!(row.display_name, display);
        assert_eq!(row.command, command);
        assert_eq!(row.source.is_some(), one_click, "{provider}");
        assert_eq!(row.page, page, "{provider}");
        if one_click {
            assert!(!row.entries.is_empty(), "{provider} needs an entry name an archive can match");
        }
    }
    // Every provider Retinue discovers has a row, and no row invents one.
    for provider in retinue_runtime::discovery_ids() {
        assert!(agent_install::ROWS.iter().any(|row| row.provider == provider), "{provider} has no install row");
    }
    // The four one-click providers download from the hosts Retinue allows.
    let sources: Vec<(&str, &str)> = agent_install::ROWS
        .iter()
        .filter_map(|row| row.source.map(|source| (row.provider, match source {
            agent_install::Source::ClaudeManifest => "downloads.claude.ai",
            agent_install::Source::GitHubAsset { .. } => "api.github.com",
            agent_install::Source::NpmPlatform { .. } => "registry.npmjs.org",
        })))
        .collect();
    assert_eq!(sources, vec![("claude", "downloads.claude.ai"), ("codex", "api.github.com"), ("opencode", "registry.npmjs.org"), ("pi", "api.github.com")]);
}

#[tokio::test]
async fn a_provider_without_a_digest_story_answers_a_link_and_nothing_else() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("link");
    for provider in ["cursor", "grok", "antigravity", "qwen"] {
        let reply = agent_install::plan_reply(&st, provider).await.unwrap();
        let plan = &reply["plan"];
        assert_eq!(plan["mode"], "link", "{provider}");
        assert_eq!(plan["supported"], false, "{provider}");
        assert_eq!(plan["version"], Value::Null, "{provider}");
        assert!(plan["page"].as_str().is_some_and(|page| page.starts_with("https://")), "{provider}");
        assert!(!data.join("agents").exists(), "{provider} wrote nothing");
    }
    // An install of a link-only provider is refused with its own sentence.
    let refused = agent_install::rpc(&st, "runtime.install", &json!({"provider": "cursor", "confirm": true})).await.unwrap_err();
    assert_eq!(refused.0, axum::http::StatusCode::CONFLICT);
    assert!(refused.1.message.contains("Open its install page"), "{}", refused.1.message);
    let _ = std::fs::remove_dir_all(&data);
}

// ---------------------------------------------------------------------------
// Manage: the good path

#[tokio::test]
async fn a_managed_install_downloads_checks_and_publishes_a_receipt() {
    let _lock = lock().await;
    let (st, mut rx, data) = state_with_data("good");
    let zip = zip_bytes(&[("pi.exe", b"MZ the stub coding agent")]);
    let checksum = sha256_hex(&zip);
    let origin = serve_with(|origin| pi_routes(origin, &zip, &checksum)).await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();

    let plan = agent_install::plan_reply(&st, "pi").await.unwrap()["plan"].clone();
    assert_eq!(plan["mode"], "manage");
    assert_eq!(plan["version"], "1.6.0");
    assert_eq!(plan["sourceHost"], "api.github.com");
    assert_eq!(plan["digestPrefix"], &checksum[..12]);
    assert_eq!(plan["digestAlgorithm"], "sha256");
    assert_eq!(plan["bytes"], zip.len());
    assert!(plan["freeBytes"].as_u64().is_some() || cfg!(not(windows)), "a plan states the free space where the platform answers it");
    assert_eq!(plan["runsTurn"], false);
    assert_eq!(plan["autoUpdate"], false);

    let started = agent_install::rpc(&st, "runtime.install", &json!({
        "provider": "pi", "confirm": true, "version": plan["version"], "digestPrefix": plan["digestPrefix"],
    })).await.unwrap();
    let job = started["jobId"].as_str().unwrap().to_owned();
    let finished = wait_for_job(&st, &job).await;
    assert_eq!(finished["phase"], "done", "{finished}");
    assert_eq!(finished["version"], "1.6.0");

    // The version folder, the receipt, and discovery's own answer.
    let folder = data.join("agents").join("pi");
    assert_eq!(std::fs::read(folder.join("1.6.0").join("pi.exe")).unwrap(), b"MZ the stub coding agent");
    let entry = retinue_runtime::managed_entry(&data.join("agents"), "pi").expect("the receipt names the installed copy");
    assert_eq!(entry.version, "1.6.0");
    assert_eq!(entry.digest, checksum);
    assert_eq!(entry.command.executable, folder.join("1.6.0").join("pi.exe"));
    // No staging is left behind.
    let staging = folder.join("staging");
    assert!(!staging.exists() || std::fs::read_dir(&staging).unwrap().next().is_none());

    // The consent record names the version and the digest, and no client may set it.
    let settings = st.db.settings().unwrap();
    let consent_key = agent_install::consent_key("pi");
    let auto_key = agent_install::auto_update_key("pi");
    let consent = &settings[&consent_key];
    assert_eq!(consent["version"], "1.6.0");
    assert_eq!(consent["digest"], checksum);
    assert_eq!(consent["sourceHost"], "api.github.com");
    assert!(consent["at"].as_str().is_some_and(|at| at.contains('T')));
    assert_eq!(settings[&auto_key], Value::Null, "auto-update is off by default");
    assert_eq!(crate::validate_setting(&consent_key, &json!({})).unwrap_err().1.code, "invalid_argument");

    // The event carries provider, phase, version and bytes, and nothing else.
    let mut phases = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if event.event_type != agent_install::EVENT {
            continue;
        }
        let mut keys: Vec<String> = event.payload.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["bytesDone", "bytesTotal", "phase", "provider", "version"], "{}", event.payload);
        let text = event.payload.to_string();
        assert!(!text.contains("http"), "an event never names a URL: {text}");
        assert!(!text.contains(&data.display().to_string()), "an event never names a path: {text}");
        phases.push(event.payload["phase"].as_str().unwrap_or("").to_owned());
    }
    for phase in ["downloading", "checking", "installing", "done"] {
        assert!(phases.iter().any(|seen| seen == phase), "{phase} was not announced: {phases:?}");
    }
    let _ = std::fs::remove_dir_all(&data);
}

// ---------------------------------------------------------------------------
// Manage: every refusal

#[tokio::test]
async fn a_wrong_digest_installs_nothing() {
    let _lock = lock().await;
    let zip = zip_bytes(&[("pi.exe", b"MZ a build nobody published")]);
    let (message, data, _) = run_pi_install("digest", zip, sha256_hex(b"something else"), false).await;
    assert_eq!(message, "The download did not match the publisher's checksum. Nothing was installed.");
    assert!(!data.join("agents").join("pi").join("1.6.0").exists());
    assert!(!data.join("agents").join("pi").join("current.json").exists());
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn a_truncated_download_installs_nothing() {
    let _lock = lock().await;
    let zip = zip_bytes(&[("pi.exe", b"MZ the whole build")]);
    let checksum = sha256_hex(&zip);
    let (st, _rx, data) = state_with_data("truncated");
    let origin = serve_with(|origin| {
        let mut routes = pi_routes(origin, &zip, &checksum);
        routes[1].1 = Reply::truncated(zip.clone(), zip.len() / 2);
        routes
    })
    .await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    let plan = agent_install::plan_reply(&st, "pi").await.unwrap()["plan"].clone();
    let started = agent_install::rpc(&st, "runtime.install", &json!({
        "provider": "pi", "confirm": true, "version": plan["version"], "digestPrefix": plan["digestPrefix"],
    })).await.unwrap();
    let finished = wait_for_job(&st, started["jobId"].as_str().unwrap()).await;
    assert_eq!(finished["phase"], "failed");
    let message = finished["error"].as_str().unwrap_or("");
    assert!(message.contains("stopped before it finished") || message.contains("different size"), "{message}");
    assert!(!data.join("agents").join("pi").join("1.6.0").exists());
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn an_archive_entry_that_escapes_is_refused() {
    let _lock = lock().await;
    for (tag, name) in [("dotdot", "../escaped.exe"), ("absolute", "/escaped.exe")] {
        let zip = zip_bytes(&[(name, b"MZ out of bounds")]);
        let checksum = sha256_hex(&zip);
        let (message, data, _) = run_pi_install(tag, zip, checksum, false).await;
        assert_eq!(message, "The archive holds a folder that leaves the install folder. Nothing was installed.", "{tag}");
        assert!(!data.join("escaped.exe").exists(), "{tag}");
        assert!(!data.join("agents").join("escaped.exe").exists(), "{tag}");
        let _ = std::fs::remove_dir_all(&data);
    }
}

#[test]
fn a_symlink_entry_is_refused() {
    let root = temp("symlink");
    let archive = root.join("pi.tgz");
    std::fs::write(&archive, tar_gz(&[("package/bin/pi", b'2', b"/etc/passwd")])).unwrap();
    {
        use std::io::Read as _;
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(std::fs::File::open(&archive).unwrap()).read_to_end(&mut raw).unwrap();
        // A symbolic link is the ASCII character '2', not the byte 2 (22 November 2026).
        assert_eq!(raw[156], b'2', "the fixture writes the typeflag at 156");
    }
    let into = root.join("out");
    std::fs::create_dir_all(&into).unwrap();
    let refused = agent_install::extract(&archive, Kind::TarGz, &into, &["pi"], None).unwrap_err();
    assert!(refused.contains("link"), "{refused}");
    assert!(!into.join("package").join("bin").join("pi").exists(), "nothing from the link is written");
    // A containing `..` in a tar entry is refused the same way the zip one is.
    std::fs::write(&archive, tar_gz(&[("../escaped", 0u8, b"MZ")])).unwrap();
    let refused = agent_install::extract(&archive, Kind::TarGz, &into, &["escaped"], None).unwrap_err();
    assert!(refused.contains("leaves the install folder"), "{refused}");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_damaged_receipt_is_reported_and_repaired_by_an_install() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("damaged");
    let root = data.join("agents");
    let folder = root.join("pi");
    std::fs::create_dir_all(folder.join("1.6.0")).unwrap();
    std::fs::write(folder.join("1.6.0").join("pi.exe"), b"MZ").unwrap();
    // An entry that leaves its version folder is damage, not a managed copy.
    std::fs::write(folder.join("current.json"), json!({"version": "1.6.0", "entry": "../outside.exe"}).to_string()).unwrap();
    assert!(retinue_runtime::managed_entry(&root, "pi").is_none());
    let zip = zip_bytes(&[("pi.exe", b"MZ")]);
    let checksum = sha256_hex(&zip);
    let origin = serve_with(|origin| pi_routes(origin, &zip, &checksum)).await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    let plan = agent_install::plan_reply(&st, "pi").await.unwrap()["plan"].clone();
    assert_eq!(plan["installed"]["damaged"], true);
    assert_eq!(plan["installed"]["version"], Value::Null, "a damaged receipt names no installed copy");
    let started = agent_install::rpc(&st, "runtime.install", &json!({
        "provider": "pi", "confirm": true, "version": plan["version"], "digestPrefix": plan["digestPrefix"],
    })).await.unwrap();
    let finished = wait_for_job(&st, started["jobId"].as_str().unwrap()).await;
    assert_eq!(finished["phase"], "done", "{finished}");
    let entry = retinue_runtime::managed_entry(&root, "pi").expect("the repaired receipt is whole");
    assert_eq!(entry.digest, checksum);
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn one_previous_version_is_kept_and_an_in_use_one_is_left() {
    let root = temp("sweep");
    let folder = root.join("claude");
    for version in ["0.9.0", "1.0.0", "2.0.0"] {
        std::fs::create_dir_all(folder.join(version)).unwrap();
        std::fs::write(folder.join(version).join("claude.exe"), b"MZ").unwrap();
    }
    // The current version is 2.0.0 and the receipt names 1.0.0 as the one to go
    // back to; 0.9.0 is beyond both.
    #[cfg(windows)]
    let held = {
        use std::os::windows::fs::OpenOptionsExt as _;
        // Share mode 0: the file cannot be deleted while this handle is open.
        std::fs::OpenOptions::new().read(true).share_mode(0).open(folder.join("0.9.0").join("claude.exe")).unwrap()
    };
    let kept = agent_install::sweep_versions(&folder, &["1.0.0".to_owned(), "2.0.0".to_owned()]);
    assert!(folder.join("1.0.0").is_dir() && folder.join("2.0.0").is_dir(), "the current and the previous version stay");
    // `held` exists only on Windows, so the two halves are compiled apart.
    #[cfg(windows)]
    {
        assert_eq!(kept, vec!["0.9.0".to_owned()], "a file in use is left, and reported");
        assert!(folder.join("0.9.0").is_dir());
        drop(held);
    }
    #[cfg(not(windows))]
    {
        assert!(kept.is_empty());
        assert!(!folder.join("0.9.0").exists());
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_cancelled_install_writes_nothing_and_says_so() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("cancel");
    let zip = zip_bytes(&[("pi.exe", b"MZ")]);
    let checksum = sha256_hex(&zip);
    let origin = serve_with(|origin| {
        let mut routes = pi_routes(origin, &zip, &checksum);
        routes[1].1 = Reply::slow(zip.clone());
        routes
    })
    .await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    let plan = agent_install::plan_reply(&st, "pi").await.unwrap()["plan"].clone();
    let started = agent_install::rpc(&st, "runtime.install", &json!({
        "provider": "pi", "confirm": true, "version": plan["version"], "digestPrefix": plan["digestPrefix"],
    })).await.unwrap();
    let job = started["jobId"].as_str().unwrap().to_owned();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let cancelled = agent_install::rpc(&st, "runtime.install.cancel", &json!({"jobId": job})).await.unwrap();
    assert_eq!(cancelled["cancellationRequested"], true);
    let finished = wait_for_job(&st, &job).await;
    assert_eq!(finished["phase"], "cancelled", "{finished}");
    assert_eq!(finished["error"], "The install was cancelled. Nothing was changed.");
    assert!(!data.join("agents").join("pi").join("current.json").exists());
    assert!(!data.join("agents").join("pi").join("1.6.0").exists());
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn an_install_needs_confirmation_and_the_plan_it_was_shown() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("consent");
    let zip = zip_bytes(&[("pi.exe", b"MZ")]);
    let checksum = sha256_hex(&zip);
    let origin = serve_with(|origin| pi_routes(origin, &zip, &checksum)).await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    // No confirmation.
    let refused = agent_install::rpc(&st, "runtime.install", &json!({"provider": "pi"})).await.unwrap_err();
    assert!(refused.1.message.contains("confirm: true"), "{}", refused.1.message);
    // A version or digest the owner was not shown.
    for (version, digest) in [("1.5.9", checksum[..12].to_owned()), ("1.6.0", "000000000000".to_owned())] {
        let refused = agent_install::rpc(&st, "runtime.install", &json!({
            "provider": "pi", "confirm": true, "version": version, "digestPrefix": digest,
        })).await.unwrap_err();
        assert_eq!(refused.1.message, "The plan changed since it was shown. Review it again.");
    }
    // An unknown provider, and an unknown job.
    assert_eq!(agent_install::rpc(&st, "runtime.install.plan", &json!({"provider": "gemini"})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(agent_install::rpc(&st, "runtime.install.status", &json!({"jobId": "nope"})).await.unwrap_err().1.code, "not_found");
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn auto_update_installs_without_a_second_card_only_from_the_consented_host() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("autoupdate");
    let zip = zip_bytes(&[("pi.exe", b"MZ")]);
    let checksum = sha256_hex(&zip);
    let origin = serve_with(|origin| pi_routes(origin, &zip, &checksum)).await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    // With the switch off, a call that repeats nothing from the plan is refused.
    let refused = agent_install::rpc(&st, "runtime.install", &json!({"provider": "pi", "confirm": true})).await.unwrap_err();
    assert!(refused.1.message.contains("version and digestPrefix"), "{}", refused.1.message);
    // The switch on is not enough without a consent for this host...
    st.db.set_setting(&agent_install::auto_update_key("pi"), &json!(true)).unwrap();
    let refused = agent_install::rpc(&st, "runtime.install", &json!({"provider": "pi", "confirm": true})).await.unwrap_err();
    assert!(refused.1.message.contains("version and digestPrefix"), "{}", refused.1.message);
    // ...and a consent for a different host is a different card.
    st.db.set_setting(&agent_install::consent_key("pi"), &json!({"sourceHost": "downloads.claude.ai"})).unwrap();
    let refused = agent_install::rpc(&st, "runtime.install", &json!({"provider": "pi", "confirm": true})).await.unwrap_err();
    assert!(refused.1.message.contains("version and digestPrefix"), "{}", refused.1.message);
    // The same host, with the switch on: the plan's own version and digest are
    // installed, and the consent is renewed for that version.
    st.db.set_setting(&agent_install::consent_key("pi"), &json!({"sourceHost": "api.github.com", "version": "1.5.0"})).unwrap();
    let started = agent_install::rpc(&st, "runtime.install", &json!({"provider": "pi", "confirm": true})).await.unwrap();
    let finished = wait_for_job(&st, started["jobId"].as_str().unwrap()).await;
    assert_eq!(finished["phase"], "done", "{finished}");
    let entry = retinue_runtime::managed_entry(&data.join("agents"), "pi").expect("the auto-update installed");
    assert_eq!(entry.version, "1.6.0");
    assert_eq!(entry.digest, checksum);
    let consent = &st.db.settings().unwrap()[&agent_install::consent_key("pi")];
    assert_eq!(consent["version"], "1.6.0");
    assert_eq!(consent["sourceHost"], "api.github.com");
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn keep_up_to_date_compares_versions_and_never_installs_from_nothing() {
    assert!(agent_install::is_newer("1.6.0", Some("1.5.2")));
    assert!(agent_install::is_newer("2.1.330", Some("2.1.322 (Claude Code)")));
    assert!(agent_install::is_newer("0.160.0", Some("codex-cli 0.159.3")));
    assert!(agent_install::is_newer("1.10.0", Some("1.9.9")));
    assert!(!agent_install::is_newer("1.6.0", Some("1.6.0")));
    assert!(!agent_install::is_newer("1.6", Some("1.6.0")));
    assert!(!agent_install::is_newer("1.5.0", Some("1.6.0")));
    assert!(!agent_install::is_newer("1.6.0", None));
    assert!(!agent_install::is_newer("1.6.0", Some("unknown")));
}

#[tokio::test]
async fn keep_up_to_date_updates_only_a_copy_that_is_older_and_consented() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("autopass");
    let zip = zip_bytes(&[("pi.exe", b"MZ")]);
    let checksum = sha256_hex(&zip);
    let origin = serve_with(|origin| pi_routes(origin, &zip, &checksum)).await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    st.db.set_setting(&agent_install::consent_key("pi"), &json!({"sourceHost": "api.github.com", "version": "1.5.0"})).unwrap();
    // The switch off: nothing happens.
    assert!(agent_install::auto_update_once(&st).await.is_empty());
    // On, but nothing installed yet: keep-up-to-date never installs a first copy.
    st.db.set_setting(&agent_install::auto_update_key("pi"), &json!(true)).unwrap();
    assert!(agent_install::auto_update_once(&st).await.is_empty());
    assert!(retinue_runtime::managed_entry(&data.join("agents"), "pi").is_none());
    // Once a copy is installed at the publisher's version, a pass has nothing to do.
    let plan = agent_install::rpc(&st, "runtime.install.plan", &json!({"provider": "pi"})).await.unwrap();
    let started = agent_install::rpc(&st, "runtime.install", &json!({"provider": "pi", "confirm": true, "version": plan["plan"]["version"], "digestPrefix": plan["plan"]["digestPrefix"]})).await.unwrap();
    let finished = wait_for_job(&st, started["jobId"].as_str().unwrap()).await;
    assert_eq!(finished["phase"], "done", "{finished}");
    assert!(agent_install::auto_update_once(&st).await.is_empty());
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn a_second_install_for_the_same_provider_joins_the_first() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("onejob");
    let zip = zip_bytes(&[("pi.exe", b"MZ")]);
    let checksum = sha256_hex(&zip);
    let origin = serve_with(|origin| {
        let mut routes = pi_routes(origin, &zip, &checksum);
        routes[1].1 = Reply::slow(zip.clone());
        routes
    })
    .await;
    let _origin = Origin::set(&origin);
    st.db.set_setting(&agent_install::mode_key("pi"), &json!("manage")).unwrap();
    let plan = agent_install::plan_reply(&st, "pi").await.unwrap()["plan"].clone();
    let params = json!({"provider": "pi", "confirm": true, "version": plan["version"], "digestPrefix": plan["digestPrefix"]});
    let first = agent_install::rpc(&st, "runtime.install", &params).await.unwrap();
    let second = agent_install::rpc(&st, "runtime.install", &params).await.unwrap();
    assert_eq!(first["jobId"], second["jobId"], "one install per provider");
    assert_eq!(second["joined"], true);
    let _ = agent_install::rpc(&st, "runtime.install.cancel", &json!({"jobId": first["jobId"]})).await;
    let _ = wait_for_job(&st, first["jobId"].as_str().unwrap()).await;
    let _ = std::fs::remove_dir_all(&data);
}

// ---------------------------------------------------------------------------
// Removing Retinue's own copy

#[tokio::test]
async fn remove_deletes_only_the_managed_copy() {
    let _lock = lock().await;
    let (st, _rx, data) = state_with_data("remove");
    let root = data.join("agents");
    let folder = root.join("pi");
    std::fs::create_dir_all(folder.join("1.6.0")).unwrap();
    std::fs::write(folder.join("1.6.0").join("pi.exe"), b"MZ").unwrap();
    std::fs::write(folder.join("current.json"), json!({"version": "1.6.0", "entry": "pi.exe", "digest": "abc"}).to_string()).unwrap();
    let refused = agent_install::rpc(&st, "runtime.install.remove", &json!({"provider": "pi"})).await.unwrap_err();
    assert!(refused.1.message.contains("confirm: true"), "{}", refused.1.message);
    let removed = agent_install::rpc(&st, "runtime.install.remove", &json!({"provider": "pi", "confirm": true})).await.unwrap();
    assert_eq!(removed["removed"], true);
    assert_eq!(removed["kept"], json!([]));
    assert!(!folder.exists(), "nothing of the managed copy is left");
    // Nothing left to remove is `not_found`, not a silent success.
    let again = agent_install::rpc(&st, "runtime.install.remove", &json!({"provider": "pi", "confirm": true})).await.unwrap_err();
    assert_eq!(again.1.code, "not_found");
    let _ = std::fs::remove_dir_all(&data);
}

// ---------------------------------------------------------------------------
// The owner's own copy

#[test]
fn the_owner_of_an_install_is_read_from_its_path() {
    let root = if cfg!(windows) { "C:/Users/someone" } else { "/home/someone" };
    let npm = PathBuf::from(format!("{root}/AppData/Roaming/npm/claude.cmd"));
    let winget = PathBuf::from(format!("{root}/AppData/Local/Microsoft/WinGet/Links/codex.exe"));
    let own = PathBuf::from(format!("{root}/.local/bin/claude"));
    let system = PathBuf::from(if cfg!(windows) { "C:/Program Files/nodejs/claude.cmd" } else { "/usr/local/lib/node_modules/claude" });
    assert_eq!(agent_install::classify(&npm), "npm");
    assert_eq!(agent_install::classify(&winget), "winget");
    assert_eq!(agent_install::classify(&own), "own");
    assert_eq!(agent_install::classify(&system), "system");
    // A system-wide install is refused with one sentence and never elevated.
    let row = agent_install::ROWS.iter().find(|row| row.provider == "claude").unwrap();
    let refused = agent_install::adopt_command(row, &system);
    assert_eq!(refused.refusal, Some("This install belongs to a system-wide package manager; Retinue will not raise an admin prompt."));
    assert!(refused.args.is_empty(), "nothing is run for a system-wide install");
    // The CLI's own updater when nothing else owns the copy.
    let own_command = agent_install::adopt_command(row, &own);
    assert_eq!(own_command.refusal, None);
    assert_eq!(own_command.args, vec!["update".to_owned()]);
    // A WinGet link without a checked package id is refused rather than guessed.
    let winget_command = agent_install::adopt_command(row, &winget);
    assert!(winget_command.refusal.is_some_and(|sentence| sentence.contains("WinGet")));
    // An npm global install under the user's profile runs npm itself.
    let npm_command = agent_install::adopt_command(row, &npm);
    if retinue_runtime::resolve_command("npm").is_some() {
        assert_eq!(npm_command.refusal, None);
        assert_eq!(npm_command.args, vec!["install".to_owned(), "-g".to_owned(), "@anthropic-ai/claude-code@latest".to_owned()]);
    } else {
        assert!(npm_command.refusal.is_some_and(|sentence| sentence.contains("npm")));
    }
}

#[test]
fn an_installs_owner_is_read_the_same_way_with_unix_paths() {
    // Whichever platform runs this test reads the same shapes: bun and npm under
    // the home folder, and a distro install under /usr or /opt.
    for (path, owner) in [
        ("/home/someone/.bun/bin/claude", "npm"),
        ("/home/someone/.npm-global/bin/codex", "npm"),
        ("/usr/local/lib/node_modules/opencode/bin/opencode", "system"),
        ("/opt/claude/claude", "system"),
        ("/home/someone/.local/bin/pi", "own"),
    ] {
        assert_eq!(agent_install::classify(&PathBuf::from(path)), owner, "{path}");
    }
}

#[test]
fn the_managed_asset_is_the_one_this_platform_publishes() {
    let source = |provider: &str| agent_install::ROWS.iter().find(|row| row.provider == provider).and_then(|row| row.source);
    let asset = |provider: &str| match source(provider) {
        Some(agent_install::Source::GitHubAsset { asset, .. }) => asset,
        _ => panic!("{provider} is a GitHub asset"),
    };
    if cfg!(windows) {
        assert_eq!(asset("codex"), "codex-x86_64-pc-windows-msvc.exe");
        assert_eq!(asset("pi"), "pi-windows-x64.zip");
    } else {
        assert_eq!(asset("codex"), "codex-x86_64-unknown-linux-musl.tar.gz");
        assert_eq!(asset("pi"), "pi-linux-x64.tar.gz");
        assert!(matches!(source("opencode"), Some(agent_install::Source::NpmPlatform { platform: "opencode-linux-x64", .. })));
    }
}

#[cfg(unix)]
#[test]
fn a_published_binary_is_made_executable() {
    use std::os::unix::fs::PermissionsExt;
    let folder = std::env::temp_dir().join(format!("retinue-exec-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    let binary = folder.join("claude");
    std::fs::write(&binary, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o644)).unwrap();
    agent_install::make_executable(&binary);
    assert_ne!(std::fs::metadata(&binary).unwrap().permissions().mode() & 0o111, 0, "a managed copy can be launched");
    let _ = std::fs::remove_dir_all(&folder);
}

// ---------------------------------------------------------------------------
// The edges the module validates

#[test]
fn an_archive_name_never_becomes_a_path_outside_the_folder() {
    for good in ["pi.exe", "package/bin/pi.exe", "package\\bin\\pi.exe", "./pi.exe"] {
        assert!(agent_install::safe_entry_name(good).is_some(), "{good}");
    }
    for bad in ["../pi.exe", "/pi.exe", "C:/Windows/pi.exe", "package/../../pi.exe", "..\\pi.exe", "a\0b", ":"] {
        assert!(agent_install::safe_entry_name(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn a_checksum_line_and_an_npm_integrity_are_read_the_way_the_publishers_write_them() {
    let hex = "f5defb6c403ee698f5b3f8886db473e55b78aac670a8ce03620a47f5f0aec57c";
    assert_eq!(agent_install::sums_lookup(&format!("{hex}  pi-windows-x64.zip\n"), "pi-windows-x64.zip"), Some(hex.to_owned()));
    assert_eq!(agent_install::sums_lookup(&format!("{hex} *pi-windows-x64.zip\n"), "pi-windows-x64.zip"), Some(hex.to_owned()));
    assert_eq!(agent_install::sums_lookup(&format!("{hex}  other.zip\n"), "pi-windows-x64.zip"), None);
    assert_eq!(agent_install::sums_lookup("not a sums file", "x"), None);
    let bytes = b"the published tarball";
    let pin = agent_install::integrity_pin(&sha512_integrity(bytes)).expect("sha512 is read");
    assert_eq!(pin.algorithm, "sha512");
    assert_eq!(pin.prefix().len(), 12);
    assert_eq!(pin.hex, {
        use sha2::Digest as _;
        let digest = sha2::Sha512::digest(bytes);
        agent_install::hex_encode(&digest)
    });
    assert!(agent_install::integrity_pin("md5-deadbeef").is_none(), "an algorithm Retinue does not check is refused");
    assert!(agent_install::integrity_pin("sha512-not-base64").is_none());
}

#[test]
fn the_installer_own_last_line_is_redacted_and_bounded() {
    let line = agent_install::last_line("doing things\nnpm warn using --force\ntoken=deadbeefcafe in the way\n");
    assert!(line.len() <= 200);
    assert!(!line.contains("deadbeefcafe"), "a secret-shaped value is redacted: {line}");
    assert_eq!(agent_install::last_line("   \n\n"), "");
}

#[test]
fn a_url_is_allowed_only_over_https_from_a_named_host() {
    for url in [
        "https://downloads.claude.ai/claude-code-releases/stable",
        "https://registry.npmjs.org/opencode-ai/latest",
        "https://api.github.com/repos/openai/codex/releases/latest",
        "https://github.com/openai/codex/releases/download/rust-v0.171.0/codex.exe",
        "https://objects.githubusercontent.com/asset",
    ] {
        assert!(agent_install::allowed_url(&reqwest::Url::parse(url).unwrap()), "{url}");
    }
    for url in ["http://downloads.claude.ai/x", "https://example.com/x", "https://registry.npmjs.org.evil.example/x", "ftp://registry.npmjs.org/x"] {
        assert!(!agent_install::allowed_url(&reqwest::Url::parse(url).unwrap()), "{url}");
    }
}

#[test]
fn a_stored_choice_is_validated() {
    assert!(agent_install::validate_setting("agents.claude.mode", &json!("manage")).is_ok());
    assert!(agent_install::validate_setting("agents.claude.mode", &json!("adopt")).is_ok());
    assert!(agent_install::validate_setting("agents.claude.mode", &json!("maybe")).is_err());
    assert!(agent_install::validate_setting("agents.claude.autoUpdate", &json!(true)).is_ok());
    assert!(agent_install::validate_setting("agents.claude.autoUpdate", &json!("yes")).is_err());
    assert!(agent_install::validate_setting("agents.claude.consent", &json!({})).is_err(), "only the installer writes a consent");
    assert!(agent_install::validate_setting("agents.gemini.mode", &json!("manage")).is_err(), "an unknown provider");
    // A key outside `agents.` is not this module's business.
    assert!(agent_install::validate_setting("appearance.style", &json!("haze")).is_ok());
}

#[test]
fn an_install_is_never_reachable_from_a_device() {
    for method in ["runtime.install.plan", "runtime.install", "runtime.install.status", "runtime.install.cancel", "runtime.install.remove"] {
        let rule = policy::rule_for(method).unwrap_or_else(|| panic!("{method} has no tiers.json row"));
        assert_eq!(rule.class, MethodClass::Never, "{method}");
        assert_eq!(rule.min_tier, None, "{method}");
        // Exact rows, not a family wildcard: the parity test names them.
        assert_eq!(rule.method, method);
    }
    // The progress event never leaves the host.
    let dropped = policy::redact_event_for(&json!({"provider": "claude", "phase": "downloading", "version": "2.1.330", "bytesDone": 1, "bytesTotal": 2}), agent_install::EVENT);
    assert!(dropped.is_none(), "runtime.install.changed must be `drop` for every device");
}

#[test]
fn a_managed_copy_can_be_a_bare_file_a_zip_or_a_gzipped_tar() {
    let root = temp("kinds");
    std::fs::write(root.join("file.bin"), b"MZ bare").unwrap();
    let into = root.join("out");
    std::fs::create_dir_all(&into).unwrap();
    let name = agent_install::extract(&root.join("file.bin"), Kind::File, &into, &["claude.exe"], Some("win32-x64/claude.exe")).unwrap();
    assert_eq!(name, "claude.exe");
    assert_eq!(std::fs::read(into.join("claude.exe")).unwrap(), b"MZ bare");
    let archive = root.join("x.tgz");
    std::fs::write(&archive, tar_gz(&[("package/bin/opencode.exe", 0u8, b"MZ tar")])).unwrap();
    let name = agent_install::extract(&archive, Kind::TarGz, &into, &["opencode.exe"], None).unwrap();
    assert_eq!(name, "package/bin/opencode.exe");
    assert_eq!(std::fs::read(into.join("package").join("bin").join("opencode.exe")).unwrap(), b"MZ tar");
    // An archive without the executable is refused, not published empty.
    let empty = root.join("empty.zip");
    std::fs::write(&empty, zip_bytes(&[("readme.txt", b"nothing to run")])).unwrap();
    let refused = agent_install::extract(&empty, Kind::Zip, &into, &["pi.exe"], None).unwrap_err();
    assert!(refused.contains("did not contain"), "{refused}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_whole_pi_release_is_read_from_a_stub_document() {
    // The publisher reading itself, without an install: a GitHub release with a
    // digest field is the sha256 path Codex uses, and a sums file is Pi's.
    let text = "f5defb6c403ee698f5b3f8886db473e55b78aac670a8ce03620a47f5f0aec57c  pi-windows-x64.zip\n";
    assert_eq!(agent_install::sums_lookup(text, "pi-windows-x64.zip").unwrap().len(), 64);
}
