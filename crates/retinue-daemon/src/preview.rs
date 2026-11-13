//! `preview.servers`: the local dev servers a conversation started.
//!
//! The owner wants the dock's preview to draw the page a bot's `npm run dev`
//! served, so the daemon has to say which loopback servers belong to which
//! conversation. Attribution is by process, never by guessing a port: a listener
//! counts for a conversation when its process descends from that conversation's
//! coding-agent process, which the daemon asks its own adapter for
//! (`AgentAdapter::process_id`).
//!
//! The rules kept here:
//!
//! * Loopback only (`127.0.0.1` and `[::1]`). A wildcard or LAN bind belongs to
//!   an application of its own and is never listed, probed or returned.
//! * One `GET /` per address, short timeout, no cookies, no proxy and no redirect
//!   followed away from loopback. Nothing from the conversation is ever sent, and
//!   the URL is built here from a port: a page cannot choose what is requested.
//! * A process shared by two conversations (OpenCode and Codex serve several
//!   conversations from one process) is attributed to neither: showing the owner
//!   another conversation's server would be worse than showing none.
//! * Scanning runs only while a client asked for a conversation's servers within
//!   the last minute, and at most every two seconds, so an idle app runs no
//!   `netstat` at all.
//!
//! The terminal dock's shells are children of the native shell (`pty.rs`), not of
//! this process, so a server started by hand in the dock is not attributed yet.
//! That needs the native side to report `{ sessionId, pid }`; it is recorded in
//! `docs/ipc-contract.md` and not built here.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use retinue_agent_core::AgentAdapter;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// A client that asked for a conversation's servers is watching for this long.
const WATCH_WINDOW: Duration = Duration::from_secs(60);
/// The fastest the answer is recomputed while anybody is watching.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// One probe is one `GET /`, short enough that several listeners cost one scan.
const PROBE_TIMEOUT: Duration = Duration::from_millis(700);
/// A listener's answer is reused this long, so a two-second scan is not a storm.
const PROBE_TTL: Duration = Duration::from_secs(5);
/// A stuck `netstat` must never hold a scan, or a request, open.
const NETSTAT_TIMEOUT: Duration = Duration::from_secs(5);
/// `netstat -ano` prints about 80 bytes a row; this is far above any real table.
const NETSTAT_MAX_BYTES: usize = 4 * 1024 * 1024;
/// A process tree is a few hops deep; the bound only stops a malformed table.
const MAX_ANCESTRY: usize = 64;

/// What the preview scan remembers between calls: who is watching, what each
/// conversation was told last, and the answers to recent probes. Memory only.
pub(crate) struct Preview {
    /// The last time a client asked for each conversation's servers.
    watching: Mutex<HashMap<String, Instant>>,
    /// The last answer per conversation, so `preview.changed` means something
    /// changed rather than "two seconds passed".
    answered: Mutex<HashMap<String, Vec<Value>>>,
    /// One probe answer per address, briefly.
    probes: Mutex<HashMap<String, (Instant, bool)>>,
    /// One scan at a time, whichever caller starts it.
    scan: tokio::sync::Mutex<()>,
    /// The client every probe uses. `None` when it could not be built, and then
    /// nothing is probed: a page is never claimed on no evidence.
    client: Option<reqwest::Client>,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            watching: Mutex::new(HashMap::new()),
            answered: Mutex::new(HashMap::new()),
            probes: Mutex::new(HashMap::new()),
            scan: tokio::sync::Mutex::new(()),
            client: probe_client(),
        }
    }
}

/// The one HTTP client every probe uses: a loopback request Retinue makes, never
/// one a page or a conversation shapes. No redirect is followed, no proxy is
/// consulted (a system proxy would carry the request off the machine), there is
/// no cookie store, and the User-Agent is Retinue's own.
fn probe_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(PROBE_TIMEOUT)
        .timeout(PROBE_TIMEOUT)
        .user_agent(concat!("Retinue/", env!("CARGO_PKG_VERSION")))
        .build()
        .ok()
}

/// One listening TCP socket, as the operating system reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listener {
    /// The bind address exactly as reported, for example `127.0.0.1`.
    pub host: String,
    pub port: u16,
    pub pid: u32,
}

