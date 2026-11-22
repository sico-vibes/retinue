//! Desktop-only registration of the stable, token-free Retinue MCP command.

use super::*;
use retinue_runtime::ResolvedCommand;
use std::{fs, io, ops::Range, process::Stdio};

const OWNERSHIP: &str = "mcp.external.clients";
const CLI_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(test)]
type TestRunner = fn(Client, &Path, &[String]) -> Result<Vec<u8>, ()>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Client {
    Claude,
    Codex,
    OpenCode,
    Cursor,
}

const CLIENTS: [Client; 4] = [Client::Claude, Client::Codex, Client::OpenCode, Client::Cursor];

impl Client {
    fn id(self) -> &'static str {
        match self {
            Self::Claude => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Cursor => "cursor",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
            Self::Cursor => "Cursor",
        }
    }

    #[cfg(not(test))]
    fn cli(self) -> &'static str {
        if self == Self::Claude { "claude" } else { self.id() }
    }

    fn path(self, env: &Environment) -> Option<PathBuf> {
        let home = env.home.as_deref();
        match self {
            Self::Claude => env.claude_dir.as_deref().or(home).map(|dir| dir.join(".claude.json")),
            Self::Codex => env.codex_dir.clone().or_else(|| home.map(|dir| dir.join(".codex"))).map(|dir| dir.join("config.toml")),
            Self::Cursor => home.map(|dir| dir.join(".cursor/mcp.json")),
            Self::OpenCode => env.opencode_dir.as_ref().map(|dir| dir.join("opencode.json")),
        }
    }

    fn section(self) -> &'static [&'static str] {
        if self == Self::OpenCode { &["mcp", "servers"] } else { &["mcpServers"] }
    }

    fn expected(self, registration: &Value) -> Value {
        match self {
            Self::OpenCode => {
                let mut argv = vec![registration["command"].clone()];
                argv.extend(registration["args"].as_array().unwrap().iter().cloned());
                json!({"type":"local","command":argv})
            }
            Self::Cursor => {
                let mut entry = registration.clone();
                entry["type"] = json!("stdio");
                entry
            }
            _ => registration.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct Environment {
    home: Option<PathBuf>,
    claude_dir: Option<PathBuf>,
    codex_dir: Option<PathBuf>,
    opencode_dir: Option<PathBuf>,
    cursor_app: Option<PathBuf>,
    #[cfg(not(test))]
    managed_root: Option<PathBuf>,
    #[cfg(test)]
    runner: Option<TestRunner>,
}

impl Environment {
    fn for_state(st: &AppState) -> Self {
        #[cfg(test)]
        return st.external.client_fixture.lock().unwrap().clone().unwrap_or_default();
        #[cfg(not(test))]
        {
            let dir = |key| std::env::var_os(key).filter(|value| !value.is_empty()).map(PathBuf::from);
            let home = dir(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
            let opencode_dir = dir("OPENCODE_CONFIG_DIR").or_else(|| {
                dir("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|home| home.join(".config")))
                    .map(|config| config.join("opencode"))
            });
            let cursor_app = if cfg!(windows) {
                dir("LOCALAPPDATA").map(|dir| dir.join("Programs/cursor/Cursor.exe"))
            } else if cfg!(target_os = "macos") {
                Some(PathBuf::from("/Applications/Cursor.app/Contents/MacOS/Cursor"))
            } else {
                None
            };
            Self { home, claude_dir: dir("CLAUDE_CONFIG_DIR"), codex_dir: dir("CODEX_HOME"),
                opencode_dir, cursor_app, managed_root: crate::agent_install::managed_root(st) }
        }
    }

    fn executable(&self, client: Client) -> Option<ResolvedCommand> {
        #[cfg(test)]
        {
            let _ = client;
            None
        }
        #[cfg(not(test))]
        {
            retinue_runtime::resolve_command(client.cli()).filter(|command| {
                command.executable.is_absolute() && (client == Client::Cursor ||
                    !command.executable.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat")))
            }).or_else(|| {
                let root = self.managed_root.as_deref()?;
                retinue_runtime::managed_entry(root, client.cli()).map(|entry| entry.command)
            })
        }
    }

    fn installed(&self, client: Client) -> bool {
        #[cfg(test)]
        if client != Client::Cursor { return self.runner.is_some(); }
        self.executable(client).is_some() || (client == Client::Cursor && self.cursor_app.as_ref().is_some_and(|path| path.is_file()))
    }

    async fn run(&self, client: Client, path: &Path, args: &[String], read: bool) -> Result<Vec<u8>, ()> {
        #[cfg(test)]
        if let Some(run) = self.runner { return run(client, path, args); }
        let resolved = self.executable(client).ok_or(())?;
        let home = self.home.as_ref().ok_or(())?;
        let mut command = tokio::process::Command::new(&resolved.executable);
        retinue_process::prepare_command(&mut command);
        command.args(&resolved.args).args(args).current_dir(home)
            .stdin(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
        let config_dir = path.parent().ok_or(())?;
        if client == Client::Codex {
            command.env("CODEX_HOME", config_dir);
        } else if client == Client::OpenCode {
            command.env("OPENCODE_CONFIG_DIR", config_dir);
        } else if self.claude_dir.is_some() {
            command.env("CLAUDE_CONFIG_DIR", config_dir);
        }
        command.stdout(if read { Stdio::piped() } else { Stdio::null() });
        let mut child = command.spawn().map_err(|_| ())?;
        // On Windows prepare_command starts suspended so the job owns the
        // child before it can spawn anything. Attaching also resumes it.
        let tree = retinue_process::ProcessTree::attach(&mut child).map_err(|_| ())?;
        let output = match tokio::time::timeout(CLI_TIMEOUT, child.wait_with_output()).await {
            Ok(output) => output.map_err(|_| ())?,
            Err(_) => {
                let _ = tree.terminate();
                return Err(());
            }
        };
        output.status.success().then_some(output.stdout).ok_or(())
    }
}

fn internal(message: &str) -> DispatchError {
    derr("internal", message, StatusCode::INTERNAL_SERVER_ERROR)
}

fn ownership(st: &AppState) -> Result<Value, DispatchError> {
    let settings = st.db.settings().map_err(|_| internal("Retinue could not read its MCP registrations."))?;
    Ok(if settings[OWNERSHIP].is_object() { settings[OWNERSHIP].clone() } else { json!({}) })
}

// Use the data-folder copy on every distribution, including packaged builds.
// It stays outside WindowsApps and the install folder.
fn registration(st: &AppState) -> Option<Value> {
    let bridge = data_dir(st)?.join(BRIDGE_FILE);
    Some(json!({"command": bridge.to_str()?, "args": ["mcp-server", "--endpoint", endpoint_path(st)?.to_str()?]}))
}

fn matches(entry: &Value, expected: &Value) -> bool {
    if expected["type"] == "local" { return entry == expected; }
    entry["command"] == expected["command"] && entry["args"] == expected["args"]
        && entry.get("type").is_none_or(|kind| kind == "stdio")
}

fn read_json(path: &Path) -> Result<(Option<Vec<u8>>, Value), ()> {
    let original = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(_) => return Err(()),
    };
    let value = if let Some(bytes) = &original {
        serde_json::from_slice::<Value>(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)).map_err(|_| ())?
    } else {
        json!({})
    };
    if !value.is_object() { return Err(()); }
    Ok((original, value))
}

fn json_entry<'a>(doc: &'a Value, section: &[&str]) -> Result<Option<&'a Value>, ()> {
    let mut current = doc;
    for key in section {
        let Some(value) = current.get(*key) else { return Ok(None) };
        if !value.is_object() { return Err(()); }
        current = value;
    }
    Ok(current.get("retinue"))
}

struct Member {
    key: String,
    start: usize,
    value: Range<usize>,
}

// The whole document is already validated. Find member spans so edits preserve
// whitespace, key order, and even the original spelling of other JSON values.
fn members(bytes: &[u8], mut offset: usize) -> Result<Vec<Member>, ()> {
    fn skip(bytes: &[u8], at: usize) -> usize {
        at + bytes[at..].iter().take_while(|byte| byte.is_ascii_whitespace()).count()
    }

    let mut members: Vec<Member> = Vec::new();
    offset += 1;
    loop {
        let start = skip(bytes, offset);
        if bytes[start] == b'}' { return Ok(members); }
        let mut key = serde_json::Deserializer::from_slice(&bytes[start..]).into_iter::<String>();
        let name = key.next().ok_or(())?.map_err(|_| ())?;
        if members.iter().any(|member| member.key == name) { return Err(()); }
        let value_start = skip(bytes, skip(bytes, start + key.byte_offset()) + 1);
        let mut value = serde_json::Deserializer::from_slice(&bytes[value_start..]).into_iter::<Value>();
        value.next().ok_or(())?.map_err(|_| ())?;
        let end = value_start + value.byte_offset();
        members.push(Member { key: name, start, value: value_start..end });
        offset = skip(bytes, end);
        if bytes[offset] == b'}' { return Ok(members); }
        offset += 1;
    }
}

fn edit_json_bytes(original: Option<&[u8]>, client: Client, expected: &Value, connect: bool) -> Result<Vec<u8>, ()> {
    let bytes = original.unwrap_or(b"{}\n");
    let root = bytes.iter().position(|byte| *byte == b'{').ok_or(())?;
    let mut section = root;
    if client == Client::OpenCode {
        section = members(bytes, root)?.into_iter().find(|field| field.key == "mcp").ok_or(())?.value.start;
    }
    let fields = members(bytes, section)?;
    let servers = fields.iter().find(|field| field.key == if client == Client::OpenCode { "servers" } else { "mcpServers" });
    let (range, replacement) = if connect {
        let entry = format!("\"retinue\":{}", serde_json::to_string(expected).map_err(|_| ())?);
        let (start, fields, entry) = if let Some(servers) = servers {
            (servers.value.start, members(bytes, servers.value.start)?, entry)
        } else {
            (root, fields, format!("\"mcpServers\":{{{entry}}}"))
        };
        if let Some(last) = fields.last() {
            let end = last.value.end;
            let spacing = if fields.len() > 1 {
                let previous_end = fields[fields.len() - 2].value.end;
                let between = std::str::from_utf8(&bytes[previous_end..last.start]).map_err(|_| ())?;
                between.split_once(',').map_or("", |(_, space)| space)
            } else {
                std::str::from_utf8(&bytes[start + 1..last.start]).map_err(|_| ())?
            };
            (end..end, format!(",{spacing}{entry}"))
        } else {
            (start + 1..start + 1, entry)
        }
    } else {
        let fields = members(bytes, servers.ok_or(())?.value.start)?;
        let index = fields.iter().position(|field| field.key == "retinue").ok_or(())?;
        let field = &fields[index];
        let range = if index > 0 {
            fields[index - 1].value.end..field.value.end
        } else {
            field.start..fields.get(1).map_or(field.value.end, |next| next.start)
        };
        (range, String::new())
    };
    let mut updated = bytes.to_vec();
    updated.splice(range, replacement.bytes());
    Ok(updated)
}

async fn entry(env: &Environment, client: Client, path: &Path) -> Result<Option<Value>, ()> {
    if client != Client::Codex {
        let (_, doc) = read_json(path)?;
        if client == Client::OpenCode {
            // The verified CLI writes mcp.servers to opencode.json. Refuse
            // ambiguous user configs and preserve any legacy flat entry.
            for other in ["opencode.jsonc", "config.json"] {
                if path.with_file_name(other).try_exists().map_err(|_| ())? { return Err(()); }
            }
            if doc["mcp"].get("retinue").is_some() { return Ok(Some(Value::Null)); }
        }
        return Ok(json_entry(&doc, client.section())?.cloned());
    }
    // A missing user config needs no CLI probe, including on an uninstalled host.
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
        Ok(_) => {}
    }
    let args = ["mcp", "list", "--json"].map(str::to_owned);
    let output = env.run(client, path, &args, true).await?;
    let rows: Vec<Value> = serde_json::from_slice(&output).map_err(|_| ())?;
    Ok(rows.into_iter().find(|row| row["name"] == "retinue").map(|row| row["transport"].clone()))
}

fn read_error(client: Client, path: &Path) -> String {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("config file");
    format!("{}'s {name} could not be read.", client.name())
}

async fn row(st: &AppState, env: &Environment, client: Client, owners: &Value, expected: Option<&Value>) -> Value {
    let path = client.path(env).filter(|path| path.is_absolute());
    let mut error = st.external.client_errors.lock().unwrap().get(client.id()).cloned();
    let mut state = "not_connected";
    let mut owned = false;
    if let Some(path) = &path {
        match entry(env, client, path).await {
            Ok(Some(value)) => {
                if expected.is_some_and(|expected| matches(&value, &client.expected(expected))) {
                    state = "connected";
                    owned = owners[client.id()].as_str() == path.to_str();
                } else {
                    state = "different";
                }
            }
            Ok(None) => {}
            Err(()) => error = Some(read_error(client, path)),
        }
    }
    json!({
        "id": client.id(), "name": client.name(), "installed": env.installed(client),
        "state": state, "configPath": path.and_then(|path| path.to_str().map(str::to_owned)),
        "addedByRetinue": owned, "error": error,
    })
}

fn edit_config(client: Client, path: &Path, expected: &Value, connect: bool) -> Result<(), ()> {
    if fs::symlink_metadata(path).is_ok_and(|meta| !meta.file_type().is_file()) {
        return Err(());
    }
    let (original, doc) = read_json(path)?;
    let current = json_entry(&doc, client.section())?;
    if connect && current.is_some() || !connect && !current.is_some_and(|value| matches(value, expected)) {
        return Err(());
    }
    let parent = path.parent().ok_or(())?;
    fs::create_dir_all(parent).map_err(|_| ())?;
    let temp = parent.join(format!(".retinue-{}.tmp", uuid::Uuid::new_v4()));
    let file_name = path.file_name().and_then(|name| name.to_str()).ok_or(())?;
    let backup = path.with_file_name(format!("{file_name}.retinue-backup"));
    let updated = edit_json_bytes(original.as_deref(), client, expected, connect)?;
    let result = (|| {
        use std::io::Write;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|_| ())?;
        if connect || client == Client::OpenCode {
            if let Some(bytes) = &original {
                match options.open(&backup) {
                    Ok(mut saved) => {
                        if saved.write_all(bytes).and_then(|()| saved.sync_all()).is_err() {
                            drop(saved);
                            let _ = fs::remove_file(&backup);
                            return Err(());
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists &&
                        fs::symlink_metadata(&backup).is_ok_and(|meta| meta.file_type().is_file()) => {}
                    Err(_) => return Err(()),
                }
            }
        }
        file.write_all(&updated).and_then(|()| file.sync_all()).map_err(|_| ())?;
        drop(file);
        if read_json(path)?.0 != original { return Err(()); }
        fs::rename(&temp, path).map_err(|_| ())
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result
}

async fn change(env: &Environment, client: Client, path: &Path, registration: &Value, connect: bool) -> Result<(), ()> {
    // Cursor's --add-mcp writes the editor profile's settings.json rather
    // than .cursor/mcp.json. OpenCode 2 has a global add but no CLI remove.
    if client == Client::Cursor || client == Client::OpenCode && !connect {
        return edit_config(client, path, &client.expected(registration), connect);
    }
    let mut args = vec!["mcp".to_owned(), if connect { "add" } else { "remove" }.into()];
    if client == Client::Claude { args.extend(["--scope".into(), "user".into()]); }
    if client == Client::OpenCode { args.push("--global".into()); }
    args.push("retinue".into());
    if connect {
        args.push("--".into());
        args.push(registration["command"].as_str().ok_or(())?.into());
        for arg in registration["args"].as_array().ok_or(())? {
            args.push(arg.as_str().ok_or(())?.into());
        }
    }
    env.run(client, path, &args, false).await.map(|_| ())
}

async fn update(
    st: &AppState, env: &Environment, client: Client, owners: &mut Value, connect: bool,
) -> Result<Option<String>, DispatchError> {
    let Some(path) = client.path(env).filter(|path| path.is_absolute()) else {
        return Ok(Some(format!("{}'s user config path could not be found.", client.name())));
    };
    let path_text = path.to_str().ok_or_else(|| internal("The coding agent config path is not valid text."))?;
    let owned = owners[client.id()] == path_text;
    if !connect && !owned {
        return Ok(owners[client.id()].as_str().map(|_| {
            format!("{}'s user config location changed since Retinue connected it.", client.name())
        }));
    }
    let Some(registration) = registration(st) else {
        return Ok(Some("Retinue's data folder is not ready yet.".into()));
    };
    let expected = client.expected(&registration);
    let current = match entry(env, client, &path).await {
        Ok(current) => current,
        Err(()) => return Ok(Some(read_error(client, &path))),
    };
    if connect {
        if let Some(current) = current {
            if matches(&current, &expected) { return Ok(None); }
            return Ok(Some(format!("{} already has a different MCP entry named retinue.", client.name())));
        }
        if !owned && owners[client.id()].is_string() {
            return Ok(Some(format!("{}'s user config location changed since Retinue connected it.", client.name())));
        }
        if bridge_path(st).is_none() {
            return Ok(Some("The Retinue MCP bridge is not ready yet.".into()));
        }
    } else if !current.is_some_and(|entry| matches(&entry, &expected)) {
        owners.as_object_mut().unwrap().remove(client.id());
        store(st, OWNERSHIP, owners.clone())?;
        return Ok(None);
    }
    let result = change(env, client, &path, &registration, connect).await;
    // Check the actual config, including commands that time out after writing.
    let changed = match entry(env, client, &path).await {
        Ok(current) if connect => current.is_some_and(|entry| matches(&entry, &expected)),
        Ok(current) => current.is_none(),
        Err(()) => false,
    };
    if !changed {
        let verb = if connect { "connect" } else { "disconnect" };
        return Ok(Some(format!("Retinue could not {verb} {}.", client.name())));
    }
    if connect {
        owners[client.id()] = json!(path_text);
    } else {
        owners.as_object_mut().unwrap().remove(client.id());
    }
    store(st, OWNERSHIP, owners.clone())?;
    let (kind, verb, preposition) = if connect { ("external.client.connected", "Connected", "to") } else { ("external.client.disconnected", "Disconnected", "from") };
    crate::remote::audit(st, None, kind, &format!("{verb} the Retinue MCP {preposition} {}.", client.name())).await;
    Ok(result.err().map(|()| format!("{} reported an error while updating its MCP registration.", client.name())))
}

fn remember_error(st: &AppState, client: Client, error: Option<String>) {
    let mut errors = st.external.client_errors.lock().unwrap();
    errors.remove(client.id());
    if let Some(error) = error { errors.insert(client.id(), error); }
}

pub(super) async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    let env = Environment::for_state(st);
    let mut owners = ownership(st)?;
    let expected = registration(st);
    if method == "external.clients.list" {
        let mut rows = Vec::with_capacity(CLIENTS.len());
        for client in CLIENTS {
            rows.push(row(st, &env, client, &owners, expected.as_ref()).await);
        }
        return Ok(json!({"clients": rows}));
    }
    let client = CLIENTS.into_iter().find(|client| p["id"] == client.id())
        .ok_or_else(|| derr("invalid_argument", "Unknown coding agent.", StatusCode::BAD_REQUEST))?;
    let connect = method == "external.clients.connect";
    if connect {
        if p["confirm"] != true {
            return Err(derr("invalid_argument", "Confirm connecting the coding agent first.", StatusCode::BAD_REQUEST));
        }
        if !enabled(st) {
            return Err(derr("conflict", "Turn the Retinue MCP on first.", StatusCode::CONFLICT));
        }
        if !env.installed(client) {
            return Err(derr("conflict", &format!("{} is not installed.", client.name()), StatusCode::CONFLICT));
        }
    }
    let error = update(st, &env, client, &mut owners, connect).await?;
    remember_error(st, client, error);
    announce(st).await;
    Ok(json!({"client": row(st, &env, client, &owners, expected.as_ref()).await}))
}

pub(super) async fn disconnect_all(st: &AppState) -> Result<(), DispatchError> {
    let env = Environment::for_state(st);
    let mut owners = ownership(st)?;
    let mut failed = false;
    for client in CLIENTS {
        if owners[client.id()].is_null() { continue; }
        let error = update(st, &env, client, &mut owners, false).await?;
        failed |= error.is_some();
        remember_error(st, client, error);
    }
    if failed {
        announce(st).await;
        return Err(derr("conflict", "Retinue could not remove every coding agent registration.", StatusCode::CONFLICT));
    }
    Ok(())
}

#[cfg(test)]
#[path = "external_clients_tests.rs"]
mod tests;
