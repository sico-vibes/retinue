//! The vault (16 November 2026, owner decision): logins, cards, authenticator
//! keys, API keys and notes.
//!
//! **The model never holds a value.** A value lives in Windows Credential
//! Manager, one credential per field (`Retinue/vault/<itemId>/<field>`), written
//! and read only here. SQLite holds the metadata a list needs (title, kind, a
//! login's origin, a card's brand and last four); see
//! `crates/retinue-storage/src/vault.rs`. No value reaches the database, an
//! event, a log, a backup, a diagnostics bundle or a push payload.
//!
//! A bot sees `vault_list` (ids, titles, kinds, origins) and asks with
//! `vault_fill`. The owner approves one use on a card; the daemon then types the
//! value into the browser on the linked computer, at the focused field of that
//! computer, or into the owner's own computer inside a live grant. The tool
//! result is `filled`, `denied`, `expired` or `uncertain`, never the value.
//!
//! Every use, reveal, refusal and change writes a content-free row to
//! `vault_activity`. It is never in a backup, and every method is `never` in
//! `tiers.json`, so nothing here is reachable from a paired device.
//!
//! A browser fill is the one place an origin is checked: a login's `origin` is
//! an `https://host` origin (http only on loopback) and the page must report
//! exactly that origin before the value is sent. A mismatch is refused and
//! recorded as `denied`. Filling on the computer and on the owner's computer
//! goes through `computer.rs` and `owner_computer.rs` and their own gates.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha1::Sha1;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroizing;

/// A field value's cap: the credential store's limit for one generic credential.
const VALUE_MAX: usize = crate::secrets::VALUE_MAX;
const TITLE_MAX: usize = 120;
const ORIGIN_MAX: usize = 300;
const USERNAME_MAX: usize = 200;
const TEXT_FIELD_MAX: usize = 200;
/// The tool call waits this long for the owner, under the bridge's 60 seconds
/// (the same margin `request_secret` keeps).
const WAIT_SECS: u64 = 50;
const ACTIVITY_DEFAULT: u32 = 100;
const ACTIVITY_MAX: u32 = 500;
const ID_MAX: usize = 64;
const FIELD_MAX: usize = 16;
const APP_MAX: usize = 200;
const TOTP_PERIOD: i64 = 30;

const CREDENTIAL_PREFIX: &str = "vault";

const KINDS: [&str; 4] = ["login", "card", "apiKey", "note"];
const LOGIN_SECRETS: [&str; 2] = ["password", "totp"];
const CARD_SECRETS: [&str; 2] = ["number", "cvc"];
const API_KEY_SECRETS: [&str; 1] = ["value"];
const NOTE_SECRETS: [&str; 1] = ["text"];

const SENTENCE_BYPASS: &str = "This conversation has Full access, and the vault keeps asking even here. Ask in a conversation set to Ask or Auto, where the owner can answer a card.";

/// Why a fill did not happen. The vault needs the three apart: a refusal typed
/// nothing, a wrong origin is a denial, and an uncertain fill may have reached
/// the page.
#[derive(Debug)]
pub(crate) enum FillRefusal {
    /// The page is not the item's own origin.
    WrongOrigin,
    /// Refused before anything was sent.
    Refused(String),
    /// Sent, but the action failed: the value may have arrived.
    Uncertain(String),
}

fn invalid(message: impl Into<String>) -> DispatchError {
    derr("invalid_argument", &message.into(), StatusCode::BAD_REQUEST)
}

fn not_found(message: impl Into<String>) -> DispatchError {
    derr("not_found", &message.into(), StatusCode::NOT_FOUND)
}

fn conflict(message: impl Into<String>) -> DispatchError {
    derr("conflict", &message.into(), StatusCode::CONFLICT)
}

fn unavailable(message: &str) -> DispatchError {
    derr("unavailable", message, StatusCode::SERVICE_UNAVAILABLE)
}

fn internal() -> DispatchError {
    derr("internal", "the vault could not be saved", StatusCode::INTERNAL_SERVER_ERROR)
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn storage_error(_: retinue_storage::StorageError) -> DispatchError {
    derr("internal", "the vault could not be read", StatusCode::INTERNAL_SERVER_ERROR)
}

// ---------------------------------------------------------------------------
// Kinds and fields

pub(crate) fn valid_kind(kind: &str) -> bool {
    KINDS.contains(&kind)
}

fn secret_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "login" => &LOGIN_SECRETS,
        "card" => &CARD_SECRETS,
        "apiKey" => &API_KEY_SECRETS,
        "note" => &NOTE_SECRETS,
        _ => &[],
    }
}

/// The metadata fields `vault.create` and `vault.update` accept for a kind.
fn public_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "login" => &["origin", "username"],
        "card" => &["holder", "expiry"],
        "apiKey" => &["service"],
        _ => &[],
    }
}

fn secret_field(kind: &str, field: &str) -> bool {
    secret_fields(kind).contains(&field)
}

// ---------------------------------------------------------------------------
// Credential Manager

fn credential_name(item_id: &str, field: &str) -> String {
    format!("{CREDENTIAL_PREFIX}/{item_id}/{field}")
}

