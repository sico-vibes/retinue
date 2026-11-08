//! The window's root view: the daemon connection, the selection, and the three
//! columns. Panels live in `sidebar`, `conversations` and `conversation`.

pub mod conversation;
pub mod conversations;
pub mod markdown;
pub mod root;
pub mod sidebar;
pub mod text_input;

pub use root::Root;
