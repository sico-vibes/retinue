//! The window's root view: the daemon connection, the selection, and the three
//! columns. Panels live in `sidebar`, `conversations`, `conversation`, `team`,
//! `settings_page`, `proposal` and `dialogs`; `motion` holds the animations.

pub mod conversation;
pub mod conversations;
pub mod dialogs;
pub mod markdown;
pub mod motion;
pub mod proposal;
pub mod root;
pub mod settings_page;
pub mod sidebar;
pub mod team;
pub mod text_input;

pub use root::Root;
