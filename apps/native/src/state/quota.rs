//! Plan limits, ported from `apps/desktop/src/ui/quotaModel.ts`.
//!
//! Values come only from the daemon's quota snapshots. A percentage or a reset
//! time the provider did not report stays absent; nothing is inferred, so the
//! usage bar never shows a number Bitbots made up.

use serde_json::Value;

/// One limit window: "Session", "Weekly", "Monthly" or a plain length.
#[derive(Clone, Debug, PartialEq)]
pub struct QuotaWindow {
    pub key: String,
    pub label: String,
    pub short: String,
    pub used_percent: f64,
    pub resets_at: Option<String>,
    pub duration_mins: Option<f64>,
}

/// One provider's row: its windows, or the reason there are none.
#[derive(Clone, Debug, PartialEq)]
pub struct QuotaProvider {
    pub provider: String,
    pub name: String,
    pub windows: Vec<QuotaWindow>,
    pub status: Option<String>,
    pub stale: bool,
    pub read_off: bool,
    pub fetched_at: Option<String>,
}

/// The order the app lists providers in.
pub const PROVIDER_ORDER: [&str; 3] = ["claude", "codex", "opencode"];

pub fn provider_name(provider: &str) -> String {
    match provider {
        "opencode" => "OpenCode Go".to_owned(),
        "claude" => "Claude".to_owned(),
        "codex" => "Codex".to_owned(),
        other => other.to_owned(),
    }
}

fn record(value: Option<&Value>) -> &Value {
    static EMPTY: Value = Value::Null;
    value.unwrap_or(&EMPTY)
}

fn error_status(code: &str) -> Option<&'static str> {
    match code {
        "no_credentials" | "unauthorized" => Some("Sign-in needed"),
        "read_off" => Some("Usage reading is off"),
        "no_subscription" => Some("No subscription"),
        "protocol" => Some("Unreadable"),
        "timeout" => Some("Timed out"),
        "unavailable" => Some("Unavailable"),
        _ => None,
    }
}

/// The names one window is shown under, from its length and the provider's
/// label (`windowNames` in `quotaModel.ts`).
fn window_names(duration: Option<f64>, limit_label: &str, provider: &str, slot: &str) -> (String, String) {
    let family = ["opus", "sonnet", "fable", "haiku"]
        .into_iter()
        .find(|family| limit_label.to_lowercase().contains(family));
    if let Some(family) = family {
        let mut chars = family.chars();
        let first = chars.next().unwrap_or('x').to_uppercase().collect::<String>();
        let name = format!("{first}{}", chars.as_str());
        let short = family.chars().take(1).collect::<String>().to_uppercase() + &family.chars().skip(1).take(3).collect::<String>();
        return (format!("Weekly · {name}"), short);
    }
    if duration == Some(300.0) || limit_label.to_lowercase().contains("5-hour") {
        return ("Session".to_owned(), "5h".to_owned());
    }
    if duration == Some(10_080.0) || limit_label.to_lowercase().contains("weekly") {
        return ("Weekly".to_owned(), "wk".to_owned());
    }
    if limit_label.to_lowercase().contains("monthly") {
        return ("Monthly".to_owned(), "30d".to_owned());
    }
    if let Some(duration) = duration {
        let short = if duration % 1440.0 == 0.0 {
            format!("{}d", duration / 1440.0)
        } else if duration % 60.0 == 0.0 {
            format!("{}h", duration / 60.0)
        } else {
            format!("{duration}m")
        };
        return (short.clone(), short);
    }
    if provider == "codex" && limit_label != "Codex" {
        return (limit_label.to_owned(), limit_label.to_owned());
    }
    if slot == "primary" {
        ("Primary".to_owned(), "pri".to_owned())
    } else {
        ("Secondary".to_owned(), "sec".to_owned())
    }
}