/// Writes one field's value. The value is checked here, without being copied
/// anywhere else.
fn write_secret(item_id: &str, field: &str, value: &str) -> Result<(), DispatchError> {
    if value.is_empty() || value.chars().count() > VALUE_MAX || value.contains('\0') {
        return Err(invalid(format!("A field value must be 1 to {VALUE_MAX} characters of text.")));
    }
    crate::secrets::store::write(&credential_name(item_id, field), value)
        .map_err(|_| unavailable("The credential store is unavailable, so the vault could not save that value."))
}

fn read_secret(item_id: &str, field: &str) -> Result<Zeroizing<String>, DispatchError> {
    match crate::secrets::store::read(&credential_name(item_id, field)) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(conflict("That value is missing from the credential store. Save it again in the vault.")),
        Err(_) => Err(unavailable("The credential store is unavailable, so that value could not be read.")),
    }
}

fn forget_secret(item_id: &str, field: &str) -> Result<(), DispatchError> {
    crate::secrets::store::delete(&credential_name(item_id, field))
        .map(|_| ())
        .map_err(|_| unavailable("The credential store is unavailable, so that value could not be removed."))
}

// ---------------------------------------------------------------------------
// Origins

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_matches(|ch| ch == '[' || ch == ']');
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.")
}

/// `scheme://host[:port]` of a page address, with the default port left out.
/// The item's stored origin and the live page are compared through this, so a
/// path or a query can never make two different origins look the same.
pub(crate) fn origin_of_url(raw: &str) -> Option<String> {
    let url = reqwest::Url::parse(raw).ok()?;
    let scheme = url.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    Some(match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

/// A login's origin: an `https://host` origin, or `http://` on loopback only,
/// with no path, query, fragment, user or password.
pub(crate) fn validate_origin(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("A login needs the site's origin, like https://example.com.".into());
    }
    if text.chars().count() > ORIGIN_MAX {
        return Err("That origin is too long.".into());
    }
    if text.chars().any(char::is_control) {
        return Err("That origin holds a control character.".into());
    }
    let url = reqwest::Url::parse(text).map_err(|_| "That is not an origin. Give the site's address, like https://example.com.".to_string())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Leave the user name and password out of the origin.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("An origin has no query or fragment: give the site's address only, like https://example.com.".into());
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err("An origin has no path: give the site's address only, like https://example.com.".into());
    }
    let host = url.host_str().ok_or_else(|| "That origin names no site.".to_string())?;
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(host) => {}
        "http" => return Err("An origin must start with https://, except on this computer (127.0.0.1).".into()),
        other => return Err(format!("An origin starts with https://, not {other}.")),
    }
    origin_of_url(url.as_ref()).ok_or_else(|| "That origin could not be read.".to_string())
}

// ---------------------------------------------------------------------------
// Cards

fn digits_of(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|ch| !ch.is_whitespace() && *ch != '-').collect();
    (!digits.is_empty() && digits.len() <= 19 && digits.bytes().all(|byte| byte.is_ascii_digit())).then_some(digits)
}

/// The Luhn check every card number passes before it is stored.
pub(crate) fn luhn_ok(digits: &str) -> bool {
    if !(12..=19).contains(&digits.len()) {
        return false;
    }
    let mut sum = 0u32;
    for (index, byte) in digits.bytes().rev().enumerate() {
        let mut value = u32::from(byte - b'0');
        if index % 2 == 1 {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
    }
    sum.is_multiple_of(10)
}

/// The brand from the number's own prefix. Unknown stays "Card", never a guess.
pub(crate) fn card_brand(digits: &str) -> &'static str {
    let prefix = |n: usize| digits.get(..n).and_then(|text| text.parse::<u32>().ok());
    match (prefix(1), prefix(2), prefix(3), prefix(4)) {
        (Some(4), _, _, _) => "Visa",
        (_, Some(34 | 37), _, _) => "American Express",
        (_, Some(51..=55), _, _) => "Mastercard",
        (_, _, _, Some(2221..=2720)) => "Mastercard",
        (_, Some(36 | 38), _, _) => "Diners Club",
        (_, Some(65), _, _) => "Discover",
        (_, _, Some(644..=649), _) => "Discover",
        (_, _, _, Some(6011)) => "Discover",
        (_, _, Some(3528..=3589), _) => "JCB",
        (_, Some(62), _, _) => "UnionPay",
        _ => "Card",
    }
}

fn last_four(digits: &str) -> Option<String> {
    (digits.len() >= 4).then(|| digits[digits.len() - 4..].to_owned())
}

/// `MM/YY`: a one- or two-digit month, then a two-digit year.
pub(crate) fn valid_expiry(raw: &str) -> bool {
    let text = raw.trim();
    let Some((month, year)) = text.split_once('/') else { return false };
    let month = month.trim();
    let year = year.trim();
    if month.is_empty() || month.len() > 2 || year.len() != 2 {
        return false;
    }
    if !month.bytes().all(|byte| byte.is_ascii_digit()) || !year.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    matches!(month.parse::<u32>(), Ok(1..=12))
}

// ---------------------------------------------------------------------------
// Authenticator keys (TOTP, RFC 6238, 6 digits, 30 seconds)

