//! The shells behind the terminal dock, through Windows ConPTY
//! (`portable-pty`), ported from `apps/desktop/src-tauri/src/pty.rs`.
//!
//! Sessions live in memory only: no transcript is written anywhere. A shell is
//! only ever started in a folder that resolves inside the conversation's project,
//! only the three fixed shells are offered, and no Retinue secret reaches a
//! shell's environment. Terminal output is never logged.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

/// A shell the dock offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    PowerShell,
    PowerShell7,
    CommandPrompt,
}

impl ShellKind {
    pub fn id(self) -> &'static str {
        match self {
            ShellKind::PowerShell => "powershell",
            ShellKind::PowerShell7 => "pwsh",
            ShellKind::CommandPrompt => "cmd",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShellKind::PowerShell => "PowerShell",
            ShellKind::PowerShell7 => "PowerShell 7",
            ShellKind::CommandPrompt => "Command Prompt",
        }
    }

    fn executable(self) -> (&'static str, &'static [&'static str]) {
        match self {
            ShellKind::PowerShell => ("powershell.exe", &["-NoLogo"]),
            ShellKind::PowerShell7 => ("pwsh.exe", &["-NoLogo"]),
            ShellKind::CommandPrompt => ("cmd.exe", &[]),
        }
    }
}

/// The terminals a person may have open at once, as the installed app allows.
pub const MAX_TERMINALS: usize = 6;
const MAX_COLS: u16 = 500;
const MAX_ROWS: u16 = 300;
const MAX_WRITE_BYTES: usize = 64 * 1024;
const MAX_PATH_BYTES: usize = 4_096;

/// What the reading thread tells the window.
#[derive(Clone, Debug)]
pub enum PtyEvent {
    Output { id: String, data: String },
    Exit { id: String, code: Option<i32> },
}

/// One running shell.
struct Session {
    id: String,
    #[allow(dead_code)]
    pid: Option<u32>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    ended: AtomicBool,
}

impl Session {
    fn stop(&self) {
        if self.ended.swap(true, Ordering::AcqRel) {
            return;
        }
        // `files::spawn_guard` allows no program to be started here (not even
        // `taskkill`), so a closed tab kills the shell itself; a command the
        // shell started may outlive it, which the report says.
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn sessions() -> &'static Mutex<HashMap<String, Arc<Session>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<Session>>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct Opened {
    pub id: String,
    pub shell: ShellKind,
    pub cwd: String,
}

/// The project folder, canonicalised, so a shell can never start outside it.
fn project_dir(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("The project folder could not be found.".to_owned());
    }
    if path.len() > MAX_PATH_BYTES {
        return Err("The project path is too long.".to_owned());
    }
    let canonical = PathBuf::from(path).canonicalize().map_err(|_| "The project folder could not be found.".to_owned())?;
    if !canonical.is_dir() {
        return Err("The project path is not a folder.".to_owned());
    }
    Ok(canonical)
}

/// The plain path a shell expects: no `\\?\` prefix, and never a UNC path.
pub fn user_path(path: &Path) -> Result<PathBuf, String> {
    let text = path.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        let mut characters = rest.chars();
        let drive = characters.next().is_some_and(|letter| letter.is_ascii_alphabetic()) && characters.next() == Some(':');
        if rest.starts_with(r"UNC\") || !drive {
            return Err("That folder is not on a local drive.".to_owned());
        }
        return Ok(PathBuf::from(rest));
    }
    if text.starts_with(r"\\") {
        return Err("That folder is not on a local drive.".to_owned());
    }
    Ok(path.to_path_buf())
}

/// No Retinue secret may reach a user shell's environment.
fn sanitize(command: &mut CommandBuilder) {
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        let upper = name.to_ascii_uppercase();
        if upper.starts_with("RETINUE_") || upper.starts_with("RETINUE_") || upper == "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS" {
            command.env_remove(name.as_ref());
        }
    }
}

pub fn check_size(cols: u16, rows: u16) -> Result<(), String> {
    if cols == 0 || rows == 0 || cols > MAX_COLS || rows > MAX_ROWS {
        return Err("The terminal size is outside the supported range.".to_owned());
    }
    Ok(())
}