/// The display rows for the daemon's quota list, in the app's provider order.
/// A provider outside the list is left out, exactly as the React table leaves
/// it out.
pub fn rows(quotas: &[Value]) -> Vec<QuotaProvider> {
    let mut list: Vec<QuotaProvider> = quotas
        .iter()
        .filter(|item| item.get("provider").and_then(Value::as_str).is_some_and(|provider| PROVIDER_ORDER.contains(&provider)))
        .map(|item| {
            let provider = item.get("provider").and_then(Value::as_str).unwrap_or("").to_owned();
            let snapshot = record(item.get("snapshot"));
            let mut windows = Vec::new();
            if let Some(limits) = snapshot.get("limits").and_then(Value::as_array) {
                for limit in limits {
                    let limit_label = limit.get("label").and_then(Value::as_str).unwrap_or("");
                    for slot in ["primary", "secondary"] {
                        let value = &limit[slot];
                        let Some(used) = value.get("usedPercent").and_then(Value::as_f64) else { continue };
                        if !used.is_finite() {
                            continue;
                        }
                        let duration = value.get("windowDurationMins").and_then(Value::as_f64);
                        let (label, short) = window_names(duration, limit_label, &provider, slot);
                        windows.push(QuotaWindow {
                            key: format!("{limit_label}-{slot}"),
                            label,
                            short,
                            used_percent: used.clamp(0.0, 100.0).round(),
                            resets_at: value.get("resetsAt").and_then(Value::as_str).map(str::to_owned),
                            duration_mins: duration,
                        });
                    }
                }
            }
            let last_error = item.get("lastError").and_then(Value::as_str);
            let failed = match last_error {
                Some(code) => Some(error_status(code).unwrap_or("Unavailable").to_owned()),
                None if windows.is_empty() => Some("Unavailable".to_owned()),
                None => None,
            };
            QuotaProvider {
                name: provider_name(&provider),
                read_off: last_error == Some("read_off"),
                stale: last_error.is_some() && !windows.is_empty(),
                status: failed,
                provider,
                windows,
                fetched_at: item.get("fetchedAt").and_then(Value::as_str).map(str::to_owned),
            }
        })
        .collect();
    list.sort_by_key(|row| PROVIDER_ORDER.iter().position(|provider| *provider == row.provider).unwrap_or(usize::MAX));
    list
}

/// The reading closest to its limit: the one worth a line in the sidebar.
pub fn most_pressing(rows: &[QuotaProvider]) -> Option<(&QuotaProvider, &QuotaWindow)> {
    let mut best: Option<(&QuotaProvider, &QuotaWindow)> = None;
    for row in rows {
        for window in &row.windows {
            if best.map(|(_, current)| window.used_percent > current.used_percent).unwrap_or(true) {
                best = Some((row, window));
            }
        }
    }
    best
}

/// How much of a window's time has passed, when both its length and reset are
/// known (`windowElapsedPercent`).
pub fn elapsed_percent(window: &QuotaWindow, now_ms: i64) -> Option<f64> {
    let resets = parse_ms(window.resets_at.as_deref()?)?;
    let duration = window.duration_mins?;
    let left = (resets - now_ms) as f64;
    if left < 0.0 {
        return None;
    }
    Some((100.0 - left / (duration * 600.0)).clamp(0.0, 100.0).round())
}

/// "4h 59m", "2d 3h", "12m", "now": how long until a window resets.
pub fn reset_in(resets_at: Option<&str>, now_ms: i64) -> Option<String> {
    let time = parse_ms(resets_at?)?;
    let seconds = ((time - now_ms) / 1000).max(0);
    if seconds == 0 {
        return Some("now".to_owned());
    }
    if seconds >= 86_400 {
        return Some(format!("{}d {}h", seconds / 86_400, (seconds % 86_400) / 3600));
    }
    if seconds >= 3600 {
        return Some(format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60));
    }
    Some(format!("{}m", (((seconds + 59) / 60).max(1))))
}

/// "Updated just now", "Updated 12m ago".
pub fn updated_ago(fetched_at: Option<&str>, now_ms: i64) -> Option<String> {
    let time = parse_ms(fetched_at?)?;
    let minutes = (now_ms - time).max(0) / 60_000;
    if minutes < 2 {
        return Some("Updated just now".to_owned());
    }
    if minutes < 60 {
        return Some(format!("Updated {minutes}m ago"));
    }
    Some(format!("Updated {}h ago", minutes / 60))
}

/// The bar's colour: calm until 70%, warning to 90%, then critical
/// (`quotaTone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuotaTone {
    Normal,
    Warning,
    Critical,
}

pub fn quota_tone(percent: f64) -> QuotaTone {
    if percent >= 90.0 {
        QuotaTone::Critical
    } else if percent >= 70.0 {
        QuotaTone::Warning
    } else {
        QuotaTone::Normal
    }
}

fn parse_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value).ok().map(|time| time.timestamp_millis())
}

