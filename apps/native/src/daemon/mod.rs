//! Starting, talking to and stopping the daemon, off the UI thread.
//!
//! One worker thread owns a small tokio runtime: it starts `bitbotsd.exe`, takes
//! the first snapshot, follows the event stream, and runs every RPC the window
//! asks for. The window only ever reads from one channel, so nothing in the UI
//! can block on the daemon.

pub mod client;
pub mod ready;
pub mod supervisor;
pub mod worker;

pub use worker::{
    start, Command, DaemonHandle, DaemonState, PromptAttachment, PromptMethod, Update, BACKOFF_MS,
};