/// A base32 key, as an authenticator app writes it: letters and 2 to 7, spaces
/// and dashes ignored, at least 16 symbols (80 bits).
pub(crate) fn base32_key(raw: &str) -> Option<Vec<u8>> {
    let mut bits: u32 = 0;
    let mut count: u32 = 0;
    let mut out = Vec::new();
    let mut symbols = 0usize;
    for ch in raw.chars() {
        if ch.is_ascii_whitespace() || ch == '-' {
            continue;
        }
        if ch == '=' {
            continue;
        }
        let value = match ch.to_ascii_uppercase() {
            letter @ 'A'..='Z' => letter as u32 - 'A' as u32,
            digit @ '2'..='7' => digit as u32 - '2' as u32 + 26,
            _ => return None,
        };
        symbols += 1;
        bits = (bits << 5) | value;
        count += 5;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1u32 << count) - 1;
        }
    }
    (symbols >= 16 && !out.is_empty()).then_some(out)
}

/// The code for one moment and how long it stays valid.
pub(crate) fn totp_now(key: &[u8], at: i64) -> Option<(String, i64)> {
    let counter = u64::try_from(at / TOTP_PERIOD).ok()?;
    let mut mac = Hmac::<Sha1>::new_from_slice(key).ok()?;
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let binary = ((u32::from(digest[offset]) & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    let code = binary % 1_000_000;
    Some((format!("{code:06}"), TOTP_PERIOD - at.rem_euclid(TOTP_PERIOD)))
}

// ---------------------------------------------------------------------------
// The ledger

/// One content-free row: the item, the action, where it happened and who asked.
/// Never a value, a typed text, a page's text or a selector.
fn ledger(st: &AppState, item: &Value, action: &str, where_: &str, session: Option<(Option<&str>, &str)>) -> Result<(), DispatchError> {
    let (bot, session_id) = match session {
        Some((bot, session_id)) => (bot.map(str::to_owned), Some(session_id.to_owned())),
        None => (None, None),
    };
    st.db.vault_activity_add(&json!({
        "id": Uuid::new_v4().to_string(),
        "at": now_iso(),
        "itemId": item["id"],
        "itemTitle": item["title"],
        "action": action,
        "where": where_,
        "origin": item["origin"],
        "bot": bot,
        "sessionId": session_id,
    }))
    .map_err(storage_error)
}

/// The bot's name, when the conversation has one. The card says "your bot"
/// otherwise; the ledger records no name rather than that placeholder.
fn bot_of(st: &AppState, session: &Value) -> Option<String> {
    session["agentId"]
        .as_str()
        .and_then(|id| st.db.agent_get(id).ok())
        .and_then(|agent| agent["name"].as_str().map(str::to_owned))
}

fn item_of(st: &AppState, id: &str) -> Result<Value, DispatchError> {
    st.db
        .vault_item(id)
        .map_err(storage_error)?
        .ok_or_else(|| not_found("That vault item no longer exists."))
}

fn id_of(p: &Value) -> Result<&str, DispatchError> {
    p["id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.chars().count() <= ID_MAX)
        .ok_or_else(|| invalid("id is required"))
}

fn title_of(p: &Value) -> Result<String, DispatchError> {
    let title = p["title"].as_str().map(str::trim).unwrap_or("");
    if title.is_empty() || title.chars().count() > TITLE_MAX {
        return Err(invalid(format!("title must be 1 to {TITLE_MAX} characters")));
    }
    if title.chars().any(char::is_control) {
        return Err(invalid("title cannot hold a control character"));
    }
    Ok(title.to_owned())
}

/// A metadata field as the caller sent it: `None` for absent or null, an error
/// for anything that is not text (or is empty, which the caller refuses).
fn public_field(p: &Value, key: &str, max: usize) -> Result<Option<String>, DispatchError> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            let text = text.trim();
            if text.is_empty() {
                return Err(invalid(format!("{key} cannot be empty")));
            }
            if text.chars().count() > max {
                return Err(invalid(format!("{key} must be at most {max} characters")));
            }
            if text.chars().any(char::is_control) {
                return Err(invalid(format!("{key} cannot hold a control character")));
            }
            Ok(Some(text.to_owned()))
        }
        Some(_) => Err(invalid(format!("{key} must be text"))),
    }
}

/// The `secrets` object: one value per field the kind has, each checked by its
/// own field. Unknown keys are refused, so a typo cannot silently drop a value.
fn secret_input(kind: &str, value: &Value) -> Result<BTreeMap<&'static str, Zeroizing<String>>, DispatchError> {
    let mut out = BTreeMap::new();
    let object = match value {
        Value::Null => return Ok(out),
        Value::Object(object) => object,
        _ => return Err(invalid("secrets must be an object of field names and values")),
    };
    for (field, raw) in object {
        let Some(name) = secret_fields(kind).iter().copied().find(|name| *name == field.as_str()) else {
            return Err(invalid(format!("{field} is not a secret field of a {kind} item")));
        };
        let Some(text) = raw.as_str() else { return Err(invalid(format!("{field} must be text"))) };
        if text.is_empty() {
            return Err(invalid(format!("{field} cannot be empty; delete the item instead")));
        }
        if text.chars().count() > VALUE_MAX || text.contains('\0') {
            return Err(invalid(format!("{field} must be 1 to {VALUE_MAX} characters of text")));
        }
        if field == "totp" && base32_key(text).is_none() {
            return Err(invalid("totp must be a base32 authenticator key of at least 16 symbols"));
        }
        if field == "number" {
            let digits = digits_of(text).ok_or_else(|| invalid("number must be 12 to 19 digits"))?;
            if !luhn_ok(&digits) {
                return Err(invalid("That card number did not pass the check every card number passes (Luhn)."));
            }
        }
        if field == "cvc" {
            let count = text.chars().count();
            if !(3..=4).contains(&count) || !text.chars().all(|ch| ch.is_ascii_digit()) {
                return Err(invalid("cvc must be 3 or 4 digits"));
            }
        }
        out.insert(name, Zeroizing::new(text.to_owned()));
    }
    Ok(out)
}