/// What the process table says about one process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessRow {
    pub parent: u32,
    /// The image name, for example `node.exe`.
    pub name: String,
}

// ---------------------------------------------------------------------------
// The RPC

/// `preview.servers { sessionId }` → `{ sessionId, servers }`.
pub(crate) async fn servers(st: &AppState, p: &Value) -> Result<Value, DispatchError> {
    let session_id = p["sessionId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            derr(
                "invalid_argument",
                "sessionId is required",
                StatusCode::BAD_REQUEST,
            )
        })?;
    // Reading the row is the existence check: an unknown or deleted conversation
    // is an error, not an empty list.
    st.db
        .session_brief(session_id)
        .map_err(|_| derr("not_found", "session not found", StatusCode::NOT_FOUND))?;
    watch(st, session_id, Instant::now());
    Ok(json!({ "sessionId": session_id, "servers": refresh(st, session_id).await }))
}

/// Recomputes the watched conversations' answers every two seconds and announces
/// a change. With nobody watching, one pass costs two `Mutex` locks.
pub(crate) async fn sweep_loop(st: AppState) {
    loop {
        tokio::time::sleep(SCAN_INTERVAL).await;
        let watched = watched_now(&st);
        // Forget the answers of conversations nobody asked about, so the next
        // question always starts from what is true then.
        {
            let mut answered = st.preview.answered.lock().unwrap_or_else(|error| error.into_inner());
            answered.retain(|id, _| watched.contains(id));
        }
        for session_id in watched {
            refresh(&st, &session_id).await;
        }
    }
}

/// Records that a client is watching this conversation now.
fn watch(st: &AppState, session_id: &str, now: Instant) {
    let mut watching = st.preview.watching.lock().unwrap_or_else(|error| error.into_inner());
    watching.insert(session_id.to_owned(), now);
    prune_watching(&mut watching, now);
}

/// The conversations asked about within the window.
fn watched_now(st: &AppState) -> Vec<String> {
    let now = Instant::now();
    let mut watching = st.preview.watching.lock().unwrap_or_else(|error| error.into_inner());
    prune_watching(&mut watching, now);
    watching.keys().cloned().collect()
}

/// Drops the conversations nobody has asked about within the last minute.
fn prune_watching(watching: &mut HashMap<String, Instant>, now: Instant) {
    watching.retain(|_, at| now.saturating_duration_since(*at) < WATCH_WINDOW);
}

/// One conversation's answer, computed now and remembered. A real change since
/// the last answer is announced with `preview.changed`, which carries no content.
async fn refresh(st: &AppState, session_id: &str) -> Vec<Value> {
    let servers = scan(st, session_id).await;
    let changed = {
        let mut answered = st.preview.answered.lock().unwrap_or_else(|error| error.into_inner());
        answered.insert(session_id.to_owned(), servers.clone()).as_ref() != Some(&servers)
    };
    if changed {
        st.emit("preview.changed", json!({ "sessionId": session_id })).await;
    }
    servers
}

/// The servers one conversation owns right now, sorted by port.
async fn scan(st: &AppState, session_id: &str) -> Vec<Value> {
    let Some(root) = roots(st).await.remove(session_id) else { return Vec::new() };
    let (listeners, table) = snapshot(st).await;
    let mut servers = Vec::new();
    for listener in owned(&listeners, &table, root) {
        // The probe address is built from the port and the listener's own
        // loopback host; no script or conversation supplies a URL.
        let host = if listener.host == "[::1]" { "[::1]" } else { "127.0.0.1" };
        let url = format!("http://{host}:{}/", listener.port);
        let answered_html = probe(st, &url).await;
        servers.push(json!({
            "port": listener.port,
            "url": url,
            "pid": listener.pid,
            "process": table.get(&listener.pid).map(|row| row.name.clone()),
            "answeredHtml": answered_html,
        }));
    }
    servers
}

// ---------------------------------------------------------------------------
// Attribution