/// The current time, in the milliseconds every function here measures from.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn windows_of(used: f64) -> Vec<Value> {
        vec![json!({
            "provider": "claude",
            "fetchedAt": "2026-11-08T10:00:00Z",
            "snapshot": {"limits": [{"label": "Claude", "primary": {"usedPercent": used, "windowDurationMins": 300, "resetsAt": "2026-11-08T13:00:00Z"}}]}
        })]
    }

    #[test]
    fn a_session_window_takes_its_name_from_its_length() {
        let rows = rows(&windows_of(12.0));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Claude");
        assert_eq!(rows[0].windows[0].label, "Session");
        assert_eq!(rows[0].windows[0].short, "5h");
        assert_eq!(rows[0].windows[0].used_percent, 12.0);
        assert!(rows[0].status.is_none());
        assert!(!rows[0].stale && !rows[0].read_off);
    }

    #[test]
    fn a_percentage_the_provider_did_not_report_is_not_shown() {
        let rows = rows(&[json!({"provider": "codex", "snapshot": {"limits": [{"label": "Codex", "primary": {}}]}})]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].windows.is_empty(), "no number is invented");
        assert_eq!(rows[0].status.as_deref(), Some("Unavailable"));
    }

    #[test]
    fn a_read_off_row_offers_the_switch_and_a_failed_one_keeps_its_last_reading() {
        let read_off = rows(&[json!({"provider": "opencode", "lastError": "read_off"})]);
        assert!(read_off[0].read_off);
        assert_eq!(read_off[0].status.as_deref(), Some("Usage reading is off"));
        let mut stale = windows_of(55.0);
        stale[0]["lastError"] = json!("timeout");
        let stale_rows = rows(&stale);
        assert!(stale_rows[0].stale, "a stale row still shows its last reading");
        assert_eq!(stale_rows[0].windows.len(), 1);
        let signed_out = rows(&[json!({"provider": "claude", "lastError": "no_credentials"})]);
        assert_eq!(signed_out[0].status.as_deref(), Some("Sign-in needed"));
    }

    #[test]
    fn the_pressing_window_is_the_one_closest_to_its_limit() {
        let rows = rows(&windows_of(41.0));
        assert_eq!(most_pressing(&rows).map(|(_, window)| window.used_percent), Some(41.0));
        assert!(most_pressing(&[]).is_none());
    }

    #[test]
    fn a_reset_time_reads_as_the_time_left() {
        let now = parse_ms("2026-11-08T10:00:00Z").unwrap();
        assert_eq!(reset_in(Some("2026-11-08T14:59:00Z"), now).as_deref(), Some("4h 59m"));
        assert_eq!(reset_in(Some("2026-11-10T13:00:00Z"), now).as_deref(), Some("2d 3h"));
        assert_eq!(reset_in(Some("2026-11-08T10:12:00Z"), now).as_deref(), Some("12m"));
        assert_eq!(reset_in(Some("2026-11-08T10:00:00Z"), now).as_deref(), Some("now"));
        assert_eq!(reset_in(None, now), None);
        assert_eq!(reset_in(Some("not a date"), now), None);
    }

    #[test]
    fn the_rate_of_the_window_comes_from_its_own_length() {
        let window = QuotaWindow {
            key: "k".into(),
            label: "Session".into(),
            short: "5h".into(),
            used_percent: 10.0,
            resets_at: Some("2026-11-08T12:30:00Z".into()),
            duration_mins: Some(300.0),
        };
        let now = parse_ms("2026-11-08T10:00:00Z").unwrap();
        // 150 of 300 minutes have passed.
        assert_eq!(elapsed_percent(&window, now), Some(50.0));
        assert_eq!(elapsed_percent(&QuotaWindow { duration_mins: None, ..window.clone() }, now), None);
    }

    #[test]
    fn the_tone_turns_at_seventy_and_ninety() {
        assert_eq!(quota_tone(69.9), QuotaTone::Normal);
        assert_eq!(quota_tone(70.0), QuotaTone::Warning);
        assert_eq!(quota_tone(90.0), QuotaTone::Critical);
    }

    #[test]
    fn an_update_line_says_how_long_ago() {
        let now = parse_ms("2026-11-08T10:00:00Z").unwrap();
        assert_eq!(updated_ago(Some("2026-11-08T09:59:30Z"), now).as_deref(), Some("Updated just now"));
        assert_eq!(updated_ago(Some("2026-11-08T09:48:00Z"), now).as_deref(), Some("Updated 12m ago"));
        assert_eq!(updated_ago(None, now), None);
    }
}