/// The public columns for a new item, after the kind's own checks.
fn public_columns(kind: &str, p: &Value) -> Result<Value, DispatchError> {
    let mut item = json!({"kind": kind});
    for &field in public_fields(kind) {
        let max = match field {
            "origin" => ORIGIN_MAX,
            "username" => USERNAME_MAX,
            _ => TEXT_FIELD_MAX,
        };
        let Some(value) = public_field(p, field, max)? else { continue };
        let value = if field == "origin" {
            validate_origin(&value).map_err(invalid)?
        } else if field == "expiry" {
            if !valid_expiry(&value) {
                return Err(invalid("expiry must be MM/YY, like 08/28"));
            }
            value
        } else {
            value
        };
        item[field] = json!(value);
    }
    if kind == "login" && item["origin"].is_null() {
        return Err(invalid("A login needs its site's origin, like https://example.com"));
    }
    Ok(item)
}

// ---------------------------------------------------------------------------
// RPCs

fn confirmed(p: &Value) -> Result<(), DispatchError> {
    if p["confirm"] == true {
        Ok(())
    } else {
        Err(invalid("confirm: true is required"))
    }
}

fn limit_of(p: &Value) -> u32 {
    p["limit"].as_u64().map(|limit| limit.clamp(1, u64::from(ACTIVITY_MAX)) as u32).unwrap_or(ACTIVITY_DEFAULT)
}

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "vault.list" => Ok(json!({"items": st.db.vault_items().map_err(storage_error)?})),
        "vault.create" => {
            let kind = p["kind"].as_str().unwrap_or("");
            if !valid_kind(kind) {
                return Err(invalid("kind must be login, card, apiKey or note"));
            }
            let title = title_of(p)?;
            let mut item = public_columns(kind, p)?;
            let secrets = secret_input(kind, &p["secrets"])?;
            // The one value each kind cannot exist without.
            let required = match kind {
                "login" => "password",
                "card" => "number",
                "apiKey" => "value",
                _ => "text",
            };
            if !secrets.contains_key(required) {
                return Err(invalid(format!("A {kind} item needs its {required}")));
            }
            if let Some(digits) = secrets.get("number").and_then(|value| digits_of(value.as_str())) {
                item["brand"] = json!(card_brand(&digits));
                item["last4"] = json!(last_four(&digits));
            }
            item["hasTotp"] = json!(secrets.contains_key("totp"));
            let id = Uuid::new_v4().to_string();
            item["id"] = json!(id);
            item["title"] = json!(title);
            // Values first, then the row: a failed row write removes them again,
            // so the store never keeps a value no item names.
            let written = write_all(&id, &secrets)?;
            let created = match st.db.vault_item_insert(&item) {
                Ok(created) => created,
                Err(error) => {
                    for field in written {
                        let _ = forget_secret(&id, field);
                    }
                    return Err(storage_error(error));
                }
            };
            ledger(st, &created, "create", "desktop", None)?;
            Ok(json!({"item": created}))
        }
        "vault.update" => {
            let id = id_of(p)?;
            let current = item_of(st, id)?;
            let kind = current["kind"].as_str().unwrap_or("login").to_owned();
            let mut item = current.clone();
            if p.get("title").is_some() {
                item["title"] = json!(title_of(p)?);
            }
            for &field in public_fields(&kind) {
                let max = match field {
                    "origin" => ORIGIN_MAX,
                    "username" => USERNAME_MAX,
                    _ => TEXT_FIELD_MAX,
                };
                if p.get(field).is_none() {
                    continue;
                }
                match public_field(p, field, max)? {
                    Some(value) if field == "origin" => item["origin"] = json!(validate_origin(&value).map_err(invalid)?),
                    Some(value) if field == "expiry" => {
                        if !valid_expiry(&value) {
                            return Err(invalid("expiry must be MM/YY, like 08/28"));
                        }
                        item["expiry"] = json!(value);
                    }
                    Some(value) => item[field] = json!(value),
                    None if kind == "login" && field == "origin" => return Err(invalid("A login needs its site's origin, like https://example.com")),
                    None => item[field] = Value::Null,
                }
            }
            let secrets = secret_input(&kind, &p["secrets"])?;
            for (field, value) in &secrets {
                write_secret(id, field, value.as_str())?;
            }
            if let Some(digits) = secrets.get("number").and_then(|value| digits_of(value.as_str())) {
                item["brand"] = json!(card_brand(&digits));
                item["last4"] = json!(last_four(&digits));
            }
            if secrets.contains_key("totp") {
                item["hasTotp"] = json!(true);
            }
            if !st.db.vault_item_update(id, &item).map_err(storage_error)? {
                return Err(not_found("That vault item no longer exists."));
            }
            let updated = item_of(st, id)?;
            ledger(st, &updated, "update", "desktop", None)?;
            Ok(json!({"item": updated}))
        }
        "vault.delete" => {
            confirmed(p)?;
            let id = id_of(p)?;
            let item = item_of(st, id)?;
            let kind = item["kind"].as_str().unwrap_or("login");
            // A credential that cannot be removed fails the whole delete, so no
            // value is left behind with nothing naming it.
            for field in secret_fields(kind) {
                forget_secret(id, field)?;
            }
            if !st.db.vault_item_delete(id).map_err(storage_error)? {
                return Err(not_found("That vault item no longer exists."));
            }
            ledger(st, &item, "delete", "desktop", None)?;
            Ok(json!({"deleted": true}))
        }
        "vault.reveal" => {
            let id = id_of(p)?;
            let item = item_of(st, id)?;
            let kind = item["kind"].as_str().unwrap_or("login");
            let field = p["field"].as_str().filter(|field| field.chars().count() <= FIELD_MAX).ok_or_else(|| invalid("field is required"))?;
            if !secret_field(kind, field) {
                return Err(invalid(format!("{field} is not a secret field of this item")));
            }
            let value = read_secret(id, field)?;
            ledger(st, &item, "reveal", "desktop", None)?;
            Ok(json!({"value": value.as_str()}))
        }
        "vault.totp" => {
            let id = id_of(p)?;
            let item = item_of(st, id)?;
            if item["kind"] != "login" || item["hasTotp"] != true {
                return Err(invalid("That item has no authenticator key."));
            }
            let key = read_secret(id, "totp")?;
            let key = base32_key(key.as_str()).ok_or_else(|| conflict("The stored authenticator key cannot be read as a base32 key."))?;
            let (code, expires_in_secs) = totp_now(&key, Utc::now().timestamp()).ok_or_else(internal)?;
            ledger(st, &item, "reveal", "desktop", None)?;
            Ok(json!({"code": code, "expiresInSecs": expires_in_secs}))
        }
        "vault.activity" => {
            let item_id = p["itemId"].as_str().filter(|id| !id.is_empty());
            let rows = st.db.vault_activity(item_id, limit_of(p)).map_err(storage_error)?;
            Ok(json!({"rows": rows}))
        }
        "vault.use.list" => Ok(json!({"requests": pending_rows()})),
        "vault.use.answer" => {
            let request_id = p["requestId"].as_str().filter(|id| !id.is_empty() && id.chars().count() <= ID_MAX).ok_or_else(|| invalid("requestId is required"))?;
            let decision = p["decision"].as_str().unwrap_or("");
            let allow = match decision {
                "allow" => true,
                "deny" => false,
                _ => return Err(invalid("decision must be allow or deny")),
            };
            let offered = {
                let mut map = requests().lock().unwrap();
                let Some(pending) = map.get_mut(request_id) else { return Err(conflict("That request is no longer waiting.")) };
                pending.waiter.take()
            };
            let Some(waiter) = offered else { return Err(conflict("That request is no longer waiting.")) };
            let status = if allow { "allowed" } else { "denied" };
            let _ = waiter.send(allow);
            st.emit("vault.use.updated", json!({"requestId": request_id, "status": status})).await;
            Ok(json!({"answered": true, "status": status}))
        }
        _ => Err(derr("not_found", "unknown vault method", StatusCode::NOT_FOUND)),
    }
}