/// The provider process serving each live conversation, for the ones the daemon
/// can name. A pid claimed by two conversations is left out: it is one process
/// serving both, and attributing it to either would show the owner a server that
/// may not be that conversation's.
async fn roots(st: &AppState) -> HashMap<String, u32> {
    let sessions: Vec<(String, Arc<dyn AgentAdapter>)> = {
        let active = st.sessions.lock().await;
        active.iter().map(|(id, session)| (id.clone(), session.adapter.clone())).collect()
    };
    let mut claims: HashMap<u32, Vec<String>> = HashMap::new();
    for (session_id, adapter) in sessions {
        if let Some(pid) = adapter.process_id(&session_id).await {
            claims.entry(pid).or_default().push(session_id);
        }
    }
    only_claims(claims)
}

/// One root per conversation, dropping a process two conversations share.
fn only_claims(claims: HashMap<u32, Vec<String>>) -> HashMap<String, u32> {
    claims
        .into_iter()
        .filter(|(_, sessions)| sessions.len() == 1)
        .map(|(pid, mut sessions)| (sessions.remove(0), pid))
        .collect()
}

/// Whether `pid` is `root` or descends from it, following the parent links the
/// process table gives. A cycle or a missing row ends the walk: a process that
/// cannot be traced is not attributed to anybody.
fn descends_from(table: &HashMap<u32, ProcessRow>, pid: u32, root: u32) -> bool {
    let mut current = pid;
    for _ in 0..MAX_ANCESTRY {
        if current == root {
            return true;
        }
        let Some(row) = table.get(&current) else { return false };
        if row.parent == 0 || row.parent == current {
            return false;
        }
        current = row.parent;
    }
    false
}

/// The loopback listeners that descend from `root`, one per port: the first
/// reported address wins, and `netstat` lists the IPv4 table first.
fn owned(listeners: &[Listener], table: &HashMap<u32, ProcessRow>, root: u32) -> Vec<Listener> {
    let mut owned: Vec<Listener> = Vec::new();
    for listener in listeners {
        if !descends_from(table, listener.pid, root) {
            continue;
        }
        if owned.iter().any(|seen| seen.port == listener.port) {
            continue;
        }
        owned.push(listener.clone());
    }
    owned.sort_by_key(|listener| listener.port);
    owned
}

// ---------------------------------------------------------------------------
// The operating system's facts

/// One snapshot of the machine at a time across every conversation: `netstat`
/// is the expensive part, and two watchers asking at once share one answer.
async fn snapshot(st: &AppState) -> (Vec<Listener>, HashMap<u32, ProcessRow>) {
    let _scan = st.preview.scan.lock().await;
    machine().await
}

/// The listening TCP sockets and the process table, in one pass.
async fn machine() -> (Vec<Listener>, HashMap<u32, ProcessRow>) {
    let mut raw = String::new();
    for protocol in ["tcp", "tcpv6"] {
        if let Some(text) = netstat(protocol).await {
            raw.push_str(&text);
            raw.push('\n');
        }
    }
    let table = tokio::task::spawn_blocking(process_table).await.unwrap_or_default();
    (parse_netstat(&raw), table)
}

/// Reads `netstat -ano -p <protocol>` output. A listening TCP socket is a row
/// whose foreign address is the zero port, which is the one marker `netstat`
/// prints the same way in every Windows language. Only a loopback bind is kept:
/// `0.0.0.0:5173` and a LAN address are somebody else's listener.
fn parse_netstat(raw: &str) -> Vec<Listener> {
    let mut listeners: Vec<Listener> = Vec::new();
    for line in raw.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 || !fields[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        // A listening socket has the wildcard foreign port; an established one
        // names the peer's port.
        if !fields[2].ends_with(":0") {
            continue;
        }
        let Some((host, port)) = fields[1].rsplit_once(':') else { continue };
        let Ok(port) = port.parse::<u16>() else { continue };
        if port == 0 || !is_loopback_host(host) {
            continue;
        }
        let Ok(pid) = fields[fields.len() - 1].parse::<u32>() else { continue };
        if pid == 0 {
            continue;
        }
        let listener = Listener { host: host.to_owned(), port, pid };
        if !listeners.contains(&listener) {
            listeners.push(listener);
        }
    }
    listeners.sort_by_key(|listener| listener.port);
    listeners
}

