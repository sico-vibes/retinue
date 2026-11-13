//! Which coding agents Retinue offers (owner decision, 11 November 2026).
//!
//! Claude Code, Codex and OpenCode are the **main** providers and are always on.
//! Every other provider Retinue knows is **optional** and off until the owner
//! turns it on in Settings.
//!
//! The switch is the existing one: `runtimes.disabled` already holds provider ids
//! and runtime ids, the Settings › Coding agents toggle writes it, and
//! `settings.set` is desktop-only (never remote). Keeping one list means the two
//! pages cannot disagree. The only new key is [`DEFAULTS_MARKER`], written once so
//! the defaults are not re-applied over the owner's choices.
//!
//! **Upgrade rule.** An install that already has an employee or a conversation on
//! an optional provider keeps that provider on: the defaults are computed against
//! the bots and conversations that exist, and never switch off something the owner
//! already uses.

use retinue_storage::Storage;
use serde_json::{json, Value};

use crate::{AppState, DispatchError};

/// The one-time marker that records that the optional providers were switched
/// off. A `migration.*` key, so a backup never carries it and `settings.set`
/// refuses it.
pub(crate) const DEFAULTS_MARKER: &str = "migration.providerTogglesApplied";

/// Whether a provider is one of the three that are always on.
pub(crate) fn is_main_provider(provider: &str) -> bool {
    retinue_runtime::is_main_provider(provider)
}

/// Every provider Retinue looks for, in display order (the main three first).
pub(crate) fn known_providers() -> Vec<&'static str> {
    retinue_runtime::discovery_ids().to_vec()
}

/// The providers that are off until the owner turns them on.
pub(crate) fn optional_providers() -> Vec<&'static str> {
    retinue_runtime::optional_providers()
}

/// The switch list exactly as stored, main providers included. Used when the
/// defaults are computed, because the list is rewritten from it.
fn raw_disabled(db: &Storage) -> Vec<String> {
    db.settings().ok().and_then(|settings| settings["runtimes.disabled"].as_array().map(|ids| ids.iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())).unwrap_or_default()
}

/// The switch list as it counts: a main provider is never off, whatever the
/// stored list says (an older install or a restored backup may name one).
pub(crate) fn disabled_ids(db: &Storage) -> Vec<String> {
    raw_disabled(db).into_iter().filter(|id| !is_main_provider(id)).collect()
}

/// The main provider a switch list names, if it names one. `settings.set` and
/// backup import refuse it rather than silently dropping it.
pub(crate) fn main_provider_named(value: &Value) -> Option<&'static str> {
    let ids = value.as_array()?;
    retinue_runtime::MAIN_PROVIDERS.into_iter().find(|main| ids.iter().any(|id| id.as_str() == Some(main)))
}

/// Whether the provider itself (not one installed copy) is switched off.
pub(crate) fn provider_is_off(db: &Storage, provider: &str) -> bool {
    !provider.is_empty() && disabled_ids(db).iter().any(|id| id == provider)
}

/// `runtimes.disabled` for a new install: every optional provider is off, unless
/// a bot or a conversation already runs on it. Written once; the marker says so.
pub(crate) fn apply_defaults(db: &Storage) -> Result<(), retinue_storage::StorageError> {
    let settings = db.settings()?;
    if settings.get(DEFAULTS_MARKER).is_some() {
        return Ok(());
    }
    let mut disabled = raw_disabled(db);
    let used = used_providers(db);
    let mut changed = false;
    for provider in optional_providers() {
        // Nothing the owner already uses may disappear.
        if used.iter().any(|used| used == provider) || disabled.iter().any(|id| id == provider) {
            continue;
        }
        disabled.push(provider.to_owned());
        changed = true;
    }
    if changed {
        let _ = db.set_setting("runtimes.disabled", &json!(disabled));
    }
    db.set_setting(DEFAULTS_MARKER, &json!(true))?;
    Ok(())
}

/// The providers an employee or a conversation already runs on. A bot names a
/// runtime; the runtime row names its provider. A conversation records its
/// provider itself.
fn used_providers(db: &Storage) -> Vec<String> {
    let mut used = Vec::new();
    let runtimes = db.runtimes().unwrap_or_default();
    for agent in db.agent_list(true, None).unwrap_or_default() {
        let Some(runtime_id) = agent["runtimeId"].as_str() else { continue };
        let Some(provider) = runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(runtime_id)).and_then(|runtime| runtime["provider"].as_str()) else { continue };
        used.push(provider.to_owned());
    }
    for session in db.session_list(true).unwrap_or_default() {
        if let Some(provider) = session["provider"].as_str().filter(|provider| !provider.is_empty()) {
            used.push(provider.to_owned());
        }
    }
    used
}

/// The refusal when a runtime that is switched off is chosen for a new bot, a
/// hire or a conversation. The sentence names the page the switch lives on.
pub(crate) async fn refuse_off_runtime(st: &AppState, runtime_id: &str) -> Result<(), DispatchError> {
    let disabled = disabled_ids(&st.db);
    let runtimes = st.runtimes.read().await;
    let Some(runtime) = runtimes.iter().find(|runtime| runtime["id"].as_str() == Some(runtime_id)) else {
        // A missing runtime is reported by the caller's own check.
        return Ok(());
    };
    if !crate::runtime_is_disabled(runtime, &disabled) {
        return Ok(());
    }
    let provider = runtime["provider"].as_str().unwrap_or("");
    Err(if provider_is_off(&st.db, provider) { crate::provider_off_error() } else { crate::runtime_off_error() })
}

/// `provider.list`: one row per provider Retinue knows, including one that is not
/// installed, so Settings can list every provider with its switch and its state.
pub(crate) async fn list(st: &AppState) -> Value {
    let disabled = disabled_ids(&st.db);
    let runtimes = st.runtimes.read().await.clone();
    let mut rows = Vec::new();
    for provider in known_providers() {
        let own = runtimes.iter().filter(|runtime| runtime["provider"].as_str() == Some(provider)).cloned().collect::<Vec<Value>>();
        let found = !own.is_empty();
        let main = is_main_provider(provider);
        // Off when the provider itself is switched off, or when every installed
        // copy of it was switched off by its runtime id.
        let every_copy_off = found && own.iter().all(|runtime| runtime["disabled"] == true);
        let enabled = !disabled.iter().any(|id| id == provider) && !every_copy_off;
        let row = retinue_runtime::acp_provider(provider);
        rows.push(json!({
            "provider": provider,
            "displayName": retinue_runtime::provider_display_name(provider),
            "main": main,
            "optional": !main,
            "enabled": enabled,
            "found": found,
            "runtimeIds": own.iter().filter_map(|runtime| runtime["id"].as_str()).collect::<Vec<_>>(),
            "version": own.iter().find_map(|runtime| runtime["version"].as_str()),
            "authState": own.iter().find_map(|runtime| runtime["authState"].as_str()),
            "status": own.iter().find_map(|runtime| runtime["status"].as_str()),
            "installHint": row.map(|row| row.install_hint),
            "signInCommand": row.map(|row| row.sign_in_command),
        }));
    }
    json!({ "providers": rows })
}