/// Writes every secret of a new item, and names the ones written so a failed
/// insert can remove them again.
fn write_all(id: &str, secrets: &BTreeMap<&'static str, Zeroizing<String>>) -> Result<Vec<&'static str>, DispatchError> {
    let mut written: Vec<&'static str> = Vec::new();
    for (field, value) in secrets {
        if let Err(error) = write_secret(id, field, value.as_str()) {
            for done in written {
                let _ = forget_secret(id, done);
            }
            return Err(error);
        }
        written.push(field);
    }
    Ok(written)
}

// ---------------------------------------------------------------------------
// One use, approved on a card

struct Pending {
    session_id: String,
    bot: Option<String>,
    item_id: String,
    item_title: String,
    kind: String,
    field: String,
    surface: String,
    origin: Option<String>,
    expires_at: String,
    /// Taken by the answer; the waiting tool call owns the other end.
    waiter: Option<oneshot::Sender<bool>>,
}

/// Pending uses live in memory only, like `request_secret`: a restart ends them,
/// and nothing about a value is kept here.
fn requests() -> &'static StdMutex<HashMap<String, Pending>> {
    static REQUESTS: OnceLock<StdMutex<HashMap<String, Pending>>> = OnceLock::new();
    REQUESTS.get_or_init(Default::default)
}

