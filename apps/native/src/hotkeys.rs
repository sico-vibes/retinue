//! The two global shortcuts, through Windows' own `RegisterHotKey`.
//!
//! `Ctrl+Alt+B` shows and hides the companion; `Ctrl+E` starts and stops
//! dictation. The React app registers both through `tauri-plugin-global-shortcut`
//! (`set_companion_hotkey`, `set_dictation_shortcut`) and reports the same two
//! sentences when Windows refuses one.
//!
//! A hotkey belongs to the thread that registered it, and a windowless
//! `RegisterHotKey` posts `WM_HOTKEY` to that thread's queue, so this module owns
//! one thread with a message loop and nothing else. No window is created: a
//! message-only window would be the other way to do it, and it buys nothing when
//! the app already owns a UI thread.

#![allow(dead_code)]  // A ported surface: the helpers are complete and
// slice 6's settings page and slice 7 wire the rest.

use std::ptr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use async_channel::{Receiver as AsyncReceiver, Sender as AsyncSender};
use std::thread::JoinHandle;
use std::time::Duration;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT,
};

use crate::trace;

/// `WM_APP`: "something to do" on the hotkey thread, used to wake it.
const WM_WAKE: u32 = 0x8000;
/// How long a register call waits for the loop to answer.
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

/// The two shortcuts the app owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    Companion,
    Dictation,
}

impl Hotkey {
    pub const ALL: [Hotkey; 2] = [Hotkey::Companion, Hotkey::Dictation];

    /// The name the UI and the log use.
    pub fn label(self) -> &'static str {
        match self {
            Hotkey::Companion => "Ctrl+Alt+B",
            Hotkey::Dictation => "Ctrl+E",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Hotkey::Companion => "companion",
            Hotkey::Dictation => "dictation",
        }
    }

    /// Windows' own id for the shortcut, so `WM_HOTKEY` can be told apart.
    fn id(self) -> i32 {
        match self {
            Hotkey::Companion => 1,
            Hotkey::Dictation => 2,
        }
    }

    fn from_id(id: i32) -> Option<Self> {
        Hotkey::ALL.into_iter().find(|hotkey| hotkey.id() == id)
    }

    fn virtual_key(self) -> u32 {
        match self {
            // `VK_B` and `VK_E`.
            Hotkey::Companion => 0x42,
            Hotkey::Dictation => 0x45,
        }
    }

    fn modifiers(self) -> u32 {
        match self {
            Hotkey::Companion => MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            Hotkey::Dictation => MOD_CONTROL | MOD_NOREPEAT,
        }
    }

    /// The sentence the React app shows when Windows refuses the shortcut
    /// (`set_companion_hotkey`, `set_dictation_shortcut`).
    pub fn in_use_message(self) -> String {
        match self {
            Hotkey::Companion => "Ctrl+Alt+B is already in use.".to_owned(),
            Hotkey::Dictation => "Ctrl+E is in use by another app.".to_owned(),
        }
    }

    /// The line the shell log keeps when a shortcut could not be taken.
    pub fn unavailable_message(self) -> String {
        match self {
            Hotkey::Companion => "Retinue companion shortcut unavailable: already in use.".to_owned(),
            Hotkey::Dictation => "Dictation shortcut unavailable: already in use.".to_owned(),
        }
    }
}

enum Request {
    Register(Hotkey, Sender<Result<(), String>>),
    Unregister(Hotkey, Sender<Result<(), String>>),
}

/// The app's handle on the hotkey thread. Dropping it stops the thread and
/// releases both shortcuts.
pub struct HotkeyManager {
    requests: Sender<Request>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
    /// What Windows said when a shortcut could not be taken at start-up.
    pub refusals: Vec<String>,
}

/// A press of one of the app's shortcuts.
pub type Presses = AsyncReceiver<Hotkey>;

/// Register `wanted` and hand back the channel their presses arrive on.
///
/// A shortcut Windows refuses is reported in [`HotkeyManager::refusals`] and
/// does not stop the other one; the app keeps working without it, as it does
/// when the Tauri plugin's registration fails.
pub fn start(wanted: &[Hotkey]) -> (HotkeyManager, Presses) {
    let (press_tx, press_rx) = async_channel::unbounded::<Hotkey>();
    let (request_tx, request_rx) = mpsc::channel::<Request>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(u32, Vec<String>), String>>();
    let wanted = wanted.to_vec();

    let thread = std::thread::Builder::new()
        .name("retinue-hotkeys".to_owned())
        .spawn(move || run(wanted, request_rx, press_tx, ready_tx))
        .expect("the hotkey thread should start");

    let (thread_id, mut refusals) = match ready_rx.recv_timeout(REPLY_TIMEOUT) {
        Ok(Ok(ready)) => ready,
        Ok(Err(error)) => {
            trace::log(format!("hotkeys: {error}"));
            (0, vec![error])
        }
        Err(_) => {
            let message = "The shortcut thread did not answer.".to_owned();
            trace::log(format!("hotkeys: {message}"));
            (0, vec![message])
        }
    };
    refusals.shrink_to_fit();
    for refusal in &refusals {
        trace::log(format!("hotkeys: {refusal}"));
    }
    (HotkeyManager { requests: request_tx, thread_id, thread: Some(thread), refusals }, press_rx)
}