/// Start one shell in the project folder, and hand its output to `events`.
pub fn open(project_path: &str, shell: ShellKind, cols: u16, rows: u16, events: async_channel::Sender<PtyEvent>) -> Result<Opened, String> {
    let root = project_dir(project_path)?;
    check_size(cols, rows)?;
    {
        let mut map = sessions().lock().map_err(|_| "The terminal service is unavailable.".to_owned())?;
        map.retain(|_, session| !session.ended.load(Ordering::Acquire));
        if map.len() >= MAX_TERMINALS {
            return Err("Close a terminal before opening another one.".to_owned());
        }
    }
    let cwd = user_path(&root)?;
    let pair = native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|_| {
        "A terminal could not be opened on this device.".to_owned()
    })?;
    let (exe, args) = shell.executable();
    let mut command = CommandBuilder::new(exe);
    command.args(args);
    command.cwd(cwd.as_path());
    sanitize(&mut command);
    let mut child = pair.slave.spawn_command(command).map_err(|_| "The selected shell could not be started.".to_owned())?;
    let pid = child.process_id();
    drop(pair.slave);
    let reader = match (pair.master.try_clone_reader(), pair.master.take_writer()) {
        (Ok(reader), Ok(writer)) => (reader, writer),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The terminal could not be started.".to_owned());
        }
    };
    let (reader, writer) = reader;
    let id = uuid::Uuid::new_v4().to_string();
    let session = Arc::new(Session {
        id: id.clone(),
        pid,
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
        ended: AtomicBool::new(false),
    });
    sessions()
        .lock()
        .map_err(|_| "The terminal service is unavailable.".to_owned())?
        .insert(id.clone(), Arc::clone(&session));
    spawn_reader(reader, Arc::clone(&session), events);
    Ok(Opened { id, shell, cwd: cwd.to_string_lossy().to_string() })
}

/// Read the pty until it ends. A partial UTF-8 sequence is carried to the next
/// read, so a split character never becomes garbage.
fn spawn_reader(mut reader: Box<dyn Read + Send>, session: Arc<Session>, events: async_channel::Sender<PtyEvent>) {
    std::thread::Builder::new()
        .name(format!("terminal-{}", &session.id[..8.min(session.id.len())]))
        .spawn(move || {
            let mut buffer = vec![0_u8; 8 * 1024];
            let mut tail: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let text = decode_chunk(&mut tail, &buffer[..count]);
                        if text.is_empty() {
                            continue;
                        }
                        if events.send_blocking(PtyEvent::Output { id: session.id.clone(), data: text }).is_err() {
                            break;
                        }
                    }
                }
            }
            if !tail.is_empty() {
                let _ = events.send_blocking(PtyEvent::Output { id: session.id.clone(), data: "\u{fffd}".to_owned() });
            }
            let code = {
                let mut child = session.child.lock().unwrap_or_else(|error| error.into_inner());
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) => break Some(status.exit_code() as i32),
                        Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                        _ => {
                            let _ = child.kill();
                            break child.wait().ok().map(|status| status.exit_code() as i32);
                        }
                    }
                }
            };
            session.ended.store(true, Ordering::Release);
            let _ = events.send_blocking(PtyEvent::Exit { id: session.id.clone(), code });
        })
        .ok();
}

/// Decode what arrived, keeping an incomplete trailing sequence for next time.
pub fn decode_chunk(pending: &mut Vec<u8>, incoming: &[u8]) -> String {
    pending.extend_from_slice(incoming);
    let mut output = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(valid) => {
                output.push_str(valid);
                pending.clear();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if let Ok(text) = std::str::from_utf8(&pending[..valid]) {
                    output.push_str(text);
                }
                match error.error_len() {
                    Some(length) => {
                        output.push('\u{fffd}');
                        pending.drain(..valid + length);
                    }
                    None => {
                        pending.drain(..valid);
                        break;
                    }
                }
            }
        }
    }
    output
}