fn row_of(id: &str, pending: &Pending) -> Value {
    json!({
        "requestId": id,
        "sessionId": pending.session_id,
        "bot": pending.bot.as_deref().unwrap_or("your bot"),
        "itemId": pending.item_id,
        "itemTitle": pending.item_title,
        "kind": pending.kind,
        "field": pending.field,
        "surface": pending.surface,
        "origin": pending.origin,
        "expiresAt": pending.expires_at,
    })
}

fn pending_rows() -> Vec<Value> {
    let mut rows: Vec<(String, Value)> = requests()
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, pending)| pending.waiter.is_some())
        .map(|(id, pending)| (pending.expires_at.clone(), row_of(id, pending)))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, row)| row).collect()
}

// ---------------------------------------------------------------------------
// The tools

pub fn tool_schemas() -> Vec<Value> {
    vec![
        json!({
            "name": "vault_list",
            "description": "List saved logins, cards, authenticator keys, API keys and notes: their ids, titles, kinds and origins only. Use vault_fill to put one value into a page or a computer; you never see the value.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {"type": "string", "enum": ["login", "card", "apiKey", "note"], "description": "Only items of this kind (optional)."},
                    "origin": {"type": "string", "description": "Only logins for this site origin, like https://example.com (optional)."}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "vault_fill",
            "description": "Fill one saved value into a field, after the owner approves this exact use on a card. The value goes from Retinue to the page or the computer; you never receive it. The result is filled, denied, expired or uncertain.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "itemId": {"type": "string", "description": "The item's id, from vault_list."},
                    "field": {"type": "string", "description": "The field to fill: password, totp or username on a login; number or cvc on a card; value on an API key; text on a note."},
                    "target": {
                        "type": "object",
                        "description": "Where the value goes: {surface: 'browser', ref} for an element on the page you are on, {surface: 'cloud'} for the focused field of the bot computer, or {surface: 'owner', app, element} inside a live grant on the owner's computer.",
                        "properties": {
                            "surface": {"type": "string", "enum": ["browser", "cloud", "owner"]},
                            "ref": {"type": "string", "description": "For a browser fill: the element reference from the last snapshot."},
                            "app": {"type": "string", "description": "For an owner's-computer fill: the app, as owner_apps lists it."},
                            "element": {"type": "integer", "description": "For an owner's-computer fill: the element index from the latest read of that app."}
                        },
                        "required": ["surface"],
                        "additionalProperties": false
                    }
                },
                "required": ["itemId", "field", "target"],
                "additionalProperties": false
            }
        }),
    ]
}

/// `vault_list`: titles, kinds, origins and ids only, and never a username.
pub(crate) async fn tool_list(st: &AppState, _session_id: &str, args: &Value) -> Result<String, String> {
    let kind = args["kind"].as_str().map(str::trim).filter(|kind| !kind.is_empty());
    if let Some(kind) = kind {
        if !valid_kind(kind) {
            return Err("kind must be login, card, apiKey or note.".into());
        }
    }
    let origin = args["origin"].as_str().map(str::trim).filter(|origin| !origin.is_empty());
    let origin = match origin {
        Some(origin) => Some(origin_of_url(origin).or_else(|| validate_origin(origin).ok()).ok_or("origin must be a site origin, like https://example.com.")?),
        None => None,
    };
    let items = st.db.vault_items().map_err(|_| "The vault could not be read.".to_owned())?;
    let mut lines = Vec::new();
    for item in items {
        if kind.is_some_and(|kind| item["kind"].as_str() != Some(kind)) {
            continue;
        }
        if origin.as_deref().is_some_and(|origin| item["origin"].as_str() != Some(origin)) {
            continue;
        }
        let kind = item["kind"].as_str().unwrap_or("");
        let origin = item["origin"].as_str().map(|origin| format!(" {origin}")).unwrap_or_default();
        lines.push(format!("{} | {} ({kind}){origin}", item["id"].as_str().unwrap_or(""), item["title"].as_str().unwrap_or("")));
    }
    if lines.is_empty() {
        return Ok("The vault holds no items that match.".into());
    }
    Ok(format!("Vault items (id | title (kind) origin):\n{}", lines.join("\n")))
}

struct Target {
    surface: &'static str,
    reference: Option<String>,
    app: Option<String>,
    element: Option<u64>,
}

fn target_of(value: &Value) -> Result<Target, String> {
    if !value.is_object() {
        return Err("target must name where the value goes: browser, cloud or owner.".into());
    }
    let surface = match value["surface"].as_str().unwrap_or("") {
        "browser" => "browser",
        "cloud" => "cloud",
        "owner" => "owner",
        _ => return Err("target.surface must be browser, cloud or owner.".into()),
    };
    let reference = if surface == "browser" {
        Some(crate::browser::ref_of(value["ref"].as_str().unwrap_or(""))?)
    } else {
        None
    };
    let app = value["app"].as_str().map(str::trim).filter(|app| !app.is_empty()).map(str::to_owned);
    if app.as_ref().is_some_and(|app| app.chars().count() > APP_MAX) {
        return Err(format!("target.app must be at most {APP_MAX} characters."));
    }
    let element = value["element"].as_u64();
    if surface == "owner" && (app.is_none() || element.is_none()) {
        return Err("A fill on the owner's computer names the app and the element from the latest read of it (owner_screen_state), like {surface:'owner', app:'Chrome', element:4}.".into());
    }
    Ok(Target { surface, reference, app, element })
}