/// `127.0.0.1` or `[::1]`, and nothing else. The wildcard binds (`0.0.0.0`,
/// `[::]`) are deliberately left out: they answer on the network too.
fn is_loopback_host(host: &str) -> bool {
    host == "127.0.0.1" || host == "[::1]"
}

/// `netstat -ano -p <protocol>` from `System32` rather than `PATH`, so a folder
/// inside a project cannot stand in for it. Bounded in time and size, no shell,
/// and the output is only ever parsed.
#[cfg(windows)]
async fn netstat(protocol: &str) -> Option<String> {
    let root = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("C:\\Windows"));
    let mut command = tokio::process::Command::new(root.join("System32").join("netstat.exe"));
    command
        .args(["-ano", "-p", protocol])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x0800_0000);
    let output = tokio::time::timeout(NETSTAT_TIMEOUT, command.output()).await.ok()?.ok()?;
    if !output.status.success() || output.stdout.len() > NETSTAT_MAX_BYTES {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
async fn netstat(_protocol: &str) -> Option<String> {
    None
}

/// Every process's parent and image name. A failure answers an empty table, and
/// then nothing is attributed: a listener is never guessed at.
#[cfg(windows)]
fn process_table() -> HashMap<u32, ProcessRow> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        },
    };
    let mut table = HashMap::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot.is_null() || snapshot as isize == -1 {
            return table;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Process32FirstW(snapshot, &mut entry) != 0;
        while found {
            let name = String::from_utf16_lossy(&entry.szExeFile);
            let name = name.trim_end_matches('\0').to_owned();
            table.insert(entry.th32ProcessID, ProcessRow { parent: entry.th32ParentProcessID, name });
            found = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    table
}

#[cfg(not(windows))]
fn process_table() -> HashMap<u32, ProcessRow> {
    HashMap::new()
}

// ---------------------------------------------------------------------------
// The probe

/// Whether this address answered with a page, asking once and reusing a recent
/// answer so a scan every two seconds stays cheap.
async fn probe(st: &AppState, url: &str) -> bool {
    {
        let probes = st.preview.probes.lock().unwrap_or_else(|error| error.into_inner());
        if let Some((at, answer)) = probes.get(url) {
            if at.elapsed() < PROBE_TTL {
                return *answer;
            }
        }
    }
    let Some(client) = st.preview.client.as_ref() else { return false };
    let answer = probe_answer(client, url).await;
    let mut probes = st.preview.probes.lock().unwrap_or_else(|error| error.into_inner());
    probes.insert(url.to_owned(), (Instant::now(), answer));
    probes.retain(|_, (at, _)| at.elapsed() < WATCH_WINDOW);
    answer
}

/// One `GET /`. The body is never read; only the status and content type decide.
async fn probe_answer(client: &reqwest::Client, url: &str) -> bool {
    match client.get(url).send().await {
        Ok(response) => answers_html(&response),
        Err(_) => false,
    }
}

/// Whether a reply is a page the preview could draw: a 2xx with an HTML media
/// type, or a redirect with somewhere to go. Anything else (JSON, a plain-text
/// 404, a binary protocol, a refusal) is not a page.
fn answers_html(response: &reqwest::Response) -> bool {
    let status = response.status();
    if status.is_redirection() {
        return response.headers().contains_key(reqwest::header::LOCATION);
    }
    if !status.is_success() {
        return false;
    }
    let media = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    media == "text/html" || media == "application/xhtml+xml"
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Real `netstat -ano` output from this machine (11 November 2026), with a
    /// dev server added on a loopback port and one on a LAN address.
    const NETSTAT: &str = "Active Connections\n\n  Proto  Local Address          Foreign Address        State           PID\n  TCP    0.0.0.0:22             0.0.0.0:0              LISTENING       3924\n  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1768\n  TCP    100.79.66.84:443       0.0.0.0:0              LISTENING       7740\n  TCP    100.79.66.84:60755     100.73.164.27:22       ESTABLISHED     19348\n  TCP    127.0.0.1:587          0.0.0.0:0              LISTENING       18352\n  TCP    127.0.0.1:5173         0.0.0.0:0              LISTENING       9020\n  TCP    192.168.1.10:3000      0.0.0.0:0              LISTENING       9021\n\n  Proto  Local Address          Foreign Address        State           PID\n  TCP    [::]:22                [::]:0                 LISTENING       3924\n  TCP    [::1]:8080             [::]:0                 LISTENING       9022\n  TCP    [::1]:8080             [::]:0                 LISTENING       9022\n";

    fn table(rows: &[(u32, u32, &str)]) -> HashMap<u32, ProcessRow> {
        rows.iter()
            .map(|(pid, parent, name)| (*pid, ProcessRow { parent: *parent, name: (*name).to_owned() }))
            .collect()
    }

    #[test]
    fn parsing_keeps_only_loopback_listeners_and_reads_each_once() {
        let listeners = parse_netstat(NETSTAT);
        assert_eq!(
            listeners,
            vec![
                Listener { host: "127.0.0.1".into(), port: 587, pid: 18352 },
                Listener { host: "127.0.0.1".into(), port: 5173, pid: 9020 },
                Listener { host: "[::1]".into(), port: 8080, pid: 9022 },
            ]
        );
    }

    #[test]
    fn a_wildcard_or_lan_bind_is_never_a_candidate() {
        // 0.0.0.0:22, 192.168.1.10:3000 and 100.79.66.84:443 are all in the
        // sample; an established connection on the LAN is not a listener either.
        let listeners = parse_netstat(NETSTAT);
        for port in [22u16, 3000, 443] {
            assert!(listeners.iter().all(|listener| listener.port != port), "port {port} was listed");
        }
        assert!(parse_netstat("  TCP    127.0.0.1:5173     127.0.0.1:52000        ESTABLISHED     12\n").is_empty());
        assert!(parse_netstat("").is_empty());
        assert!(parse_netstat("Active Connections\n").is_empty());
    }

    #[test]
    fn attribution_follows_the_process_tree() {
        // 100 is the agent, 101 the shell it ran, 102 the dev server.
        let processes = table(&[
            (100, 40, "opencode.exe"),
            (101, 100, "node.exe"),
            (102, 101, "node.exe"),
            (200, 40, "node.exe"),
        ]);
        let listeners = vec![
            Listener { host: "127.0.0.1".into(), port: 9000, pid: 200 },
            Listener { host: "127.0.0.1".into(), port: 5173, pid: 102 },
            Listener { host: "127.0.0.1".into(), port: 5173, pid: 102 },
        ];
        assert_eq!(owned(&listeners, &processes, 100), vec![Listener { host: "127.0.0.1".into(), port: 5173, pid: 102 }]);
        assert!(owned(&listeners, &processes, 200).iter().all(|listener| listener.pid == 200));
        // A process nobody owns, a missing row and a cycle are all "no".
        assert!(owned(&listeners, &processes, 999).is_empty());
        assert!(!descends_from(&processes, 5, 100));
        assert!(!descends_from(&table(&[(7, 8, "a.exe"), (8, 7, "b.exe")]), 7, 100));
    }

    #[test]
    fn a_process_two_conversations_share_is_attributed_to_neither() {
        let claims = HashMap::from([
            (100u32, vec!["one".to_string(), "two".to_string()]),
            (200, vec!["three".to_string()]),
        ]);
        assert_eq!(only_claims(claims), HashMap::from([("three".to_string(), 200u32)]));
    }

    #[test]
    fn watching_lapses_after_a_minute() {
        let start = Instant::now();
        let mut watching = HashMap::from([("one".to_string(), start), ("two".to_string(), start)]);
        prune_watching(&mut watching, start + WATCH_WINDOW - Duration::from_secs(1));
        assert_eq!(watching.len(), 2, "a conversation asked about a minute ago is still watched");
        prune_watching(&mut watching, start + WATCH_WINDOW + Duration::from_secs(1));
        assert!(watching.is_empty(), "nobody asked for a minute, so the scan stops");
    }

    #[tokio::test]
    async fn an_idle_loop_scans_nothing_and_announces_nothing() {
        use retinue_protocol::EventEnvelope;
        let (st, mut events): (AppState, tokio::sync::broadcast::Receiver<EventEnvelope>) = crate::phase2a_tests::state();
        let sweep = tokio::spawn(sweep_loop(st.clone()));
        tokio::time::sleep(SCAN_INTERVAL + Duration::from_millis(250)).await;
        sweep.abort();
        let watching = st.preview.watching.lock().unwrap_or_else(|error| error.into_inner());
        let answered = st.preview.answered.lock().unwrap_or_else(|error| error.into_inner());
        assert!(watching.is_empty(), "no client asked, so nothing is watched");
        assert!(answered.is_empty(), "no conversation was scanned, so nothing was announced");
        assert!(events.try_recv().is_err(), "an idle loop emits no event");
    }

    #[test]
    fn the_rpc_is_refused_from_the_remote_gateway() {
        use crate::remote::policy::{authorize, Decision, RemoteCall, Tier};
        let call = RemoteCall {
            tier: Tier::Operator,
            approvals_from_remote: true,
            method: "preview.servers".into(),
            params: json!({"sessionId": "s"}),
            quiet_hours: false,
        };
        assert!(matches!(authorize(&call), Decision::Denied { .. }));
        assert_eq!(
            crate::remote::policy::rule_for("preview.servers").map(|rule| rule.class),
            Some(crate::remote::policy::MethodClass::Never)
        );
    }

    #[test]
    fn the_change_event_never_reaches_a_device() {
        let event = json!({"type": "preview.changed", "payload": {"sessionId": "s"}});
        assert!(crate::remote::policy::redact_event(&event).is_none());
    }

    #[test]
    fn an_html_answer_is_a_page_and_anything_else_is_not() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let client = probe_client().expect("probe client");
            let (page, redirect, silent, plain, followed) = four_servers().await;
            assert!(probe_answer(&client, &page).await, "200 text/html is a page");
            assert!(probe_answer(&client, &redirect).await, "a redirect with a target is a page");
            assert_eq!(followed.load(std::sync::atomic::Ordering::SeqCst), 0, "the redirect was not followed");
            assert!(!probe_answer(&client, &plain).await, "200 text/plain is not a page");
            let started = Instant::now();
            assert!(!probe_answer(&client, &silent).await, "a server that never answers is not a page");
            assert!(started.elapsed() < Duration::from_secs(3), "the probe gave up in time");
        });
    }

    #[test]
    fn the_probe_asks_once_for_the_root_and_sends_nothing() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let client = probe_client().expect("probe client");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (received, answer) = tokio::sync::oneshot::channel::<String>();
            tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = vec![0u8; 4096];
                let read = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer).await.unwrap();
                let _ = received.send(String::from_utf8_lossy(&buffer[..read]).into_owned());
                let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 5\r\n\r\nhello").await;
            });
            let url = format!("http://127.0.0.1:{}/", address.port());
            assert!(probe_answer(&client, &url).await);
            let request = answer.await.unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get / http/1.1\r\n"), "one GET of the root: {request:?}");
            assert!(!request.contains("cookie"), "no cookie is sent: {request:?}");
            assert!(request.contains("user-agent: retinue/"), "the User-Agent is ours: {request:?}");
        });
    }

    /// Four one-shot servers: an HTML page, a redirect, a silent one and a
    /// plain-text one. The redirect's target counts its own connections, so a
    /// followed redirect would be visible.
    async fn four_servers() -> (String, String, String, String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_port = target.local_addr().unwrap().port();
        let followed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = followed.clone();
        tokio::spawn(async move {
            while let Ok((_socket, _)) = target.accept().await {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let mut urls = Vec::new();
        for answer in [
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 2\r\n\r\nok".to_string(),
            format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/\r\nContent-Length: 0\r\n\r\n"),
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nok".to_string(),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            tokio::spawn(async move {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let mut buffer = vec![0u8; 4096];
                    let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer).await;
                    let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, answer.as_bytes()).await;
                }
            });
            urls.push(format!("http://127.0.0.1:{port}/"));
        }
        // A server that accepts and then says nothing at all.
        let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let silent_port = silent.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = silent.accept().await {
                held.push(socket);
            }
        });
        let page = urls[0].clone();
        let redirect = urls[1].clone();
        let plain = urls[2].clone();
        (page, redirect, format!("http://127.0.0.1:{silent_port}/"), plain, followed)
    }
}
