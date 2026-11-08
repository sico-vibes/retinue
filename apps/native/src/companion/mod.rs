//! The companion: the second, floating window and everything it decides.
//!
//! Ported from `apps/desktop/src/ui/Companion*.tsx`, `companion*.ts`,
//! `src/blob/companionFsm.ts`, `companionLayout.ts` and `src-tauri/src/lib.rs`
//! (`make_companion`, `set_companion_mode`, the position and resize maths).
//! Haze only, as the brief says.
//!
//! The window reads the same daemon snapshot and events as the main window
//! through [`shared::Shared`]; it never opens a connection of its own.

pub mod attention;
pub mod drop;
pub mod fsm;
pub mod layout;
pub mod peek;
pub mod positions;
pub mod settings;
pub mod shared;
pub mod status;
pub mod view;
pub mod window;

pub use fsm::Mode;
pub use settings::CompanionSettings;
pub use shared::Shared;
pub use view::Companion;