pub fn write(id: &str, data: &str) -> Result<(), String> {
    if data.len() > MAX_WRITE_BYTES {
        return Err("Terminal input is too large.".to_owned());
    }
    let session = lookup(id)?;
    if session.ended.load(Ordering::Acquire) {
        return Err("The terminal has closed.".to_owned());
    }
    if data.is_empty() {
        return Ok(());
    }
    let mut writer = session.writer.lock().map_err(|_| "The terminal input is unavailable.".to_owned())?;
    writer.write_all(data.as_bytes()).and_then(|()| writer.flush()).map_err(|_| "The terminal has closed.".to_owned())
}

pub fn resize(id: &str, cols: u16, rows: u16) -> Result<(), String> {
    check_size(cols, rows)?;
    let session = lookup(id)?;
    if session.ended.load(Ordering::Acquire) {
        return Ok(());
    }
    let master = session.master.lock().map_err(|_| "The terminal is unavailable.".to_owned())?;
    master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|_| "The terminal could not be resized.".to_owned())
}

pub fn close(id: &str) {
    let session = sessions().lock().ok().and_then(|mut map| map.remove(id));
    if let Some(session) = session {
        session.stop();
    }
}

/// The window is closing: no shell outlives it. (The window's own exit path is
/// the Director's; a process exit closes the ConPTY handles anyway.)
#[allow(dead_code)]
pub fn shutdown() {
    let all: Vec<Arc<Session>> = sessions().lock().map(|mut map| map.drain().map(|(_, session)| session).collect()).unwrap_or_default();
    for session in all {
        session.stop();
    }
}

fn lookup(id: &str) -> Result<Arc<Session>, String> {
    if id.is_empty() || id.len() > 64 {
        return Err("The terminal has closed.".to_owned());
    }
    sessions()
        .lock()
        .map_err(|_| "The terminal service is unavailable.".to_owned())?
        .get(id)
        .cloned()
        .ok_or_else(|| "The terminal has closed.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_shells_have_stable_ids() {
        assert_eq!(ShellKind::PowerShell.id(), "powershell");
        assert_eq!(ShellKind::PowerShell7.id(), "pwsh");
        assert_eq!(ShellKind::CommandPrompt.id(), "cmd");
    }

    #[test]
    fn a_size_outside_the_supported_range_is_refused() {
        assert!(check_size(80, 24).is_ok());
        assert!(check_size(0, 24).is_err());
        assert!(check_size(80, 0).is_err());
        assert!(check_size(501, 24).is_err());
        assert!(check_size(80, 301).is_err());
    }

    #[test]
    fn a_split_utf8_character_is_carried_to_the_next_read() {
        let bytes = "héllo".as_bytes();
        let (first, second) = bytes.split_at(2);
        let mut tail = Vec::new();
        let mut out = decode_chunk(&mut tail, first);
        out.push_str(&decode_chunk(&mut tail, second));
        assert_eq!(out, "héllo");
        assert!(tail.is_empty());
    }

    #[test]
    fn an_invalid_byte_becomes_the_replacement_character_rather_than_losing_the_line() {
        let mut tail = Vec::new();
        let out = decode_chunk(&mut tail, &[b'a', 0xff, b'b']);
        assert_eq!(out, "a\u{fffd}b");
    }

    #[test]
    fn a_project_folder_that_does_not_exist_is_refused() {
        assert!(project_dir("").is_err());
        assert!(project_dir("D:/this/folder/does/not/exist/at/all").is_err());
    }

    #[test]
    fn a_verbatim_prefix_is_dropped_for_the_shell_and_unc_is_refused() {
        assert_eq!(user_path(Path::new(r"\\?\D:\work")).unwrap(), PathBuf::from(r"D:\work"));
        assert!(user_path(Path::new(r"\\?\UNC\server\share")).is_err());
        assert!(user_path(Path::new(r"\\server\share")).is_err());
    }

    #[test]
    fn a_terminal_id_that_is_not_one_is_refused() {
        assert!(write("", "x").is_err());
        assert!(resize("", 80, 24).is_err());
        assert!(lookup(&"x".repeat(65)).is_err());
    }

    #[test]
    fn too_much_input_is_refused_before_the_shell_sees_it() {
        let big = "x".repeat(MAX_WRITE_BYTES + 1);
        assert!(write("unknown", &big).is_err());
    }
}
