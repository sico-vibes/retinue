//! Keeps the computer awake while Retinue has work, without fighting the user.
//!
//! The UI decides when a keep-awake request is wanted and calls
//! [`set_keep_awake`]; this module owns the native request. On Windows the
//! execution state is per-thread, so a single dedicated thread holds
//! `ES_CONTINUOUS | ES_SYSTEM_REQUIRED` and stays parked until the request is
//! released; on Linux the request is logind's idle inhibitor, held by a file
//! descriptor. The display is deliberately left free to sleep on both. Nothing
//! here stops the user from putting the computer to sleep themselves (closing the
//! lid, Start menu, power button): it only asks the idle sleep timer not to fire
//! while Retinue is working.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, State};

/// One platform call that holds or releases the keep-awake request.
///
/// The pure lifecycle in [`KeepAwake`] is written against this trait so it can
/// be unit-tested with a recording fake instead of touching the real OS.
pub trait PowerBackend: Send + Sync {
    fn set(&self, enabled: bool) -> Result<(), String>;
}

/// Tracks whether a keep-awake request is held and drives the backend.
pub struct KeepAwake {
    backend: Arc<dyn PowerBackend>,
    enabled: Mutex<bool>,
}

impl KeepAwake {
    pub fn new(backend: Arc<dyn PowerBackend>) -> Self {
        Self {
            backend,
            enabled: Mutex::new(false),
        }
    }

    /// Enables or disables the request and reports the state now in force.
    /// Enabling while already enabled and disabling while already disabled are
    /// no-ops. A backend failure is reported without changing the state.
    pub fn set(&self, enabled: bool) -> Result<bool, String> {
        let mut state = self
            .enabled
            .lock()
            .map_err(|_| "Keep-awake state is unavailable.".to_string())?;
        if *state == enabled {
            return Ok(*state);
        }
        self.backend.set(enabled)?;
        *state = enabled;
        Ok(*state)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.lock().map(|state| *state).unwrap_or(false)
    }

    /// Releases the request if one is held. Used on application exit; the OS
    /// also drops the per-thread request when the worker thread dies.
    pub fn release(&self) {
        let _ = self.set(false);
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        let _ = self.set(false);
    }
}

/// The managed Tauri state: one keep-awake owner for the whole app.
pub struct PowerState {
    keeper: KeepAwake,
}

impl Default for PowerState {
    fn default() -> Self {
        Self {
            keeper: KeepAwake::new(default_backend()),
        }
    }
}

impl PowerState {
    fn set(&self, enabled: bool) -> Result<bool, String> {
        self.keeper.set(enabled)
    }

    fn is_enabled(&self) -> bool {
        self.keeper.is_enabled()
    }

    /// Releases any held request when the application is exiting.
    pub fn release(&self) {
        self.keeper.release();
    }
}

/// Holds or releases the keep-awake request and returns the state in force.
///
/// The native call may talk to another process (logind over D-Bus on Linux), so
/// it runs on the blocking pool and never on the window's main thread.
#[tauri::command]
pub async fn set_keep_awake(app: AppHandle, enabled: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<PowerState>();
        state.set(enabled)
    })
    .await
    .map_err(|_| "Keep-awake state is unavailable.".to_string())?
}

/// Reports whether Retinue currently holds a keep-awake request.
#[tauri::command]
pub fn keep_awake_state(state: State<'_, PowerState>) -> bool {
    state.is_enabled()
}

#[cfg(windows)]
fn default_backend() -> Arc<dyn PowerBackend> {
    Arc::new(Win32Power::default())
}

