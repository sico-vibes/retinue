//! The window's root view: the daemon connection, the selection, and the three
//! columns. Panels live in `sidebar`, `conversations`, `conversation`, `team`,
//! `settings_page`, `proposal` and `dialogs`; the shared pieces in `menu`,
//! `composer`, `message`, `approvals`, `search`, `bots` and `usage`; `motion`
//! holds the animations.

pub mod approvals;
pub mod bots;
pub mod composer;
pub mod conversation;
pub mod conversations;
pub mod dialogs;
pub mod guards;
pub mod markdown;
pub mod menu;
pub mod message;
pub mod motion;
pub mod proposal;
pub mod root;
pub mod search;
pub mod settings_page;
pub mod sidebar;
pub mod team;
pub mod text_input;
pub mod usage;

pub use root::Root;
