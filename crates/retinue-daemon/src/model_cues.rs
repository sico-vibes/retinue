//! Wave 2 (nod adoption, 17 November 2026): price and health cues on the model
//! catalogue.
//!
//! The owner had to know from memory what a model costs and whether it just
//! failed. nod shows both in its model menu; this puts the same two facts on every
//! row `runtime.models` already returns, so the window needs no second read and no
//! second vocabulary:
//!
//! * `price` comes from `retinue-usage`'s shipped official price file, with the
//!   source and the date it was checked. A model with no rule gets `null`, and the
//!   window says **price unknown**: an unknown price is never a zero
//!   (`AGENTS.md`, "never present unknown prices as zero").
//! * `health` comes from the daemon's own failure ledger ([`crate::model_health`])
//!   and is `null` when nothing has failed.
//!
//! Capability cues (effort levels, service tiers, context window, release date) are
//! already on the rows the adapters send, so this adds nothing that repeats them.

use crate::AppState;
use serde_json::{json, Value};

/// Adds `price` and `health` to every row of a served catalogue. Rows are left
/// otherwise untouched, and a provider with no price file keeps every price
/// unknown rather than inventing one.
pub(crate) fn apply(st: &AppState, provider: &str, catalog: &mut Value) {
    let rules = retinue_usage::shipped_official_price_rules().unwrap_or_default();
    let health = crate::model_health::rows(st);
    apply_rows(provider, catalog, &rules, &health);
}

/// The overlay itself, with its two inputs passed in, so a test can name them.
fn apply_rows(provider: &str, catalog: &mut Value, rules: &[retinue_usage::PriceRule], health: &[Value]) {
    // `get_mut`, not `catalog["models"]`: indexing a `Value` mutably *inserts* a
    // null key when it is missing, which would edit a catalogue that has no rows.
    let Some(models) = catalog.get_mut("models").and_then(Value::as_array_mut) else { return };
    for model in models.iter_mut() {
        let id = model["id"].as_str().unwrap_or("").to_owned();
        if id.is_empty() { continue; }
        // An OpenCode row names its own gateway (`opencode` or `opencode-go`),
        // which is the route the price file keys on.
        let route = model["providerId"].as_str().filter(|route| !route.is_empty()).unwrap_or(provider).to_owned();
        model["price"] = match retinue_usage::resolve_rule(&route, Some(&id), rules) {
            Some(rule) => json!({
                "inputPerMTok": rule.input_per_million,
                "outputPerMTok": rule.output_per_million,
                "currency": rule.currency,
                "source": rule.source.clone().or_else(|| rule.source_url.clone()),
                "checkedAt": rule.checked_at,
            }),
            None => Value::Null,
        };
        model["health"] = crate::model_health::health_for(health, provider, &id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Vec<retinue_usage::PriceRule> {
        retinue_usage::shipped_official_price_rules().unwrap_or_default()
    }

    #[test]
    fn a_priced_model_carries_its_rates_and_a_checked_date() {
        let mut catalog = json!({"models":[{"id":"claude-fable-5"}]});
        apply_rows("claude", &mut catalog, &rules(), &[]);
        let price = &catalog["models"][0]["price"];
        assert_eq!(price["inputPerMTok"], "10");
        assert_eq!(price["outputPerMTok"], "50");
        assert_eq!(price["currency"], "USD");
        assert!(price["checkedAt"].as_str().is_some());
        assert!(catalog["models"][0]["health"].is_null(), "no failure recorded, no health row");
    }

    #[test]
    fn an_unknown_model_has_no_price_rather_than_a_zero() {
        let mut catalog = json!({"models":[{"id":"no-such-model-anywhere"}]});
        apply_rows("claude", &mut catalog, &rules(), &[]);
        assert!(catalog["models"][0]["price"].is_null());
    }

    #[test]
    fn the_ledger_row_lands_on_the_model_it_belongs_to() {
        let health = vec![json!({"provider":"claude","model":"claude-fable-5","failures":2,"pausedUntil":"2026-11-17T12:30:00.000Z","reason":"The provider reported a turn error.","paused":true})];
        let mut catalog = json!({"models":[{"id":"claude-fable-5"},{"id":"claude-opus-5"}]});
        apply_rows("claude", &mut catalog, &rules(), &health);
        assert_eq!(catalog["models"][0]["health"]["failures"], 2);
        assert!(catalog["models"][1]["health"].is_null());
    }

    #[test]
    fn a_catalogue_with_no_models_array_is_left_alone() {
        let mut catalog = json!({"provider":"claude"});
        apply_rows("claude", &mut catalog, &rules(), &[]);
        assert_eq!(catalog, json!({"provider":"claude"}));
    }
}