/// The waiting, the fill, and the ledger row. `wait` is shortened by the tests.
pub(crate) async fn tool_fill(st: &AppState, session_id: &str, args: &Value) -> Result<String, String> {
    tool_fill_with(st, session_id, args, Duration::from_secs(WAIT_SECS)).await
}

pub(crate) async fn tool_fill_with(st: &AppState, session_id: &str, args: &Value, wait: Duration) -> Result<String, String> {
    let object = args.as_object().ok_or("vault_fill input must be an object.")?;
    if let Some(key) = object.keys().find(|key| !matches!(key.as_str(), "itemId" | "field" | "target")) {
        return Err(format!("vault_fill input has an unsupported field: {key}."));
    }
    let item_id = args["itemId"].as_str().map(str::trim).filter(|id| !id.is_empty() && id.chars().count() <= ID_MAX).ok_or("itemId is required: the id from vault_list.")?;
    let item = st
        .db
        .vault_item(item_id)
        .map_err(|_| "The vault could not be read.".to_owned())?
        .ok_or("That vault item no longer exists. Call vault_list again.")?;
    let kind = item["kind"].as_str().unwrap_or("").to_owned();
    let field = args["field"].as_str().map(str::trim).filter(|field| !field.is_empty() && field.chars().count() <= FIELD_MAX).ok_or("field is required.")?;
    let allowed = secret_field(&kind, field) || (kind == "login" && field == "username");
    if !allowed {
        return Err(format!("{field} is not a field of that item. A login has username, password and totp."));
    }
    let target = target_of(&args["target"])?;
    if target.surface == "browser" && kind != "login" {
        return Err("Only a login can be filled into a web page, because only a login names the site it belongs to.".into());
    }
    let session = st.db.session_detail(session_id).map_err(|_| "This conversation is no longer available.".to_owned())?;
    if crate::approval_mode_for_session(st, &session) == retinue_agent_core::ApprovalMode::Bypass {
        return Err(SENTENCE_BYPASS.into());
    }
    if crate::questions::may_ask(st, &session).is_err() {
        return Err("The vault is not available here: nobody is watching this conversation to approve a use. Ask in a conversation the owner is in.".into());
    }
    // One open use per conversation: a second card would only stack.
    if requests().lock().unwrap().values().any(|pending| pending.session_id == session_id && pending.waiter.is_some()) {
        return Err("You already have a vault use waiting for the owner. Wait for their answer to it.".into());
    }
    let value = if field == "username" {
        Zeroizing::new(item["username"].as_str().filter(|name| !name.is_empty()).ok_or("That login has no user name saved.")?.to_owned())
    } else {
        read_secret(item_id, field).map_err(|error| error.1.message)?
    };
    let bot = bot_of(st, &session);
    let origin = item["origin"].as_str().map(str::to_owned);
    let id = Uuid::new_v4().to_string();
    let (sender, mut receiver) = oneshot::channel();
    let expires_at = (Utc::now() + ChronoDuration::milliseconds(wait.as_millis() as i64)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let announced = {
        let mut map = requests().lock().unwrap();
        let pending = Pending {
            session_id: session_id.to_owned(),
            bot,
            item_id: item_id.to_owned(),
            item_title: item["title"].as_str().unwrap_or("").to_owned(),
            kind: kind.clone(),
            field: field.to_owned(),
            surface: target.surface.to_owned(),
            origin: origin.clone(),
            expires_at,
            waiter: Some(sender),
        };
        let event = row_of(&id, &pending);
        map.insert(id.clone(), pending);
        event
    };
    st.emit("vault.use.requested", announced).await;

    let turn = st.active_turns.lock().await.get(session_id).cloned();
    let deadline = tokio::time::Instant::now() + wait;
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let outcome = loop {
        tokio::select! {
            answer = &mut receiver => break answer.ok(),
            _ = tick.tick() => {
                if let Some(turn) = turn.as_deref() {
                    if st.active_turns.lock().await.get(session_id).map(String::as_str) != Some(turn) { break None; }
                }
                if tokio::time::Instant::now() >= deadline { break None; }
            }
        }
    };
    // Close the request first, so a late answer finds nothing to send to.
    let pending = requests().lock().unwrap().remove(&id);
    let Some(pending) = pending else { return Ok("expired".into()) };
    let outcome = outcome.or_else(|| receiver.try_recv().ok());
    let ended = match turn.as_deref() {
        Some(turn) => st.active_turns.lock().await.get(session_id).map(String::as_str) != Some(turn),
        None => false,
    };
    let (status, result) = match outcome {
        Some(true) => match perform(st, session_id, &item, &value, &target).await {
            Ok(()) => ("fill", "filled"),
            Err(FillRefusal::WrongOrigin) => ("denied", "denied"),
            Err(FillRefusal::Uncertain(text)) => {
                // Fixed text only: the tool's own words could echo the page (logs stay content-free).
                let _ = text;
                tracing::debug!("vault: a fill may have arrived");
                ("uncertain", "uncertain")
            }
            Err(FillRefusal::Refused(reason)) => {
                // Nothing was sent, so the bot is told why and can try the right
                // thing; the sentence is fixed text and never holds the value.
                if ledger(st, &item, "denied", &pending.surface, Some((pending.bot.as_deref(), session_id))).is_err() {
                    tracing::warn!("vault: the ledger row for a refused use could not be written");
                }
                st.emit("vault.use.updated", json!({"requestId": id, "status": "denied"})).await;
                return Err(reason);
            }
        },
        Some(false) => ("denied", "denied"),
        None if ended => ("expired", "expired"),
        None => ("expired", "expired"),
    };
    if ledger(st, &item, status, &pending.surface, Some((pending.bot.as_deref(), session_id))).is_err() {
        // The fill already happened; a failed ledger write is logged content-free
        // and never turns a use into an error the bot would retry.
        tracing::warn!("vault: the ledger row for one use could not be written");
    }
    if status == "fill" {
        let _ = st.db.vault_item_touch(item_id, &now_iso());
    }
    st.emit("vault.use.updated", json!({"requestId": id, "status": status})).await;
    Ok(result.into())
}

/// The fill itself, through the path that owns the surface.
async fn perform(st: &AppState, session_id: &str, item: &Value, value: &Zeroizing<String>, target: &Target) -> Result<(), FillRefusal> {
    let origin = item["origin"].as_str().unwrap_or("");
    match target.surface {
        "browser" => {
            let reference = target.reference.as_deref().unwrap_or("");
            crate::browser::fill_secret(st, session_id, origin, reference, value.as_str()).await
        }
        "cloud" => crate::computer::type_at_focus(st, session_id, value.as_str()).await,
        _ => {
            let app = target.app.as_deref().unwrap_or("");
            let element = target.element.unwrap_or(0);
            let args = json!({"app": app, "element": element, "text": value.as_str()});
            match crate::owner_computer::call_tool(st, session_id, "owner_type", &args).await {
                Ok(_) => Ok(()),
                Err(text) if text.contains(crate::owner_computer::MAY_HAVE_ARRIVED) => Err(FillRefusal::Uncertain(text)),
                Err(text) => Err(FillRefusal::Refused(text)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luhn_and_brand_agree_with_the_known_cards() {
        assert!(luhn_ok("4242424242424242"), "a Visa test number passes");
        assert!(luhn_ok("5555555555554444"));
        assert!(!luhn_ok("4242424242424241"), "one digit off fails");
        assert!(!luhn_ok("1234"), "too short");
        assert_eq!(card_brand("4242424242424242"), "Visa");
        assert_eq!(card_brand("5555555555554444"), "Mastercard");
        assert_eq!(card_brand("2221000000000009"), "Mastercard");
        assert_eq!(card_brand("378282246310005"), "American Express");
        assert_eq!(card_brand("6011111111111117"), "Discover");
        assert_eq!(card_brand("9999999999999999"), "Card");
    }

    #[test]
    fn an_origin_is_a_scheme_and_a_host_and_nothing_else() {
        assert_eq!(validate_origin("https://Example.com/").unwrap(), "https://example.com");
        assert_eq!(validate_origin("https://example.com:8443").unwrap(), "https://example.com:8443");
        assert!(validate_origin("http://example.com").is_err(), "http is not an origin for a site");
        assert_eq!(validate_origin("http://127.0.0.1:3000").unwrap(), "http://127.0.0.1:3000", "loopback may use http");
        assert!(validate_origin("https://example.com/login").is_err(), "no path");
        assert!(validate_origin("https://user:pass@example.com").is_err(), "no credentials");
        assert_eq!(origin_of_url("https://example.com/a/b?c=1#d").unwrap(), "https://example.com");
        assert!(origin_of_url("file:///c:/x").is_none());
    }

    #[test]
    fn a_totp_code_matches_the_rfc_vectors() {
        // RFC 6238's SHA-1 vectors: the key "12345678901234567890" in base32.
        let key = base32_key("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ").unwrap();
        assert_eq!(totp_now(&key, 59).unwrap().0, "287082");
        assert_eq!(totp_now(&key, 1_111_111_109).unwrap().0, "081804");
        assert_eq!(totp_now(&key, 1_111_111_111).unwrap().0, "050471");
        assert_eq!(totp_now(&key, 1_234_567_890).unwrap().0, "005924");
        assert_eq!(totp_now(&key, 2_000_000_000).unwrap().0, "279037");
        assert_eq!(totp_now(&key, 59).unwrap().1, 1, "the code shown at 59 seconds lasts one more");
    }

    #[test]
    fn a_bad_base32_key_is_refused() {
        assert!(base32_key("not a key!").is_none());
        assert!(base32_key("ABCD").is_none(), "too few symbols");
        assert!(base32_key("gezd gnbv gy3t qojq").is_some(), "spaces and lower case are accepted");
    }

    #[test]
    fn an_expiry_is_mm_yy() {
        assert!(valid_expiry("08/28"));
        assert!(valid_expiry("1/29"));
        assert!(!valid_expiry("13/28"));
        assert!(!valid_expiry("08/2028"));
        assert!(!valid_expiry("0828"));
    }
}