#[cfg(target_os = "linux")]
fn default_backend() -> Arc<dyn PowerBackend> {
    Arc::new(linux::LogindInhibit::default())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn default_backend() -> Arc<dyn PowerBackend> {
    Arc::new(NoopPower)
}

/// Builds with neither native request available keep the flag so the command
/// surface behaves identically.
#[cfg(not(any(windows, target_os = "linux")))]
struct NoopPower;

#[cfg(not(any(windows, target_os = "linux")))]
impl PowerBackend for NoopPower {
    fn set(&self, _enabled: bool) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn SetThreadExecutionState(flags: u32) -> u32;
}

#[cfg(windows)]
const ES_CONTINUOUS: u32 = 0x8000_0000;
#[cfg(windows)]
const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

/// The real Windows backend. The execution state belongs to the thread that
/// set it, so enabling starts a dedicated thread that parks until released.
#[cfg(windows)]
#[derive(Default)]
struct Win32Power {
    parked: Mutex<Option<Parked>>,
}

#[cfg(windows)]
struct Parked {
    commands: Sender<Command>,
    join: Option<JoinHandle<()>>,
}

#[cfg(windows)]
enum Command {
    Release(Sender<Result<(), String>>),
}

#[cfg(windows)]
impl PowerBackend for Win32Power {
    fn set(&self, enabled: bool) -> Result<(), String> {
        let mut parked = self
            .parked
            .lock()
            .map_err(|_| "Keep-awake state is unavailable.".to_string())?;
        if enabled {
            if parked.is_some() {
                return Ok(());
            }
            let (commands, receiver) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let join = thread::Builder::new()
                .name("retinue-keep-awake".to_string())
                .spawn(move || run_keep_awake(receiver, ready_tx))
                .map_err(|error| format!("Could not start the keep-awake thread: {error}"))?;
            match ready_rx.recv() {
                Ok(Ok(())) => {
                    *parked = Some(Parked {
                        commands,
                        join: Some(join),
                    });
                    Ok(())
                }
                Ok(Err(error)) => {
                    let _ = join.join();
                    Err(error)
                }
                Err(_) => {
                    let _ = join.join();
                    Err("The keep-awake thread stopped before it started.".to_string())
                }
            }
        } else {
            if let Some(held) = parked.take() {
                let (done_tx, done_rx) = mpsc::channel();
                if held.commands.send(Command::Release(done_tx)).is_ok() {
                    let _ = done_rx.recv();
                }
                if let Some(join) = held.join {
                    let _ = join.join();
                }
            }
            Ok(())
        }
    }
}

/// Runs on the dedicated thread: acquire the request, then stay alive (and
/// therefore keep holding it) until asked to release or the manager is dropped.
#[cfg(windows)]
fn run_keep_awake(receiver: Receiver<Command>, ready: Sender<Result<(), String>>) {
    let acquired = set_thread_execution_state(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
    let failed = acquired.is_err();
    let _ = ready.send(acquired);
    if failed {
        return;
    }
    // One release command at most: the first one arrives, the thread hands the
    // request back and ends, which is what drops the execution state.
    if let Ok(Command::Release(reply)) = receiver.recv() {
        let _ = reply.send(set_thread_execution_state(ES_CONTINUOUS));
        return;
    }
    let _ = set_thread_execution_state(ES_CONTINUOUS);
}

/// A successful call returns the previous execution state (always including
/// `ES_CONTINUOUS`); only a failure returns zero.
#[cfg(windows)]
fn set_thread_execution_state(flags: u32) -> Result<(), String> {
    if unsafe { SetThreadExecutionState(flags) } == 0 {
        Err("Windows refused the keep-awake request.".to_string())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
use std::sync::mpsc::{self, Receiver, Sender};
#[cfg(windows)]
use std::thread::{self, JoinHandle};

/// The Linux backend: logind's `Inhibit` on the system bus.
///
/// `what = "idle"` is the counterpart of `ES_SYSTEM_REQUIRED`: it stops the idle
/// action (a suspend when the machine goes idle) and says nothing about the
/// display or an explicit suspend. The method hands back a file descriptor that
/// login1 owns; dropping it releases the inhibitor, so the descriptor is the
/// whole state.
///
/// Chosen over `systemd-inhibit` as a child process because there is no extra
/// process (nothing to show a console window, nothing to orphan if the app is
/// killed), and over the session bus because a keep-awake request is a
/// machine-wide one that logind, not a session manager, has to hear.
#[cfg(target_os = "linux")]
mod linux {
    use super::PowerBackend;
    use std::sync::Mutex;
    use std::time::Duration;
    use zbus::zvariant::OwnedFd;

    /// Only the idle timer is inhibited; the screen may sleep, as on Windows.
    const WHAT: &str = "idle";
    const WHO: &str = "Retinue";
    const WHY: &str = "Retinue is working";
    const MODE: &str = "block";
    /// A hung system bus must not hold the command for long; the state is
    /// reported as a failure instead.
    const WAIT: Duration = Duration::from_secs(2);

    const UNREACHABLE: &str = "This computer's power service is not available, so Retinue cannot stay awake.";
    const REFUSED: &str = "The system refused the keep-awake request.";

    #[derive(Default)]
    pub(super) struct LogindInhibit {
        held: Mutex<Option<Held>>,
    }

    /// What a held request consists of.
    struct Held {
        /// The inhibitor pipe login1 handed out; closing it releases the lock.
        _fd: OwnedFd,
        /// The bus connection the lock was taken on. Kept open with the request
        /// because logind may also release a lock when its client leaves the bus,
        /// and a keep-awake request must not depend on which rule it applies.
        _connection: zbus::blocking::Connection,
    }

    impl PowerBackend for LogindInhibit {
        fn set(&self, enabled: bool) -> Result<(), String> {
            let mut held = self
                .held
                .lock()
                .map_err(|_| "Keep-awake state is unavailable.".to_string())?;
            if !enabled {
                // Closing the descriptor (on drop) releases the inhibitor.
                *held = None;
                return Ok(());
            }
            if held.is_some() {
                return Ok(());
            }
            *held = Some(inhibit()?);
            Ok(())
        }
    }

    /// Asks login1 for an idle inhibitor and keeps the descriptor it hands back.
    fn inhibit() -> Result<Held, String> {
        let connection = zbus::blocking::connection::Builder::system()
            .map_err(|_| UNREACHABLE.to_string())?
            .method_timeout(WAIT)
            .build()
            .map_err(|_| UNREACHABLE.to_string())?;
        let reply = connection
            .call_method(
                Some("org.freedesktop.login1"),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "Inhibit",
                &(WHAT, WHO, WHY, MODE),
            )
            .map_err(|_| REFUSED.to_string())?;
        let fd = reply
            .body()
            .deserialize::<OwnedFd>()
            .map_err(|_| REFUSED.to_string())?;
        Ok(Held {
            _fd: fd,
            _connection: connection,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingBackend {
        calls: Mutex<Vec<bool>>,
        fail_on: Mutex<Option<bool>>,
    }

    impl RecordingBackend {
        fn calls(&self) -> Vec<bool> {
            self.calls.lock().unwrap().clone()
        }

        fn fail_on(&self, value: bool) {
            *self.fail_on.lock().unwrap() = Some(value);
        }

        fn stop_failing(&self) {
            *self.fail_on.lock().unwrap() = None;
        }
    }

    impl PowerBackend for RecordingBackend {
        fn set(&self, enabled: bool) -> Result<(), String> {
            if *self.fail_on.lock().unwrap() == Some(enabled) {
                return Err("the backend refused the request".to_string());
            }
            self.calls.lock().unwrap().push(enabled);
            Ok(())
        }
    }

    fn keeper(backend: &Arc<RecordingBackend>) -> KeepAwake {
        KeepAwake::new(Arc::clone(backend) as Arc<dyn PowerBackend>)
    }

    #[test]
    fn enabling_twice_calls_the_backend_once() {
        let backend = Arc::new(RecordingBackend::default());
        let keeper = keeper(&backend);
        assert_eq!(keeper.set(true), Ok(true));
        assert_eq!(keeper.set(true), Ok(true));
        assert_eq!(backend.calls(), vec![true]);
    }

    #[test]
    fn disabling_when_off_is_a_no_op() {
        let backend = Arc::new(RecordingBackend::default());
        let keeper = keeper(&backend);
        assert_eq!(keeper.set(false), Ok(false));
        assert!(backend.calls().is_empty());
    }

    #[test]
    fn state_is_reported_and_toggles_with_the_backend() {
        let backend = Arc::new(RecordingBackend::default());
        let keeper = keeper(&backend);
        assert!(!keeper.is_enabled());
        assert_eq!(keeper.set(true), Ok(true));
        assert!(keeper.is_enabled());
        assert_eq!(keeper.set(false), Ok(false));
        assert!(!keeper.is_enabled());
        assert_eq!(backend.calls(), vec![true, false]);
    }

    #[test]
    fn dropping_a_held_request_releases_it() {
        let backend = Arc::new(RecordingBackend::default());
        {
            let keeper = keeper(&backend);
            assert_eq!(keeper.set(true), Ok(true));
        }
        assert_eq!(backend.calls(), vec![true, false]);
    }

    #[test]
    fn dropping_without_a_request_touches_nothing() {
        let backend = Arc::new(RecordingBackend::default());
        {
            let _keeper = keeper(&backend);
        }
        assert!(backend.calls().is_empty());
    }

    #[test]
    fn a_failed_enable_reports_an_error_without_holding_the_request() {
        let backend = Arc::new(RecordingBackend::default());
        backend.fail_on(true);
        let keeper = keeper(&backend);
        assert!(keeper.set(true).is_err());
        assert!(!keeper.is_enabled());
        assert!(backend.calls().is_empty());
        backend.stop_failing();
        assert_eq!(keeper.set(true), Ok(true));
        assert!(keeper.is_enabled());
    }

    #[test]
    fn a_failed_disable_reports_an_error_and_keeps_the_request() {
        let backend = Arc::new(RecordingBackend::default());
        let keeper = keeper(&backend);
        assert_eq!(keeper.set(true), Ok(true));
        backend.fail_on(false);
        assert!(keeper.set(false).is_err());
        assert!(keeper.is_enabled());
        backend.stop_failing();
        assert_eq!(keeper.set(false), Ok(false));
        assert!(!keeper.is_enabled());
        assert_eq!(backend.calls(), vec![true, false]);
    }

    /// The Linux backend must not talk to the system bus for a request it does
    /// not hold: a machine with no logind at all still answers "off".
    #[cfg(target_os = "linux")]
    #[test]
    fn releasing_an_unheld_logind_request_touches_no_bus() {
        let backend = linux::LogindInhibit::default();
        assert_eq!(backend.set(false), Ok(()));
    }
}