impl HotkeyManager {
    /// Take one shortcut. The React app calls this when dictation is switched on.
    pub fn register(&self, hotkey: Hotkey) -> Result<(), String> {
        self.ask(|reply| Request::Register(hotkey, reply))
    }

    /// Release one shortcut.
    pub fn unregister(&self, hotkey: Hotkey) -> Result<(), String> {
        self.ask(|reply| Request::Unregister(hotkey, reply))
    }

    fn ask(&self, build: impl FnOnce(Sender<Result<(), String>>) -> Request) -> Result<(), String> {
        if self.thread_id == 0 {
            return Err("The shortcut thread is not running.".to_owned());
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        self.requests
            .send(build(reply_tx))
            .map_err(|_| "The shortcut thread is not running.".to_owned())?;
        // Wake the loop: `GetMessageW` is parked on it, and the request is
        // already in the queue above.
        unsafe { PostThreadMessageW(self.thread_id, WM_WAKE, 0, 0) };
        match reply_rx.recv_timeout(REPLY_TIMEOUT) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err("The shortcut thread did not answer.".to_owned()),
            Err(RecvTimeoutError::Disconnected) => Err("The shortcut thread stopped.".to_owned()),
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        if self.thread_id != 0 {
            unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    wanted: Vec<Hotkey>,
    requests: Receiver<Request>,
    presses: AsyncSender<Hotkey>,
    ready: Sender<Result<(u32, Vec<String>), String>>,
) {
    let thread_id = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let mut refusals = Vec::new();
    for hotkey in wanted {
        if let Err(refusal) = register(hotkey) {
            refusals.push(refusal);
        }
    }
    if ready.send(Ok((thread_id, refusals))).is_err() {
        return;
    }

    loop {
        let mut message: MSG = unsafe { std::mem::zeroed() };
        let result = unsafe { GetMessageW(&mut message, ptr::null_mut::<std::ffi::c_void>(), 0, 0) };
        if result <= 0 {
            // 0 is WM_QUIT (the manager is being dropped); -1 is an error that
            // ends the loop just as surely.
            break;
        }
        match message.message {
            WM_HOTKEY => {
                if let Some(hotkey) = Hotkey::from_id(message.wParam as i32) {
                    // A dead receiver just means the app is closing.
                    let _ = presses.try_send(hotkey);
                }
            }
            WM_WAKE => {
                while let Ok(request) = requests.try_recv() {
                    match request {
                        Request::Register(hotkey, reply) => {
                            let _ = reply.send(register(hotkey));
                        }
                        Request::Unregister(hotkey, reply) => {
                            let _ = reply.send(unregister(hotkey));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    for hotkey in Hotkey::ALL {
        unsafe { UnregisterHotKey(ptr::null_mut::<std::ffi::c_void>(), hotkey.id()) };
    }
}

fn register(hotkey: Hotkey) -> Result<(), String> {
    let ok = unsafe {
        RegisterHotKey(
            ptr::null_mut::<std::ffi::c_void>(),
            hotkey.id(),
            hotkey.modifiers(),
            hotkey.virtual_key(),
        )
    };
    if ok == 0 {
        Err(hotkey.in_use_message())
    } else {
        trace::log(format!("hotkeys: {} registered", hotkey.label()));
        Ok(())
    }
}

fn unregister(hotkey: Hotkey) -> Result<(), String> {
    let ok = unsafe { UnregisterHotKey(ptr::null_mut::<std::ffi::c_void>(), hotkey.id()) };
    if ok == 0 {
        Err(format!("The {} shortcut could not be removed.", hotkey.label()))
    } else {
        trace::log(format!("hotkeys: {} released", hotkey.label()));
        Ok(())
    }
}

/// The parameters are `u32` because that is what Windows takes; keeping the
/// constant list here and a test on it catches a typo before it compiles.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_shortcut_says_exactly_what_the_react_app_says() {
        assert_eq!(Hotkey::Companion.in_use_message(), "Ctrl+Alt+B is already in use.");
        assert_eq!(Hotkey::Dictation.in_use_message(), "Ctrl+E is in use by another app.");
        assert!(Hotkey::Companion.unavailable_message().contains("companion shortcut unavailable"));
    }

    #[test]
    fn the_two_shortcuts_have_their_own_ids_keys_and_modifiers() {
        assert_eq!(Hotkey::Companion.id(), 1);
        assert_eq!(Hotkey::Dictation.id(), 2);
        assert_ne!(Hotkey::Companion.id(), Hotkey::Dictation.id());
        assert_eq!(Hotkey::from_id(1), Some(Hotkey::Companion));
        assert_eq!(Hotkey::from_id(9), None);
        // Ctrl+Alt+B and Ctrl+E: the letters Windows' virtual-key codes name.
        assert_eq!((Hotkey::Companion.virtual_key(), Hotkey::Dictation.virtual_key()), (0x42, 0x45));
        assert!(Hotkey::Companion.modifiers() & MOD_ALT != 0);
        assert!(Hotkey::Dictation.modifiers() & MOD_ALT == 0);
        assert!(Hotkey::Companion.modifiers() & MOD_CONTROL != 0);
        assert_eq!(Hotkey::Companion.label(), "Ctrl+Alt+B");
        assert_eq!(Hotkey::Dictation.label(), "Ctrl+E");
        assert_eq!(Hotkey::Companion.key(), "companion");
    }
}
