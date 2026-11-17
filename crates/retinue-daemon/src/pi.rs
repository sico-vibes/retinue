//! Section C of `docs/plans/vault-channels-pi.md`: Pi, with the keys held by
//! Retinue.
//!
//! Owner decision of 16 November 2026 (`AGENTS.md`): "This amends 'no copied
//! provider authentication' for this path only: Retinue may store the owner's
//! model-provider API keys (Credential Manager, `Retinue/pi/<provider>`) and give
//! them to a supervised Pi child at spawn, to reach Pi's providers. The keys are
//! never logged, returned, backed up or sent anywhere but that child."
//!
//! What that means in code:
//!
//! - **One credential per provider**, `Retinue/pi/<provider>`, written and read
//!   through `secrets::store` (the same Credential Manager code as bot secrets).
//!   No RPC returns a value: `pi.providers` answers whether a key is stored, never
//!   what it is, and the value is held in a `Zeroizing` string only long enough to
//!   put it in one child's environment.
//! - **The key reaches only a Pi conversation's child.** `add_keys_to_environment`
//!   is called from `exec_options_for_session` for a conversation whose provider
//!   is `pi`, and nowhere else, so a Claude Code or Codex child never inherits
//!   one.
//! - **A test is Pi's own answer.** `pi.key.test` runs Pi's `auth check` with the
//!   key in that child's environment and reads its exit status, so Retinue never
//!   talks to a provider itself.
//! - **No provider output is forwarded.** A refusal is a fixed sentence naming
//!   the provider; Pi's stdout is never parsed for it.

use crate::{derr, AppState, DispatchError};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{Mutex as StdMutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::{process::Command, time::timeout};
use zeroize::Zeroizing;

/// The one place a provider's key variable is decided. A provider is listed here
/// only when its single API key is the whole setup: providers that also need a
/// resource, region or account id (Azure, Bedrock, Vertex, Cloudflare) and the
/// sign-in-only ones stay out, because a key alone would not make them work.
/// Names and variables: `docs/providers.md` in
/// `@earendil-works/pi-coding-agent` 1.5.2; the ids are Pi's own provider ids,
/// read from its provider registry.
#[derive(Debug)]
pub(crate) struct KeyProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub key_env: &'static str,
}

pub(crate) const KEY_PROVIDERS: &[KeyProvider] = &[
    KeyProvider { id: "anthropic", name: "Anthropic", key_env: "ANTHROPIC_API_KEY" },
    KeyProvider { id: "openai", name: "OpenAI", key_env: "OPENAI_API_KEY" },
    KeyProvider { id: "deepseek", name: "DeepSeek", key_env: "DEEPSEEK_API_KEY" },
    KeyProvider { id: "google", name: "Google Gemini", key_env: "GEMINI_API_KEY" },
    KeyProvider { id: "openrouter", name: "OpenRouter", key_env: "OPENROUTER_API_KEY" },
    KeyProvider { id: "groq", name: "Groq", key_env: "GROQ_API_KEY" },
    KeyProvider { id: "cerebras", name: "Cerebras", key_env: "CEREBRAS_API_KEY" },
    KeyProvider { id: "xai", name: "xAI", key_env: "XAI_API_KEY" },
    KeyProvider { id: "mistral", name: "Mistral", key_env: "MISTRAL_API_KEY" },
    KeyProvider { id: "moonshotai", name: "Moonshot AI", key_env: "MOONSHOT_API_KEY" },
    KeyProvider { id: "moonshotai-cn", name: "Moonshot AI China", key_env: "MOONSHOT_API_KEY" },
    KeyProvider { id: "minimax", name: "MiniMax", key_env: "MINIMAX_API_KEY" },
    KeyProvider { id: "minimax-cn", name: "MiniMax China", key_env: "MINIMAX_CN_API_KEY" },
    KeyProvider { id: "nvidia", name: "NVIDIA NIM", key_env: "NVIDIA_API_KEY" },
    KeyProvider { id: "together", name: "Together", key_env: "TOGETHER_API_KEY" },
    KeyProvider { id: "fireworks", name: "Fireworks", key_env: "FIREWORKS_API_KEY" },
    KeyProvider { id: "baseten", name: "Baseten", key_env: "BASETEN_API_KEY" },
    KeyProvider { id: "huggingface", name: "Hugging Face", key_env: "HF_TOKEN" },
    KeyProvider { id: "kimi-coding", name: "Kimi For Coding", key_env: "KIMI_API_KEY" },
    KeyProvider { id: "meta", name: "Meta", key_env: "META_API_KEY" },
    KeyProvider { id: "ant-ling", name: "Ant Ling", key_env: "ANT_LING_API_KEY" },
    KeyProvider { id: "github-copilot", name: "GitHub Copilot", key_env: "COPILOT_GITHUB_TOKEN" },
    KeyProvider { id: "vercel-ai-gateway", name: "Vercel AI Gateway", key_env: "AI_GATEWAY_API_KEY" },
    KeyProvider { id: "zai", name: "Z.AI", key_env: "ZAI_API_KEY" },
    KeyProvider { id: "zai-coding-cn", name: "Z.AI China", key_env: "ZAI_CODING_CN_API_KEY" },
    KeyProvider { id: "opencode", name: "OpenCode Zen", key_env: "OPENCODE_API_KEY" },
    KeyProvider { id: "opencode-go", name: "OpenCode Go", key_env: "OPENCODE_API_KEY" },
    KeyProvider { id: "qwen-token-plan", name: "Qwen Token Plan", key_env: "QWEN_TOKEN_PLAN_API_KEY" },
    KeyProvider { id: "qwen-token-plan-cn", name: "Qwen Token Plan China", key_env: "QWEN_TOKEN_PLAN_CN_API_KEY" },
    KeyProvider { id: "qwen-token-plan-individual", name: "Qwen Token Plan Individual", key_env: "QWEN_TOKEN_PLAN_API_KEY" },
    KeyProvider { id: "qwen-token-plan-individual-cn", name: "Qwen Token Plan Individual China", key_env: "QWEN_TOKEN_PLAN_CN_API_KEY" },
    KeyProvider { id: "xiaomi", name: "Xiaomi MiMo", key_env: "XIAOMI_API_KEY" },
    KeyProvider { id: "xiaomi-token-plan-cn", name: "Xiaomi Token Plan China", key_env: "XIAOMI_TOKEN_PLAN_CN_API_KEY" },
    KeyProvider { id: "xiaomi-token-plan-ams", name: "Xiaomi Token Plan Amsterdam", key_env: "XIAOMI_TOKEN_PLAN_AMS_API_KEY" },
    KeyProvider { id: "xiaomi-token-plan-sgp", name: "Xiaomi Token Plan Singapore", key_env: "XIAOMI_TOKEN_PLAN_SGP_API_KEY" },
];

/// Where Pi documents the providers it reaches. Every row points here: it is
/// Pi's own page, not a guessed key page for each provider.
const PI_MODELS_DOCS: &str = "https://pi.dev/models";
/// The command name Pi installs.
const PI_COMMAND: &str = "pi";
/// How long an installation answer is trusted: `pi --version` starts Node, so a
/// Settings page that reads it on every open would feel slow.
const INSTALLED_TTL: Duration = Duration::from_secs(5 * 60);
/// A key is bounded like every other stored secret.
const KEY_MAX: usize = crate::secrets::VALUE_MAX;

fn invalid(message: &str) -> DispatchError {
    derr("invalid_argument", message, StatusCode::BAD_REQUEST)
}

fn unavailable(message: String) -> DispatchError {
    derr("unavailable", &message, StatusCode::SERVICE_UNAVAILABLE)
}

/// The credential's name inside Credential Manager: `Retinue/` is added by the
/// store, so this value produces the `Retinue/pi/<provider>` target the plan
/// names.
fn credential_name(provider: &str) -> String {
    format!("pi/{provider}")
}

fn provider_row(provider: &str) -> Option<&'static KeyProvider> {
    KEY_PROVIDERS.iter().find(|row| row.id == provider)
}

/// Whether a key is stored for this provider, without keeping it.
fn has_key(provider: &str) -> Result<bool, String> {
    crate::secrets::store::read(&credential_name(provider)).map(|value| value.is_some())
}

/// The key this provider's child reads, if one is stored.
fn key_of(provider: &str) -> Result<Option<Zeroizing<String>>, String> {
    crate::secrets::store::read(&credential_name(provider))
}

/// Another provider whose key variable is the same one. Pi reads one variable per
/// provider family, so two keys cannot both reach the child: the second is
/// refused rather than silently overriding the first.
fn env_conflict(provider: &str, key_env: &str) -> Option<&'static str> {
    KEY_PROVIDERS
        .iter()
        .filter(|row| row.id != provider && row.key_env == key_env)
        .find(|row| has_key(row.id).unwrap_or(false))
        .map(|row| row.id)
}

/// Puts every stored key into a Pi child's environment. Called only for a
/// conversation whose provider is `pi`.
pub(crate) fn add_keys_to_environment(env: &mut BTreeMap<String, String>) -> Result<(), DispatchError> {
    for row in KEY_PROVIDERS {
        match key_of(row.id) {
            Ok(Some(value)) => {
                env.insert(row.key_env.to_owned(), value.as_str().to_owned());
            }
            Ok(None) => {}
            Err(_) => {
                return Err(unavailable(format!("Retinue could not read the Pi key for {} from the credential store.", row.name)));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Is Pi installed?

struct Installed {
    at: Instant,
    found: bool,
    version: Option<String>,
}

fn installed_cache() -> &'static StdMutex<Option<Installed>> {
    static CACHE: OnceLock<StdMutex<Option<Installed>>> = OnceLock::new();
    CACHE.get_or_init(|| StdMutex::new(None))
}

async fn installed() -> (bool, Option<String>) {
    if let Some((found, version, age)) = installed_cache().lock().ok().and_then(|cache| cache.as_ref().map(|cached| (cached.found, cached.version.clone(), cached.at.elapsed()))) {
        if age < INSTALLED_TTL {
            return (found, version);
        }
    }
    let answer = match retinue_runtime::resolve_command(PI_COMMAND) {
        Some(resolved) => {
            let version = timeout(Duration::from_secs(8), retinue_runtime::version_of(&resolved.executable, &resolved.args)).await.ok().flatten();
            (true, version)
        }
        None => (false, None),
    };
    remember(&answer);
    answer
}

fn remember(answer: &(bool, Option<String>)) {
    if let Ok(mut cache) = installed_cache().lock() {
        *cache = Some(Installed { at: Instant::now(), found: answer.0, version: answer.1.clone() });
    }
}

#[cfg(test)]
fn forget_installed() {
    if let Ok(mut cache) = installed_cache().lock() {
        *cache = None;
    }
}

// ---------------------------------------------------------------------------
// RPC

pub async fn rpc(st: &AppState, method: &str, p: &Value) -> Result<Value, DispatchError> {
    match method {
        "pi.providers" => {
            let (found, version) = installed().await;
            let counts = model_counts(st).await;
            let mut providers = Vec::with_capacity(KEY_PROVIDERS.len());
            for row in KEY_PROVIDERS {
                providers.push(json!({
                    "id": row.id,
                    "name": row.name,
                    "keyEnv": row.key_env,
                    "hasKey": has_key(row.id).map_err(unavailable)?,
                    // A count only exists once the model catalogue has been read;
                    // `null` says "not counted", never zero.
                    "models": counts.as_ref().and_then(|counts| counts.get(row.id).copied()),
                    "docsUrl": PI_MODELS_DOCS,
                }));
            }
            Ok(json!({"installed": found, "version": version, "providers": providers}))
        }
        "pi.key.set" => {
            let provider = named_provider(p)?;
            let key = p["key"].as_str().filter(|key| !key.trim().is_empty() && key.len() <= KEY_MAX && !key.chars().any(char::is_control)).ok_or_else(|| invalid("key must be 1 to 2,048 characters of text"))?;
            if let Some(other) = env_conflict(provider.id, provider.key_env) {
                let other = provider_row(other).map(|row| row.name).unwrap_or("another provider");
                return Err(derr(
                    "conflict",
                    &format!("Pi reads {} for both {other} and {}. One key serves both, so remove the other key first.", provider.key_env, provider.name),
                    StatusCode::CONFLICT,
                ));
            }
            crate::secrets::store::write(&credential_name(provider.id), key).map_err(unavailable)?;
            Ok(json!({"ok": true}))
        }
        "pi.key.delete" => {
            let provider = named_provider(p)?;
            crate::secrets::store::delete(&credential_name(provider.id)).map_err(unavailable)?;
            Ok(json!({"ok": true}))
        }
        "pi.key.test" => {
            let provider = named_provider(p)?;
            let Some(key) = key_of(provider.id).map_err(unavailable)? else {
                return Ok(json!({"ok": false, "detail": format!("No key is stored for {}.", provider.name)}));
            };
            Ok(test_key(provider, &key).await)
        }
        _ => Err(derr("not_found", "unknown Pi method", StatusCode::NOT_FOUND)),
    }
}

fn named_provider(p: &Value) -> Result<&'static KeyProvider, DispatchError> {
    p["provider"].as_str().and_then(provider_row).ok_or_else(|| invalid("provider must be one of the providers Pi takes a key for"))
}

/// Model counts per Pi provider, from the catalogue the daemon already holds for
/// the Pi runtime. A cold cache answers nothing rather than starting a probe from
/// a Settings page.
async fn model_counts(st: &AppState) -> Option<BTreeMap<&'static str, u64>> {
    let runtime = st.runtimes.read().await.iter().find(|runtime| runtime["provider"].as_str() == Some("pi")).cloned()?;
    let served = crate::catalog::cached(st, &runtime)?;
    let mut counts = BTreeMap::new();
    for model in served.value["models"].as_array().into_iter().flatten() {
        let Some(provider) = model["providerId"].as_str() else { continue };
        if let Some(row) = provider_row(provider) {
            *counts.entry(row.id).or_insert(0) += 1;
        }
    }
    Some(counts)
}

/// Pi's own answer about one key: `pi auth check --provider <id> --json` exits 0
/// when the credential resolves, 1 when it does not, and 2 when it is invalid.
/// Only the exit status is read; the output is never parsed or forwarded.
async fn test_key(provider: &'static KeyProvider, key: &Zeroizing<String>) -> Value {
    let Some(resolved) = retinue_runtime::resolve_command(PI_COMMAND) else {
        return json!({"ok": false, "detail": "Pi is not installed, so the key could not be tested."});
    };
    let mut command = Command::new(&resolved.executable);
    retinue_process::prepare_command(&mut command);
    command.args(&resolved.args).args(["auth", "check", "--provider", provider.id, "--json"]);
    command.env(provider.key_env, key.as_str());
    command.env("PI_TELEMETRY", "0");
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return json!({"ok": false, "detail": "Pi could not be started to test the key."}),
    };
    let tree = match retinue_process::ProcessTree::attach(&mut child) {
        Ok(tree) => tree,
        Err(_) => {
            let _ = child.start_kill();
            return json!({"ok": false, "detail": "Pi could not be started to test the key."});
        }
    };
    let waited = timeout(Duration::from_secs(20), child.wait()).await;
    let _ = tree.terminate();
    match waited {
        Err(_) => json!({"ok": false, "detail": "Pi did not answer in time, so the key was not tested."}),
        Ok(Err(_)) => json!({"ok": false, "detail": "Pi could not be asked about the key."}),
        Ok(Ok(status)) => match status.code() {
            Some(0) => json!({"ok": true, "detail": format!("Pi reports the key for {} is ready.", provider.name)}),
            Some(1) => json!({"ok": false, "detail": format!("Pi reports no usable key for {}.", provider.name)}),
            Some(2) => json!({"ok": false, "detail": format!("Pi rejected the key for {}.", provider.name)}),
            _ => json!({"ok": false, "detail": "Pi ended without answering, so the key was not tested."}),
        },
    }
}

/// The fixed sentence nothing secret may appear in: a provider's own output is
/// never forwarded, and a key-shaped word is hidden if one ever is.
#[cfg(test)]
fn safe_detail(text: &str) -> String {
    let clipped = text.chars().take(300).collect::<String>();
    if retinue_security::contains_secret_marker(&clipped) { "[hidden]".to_owned() } else { clipped }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_credential_name_is_the_target_the_plan_names() {
        assert_eq!(credential_name("anthropic"), "pi/anthropic");
        // The store adds `Retinue/`, so the stored target is `Retinue/pi/anthropic`.
        assert_eq!(format!("Retinue/{}", credential_name("opencode-go")), "Retinue/pi/opencode-go");
        // A name the secret page accepts is not what is stored: `/` keeps Pi keys
        // out of `secret.list`.
        assert!(!crate::secrets::valid_name("pi/anthropic"));
    }

    #[test]
    fn every_row_has_one_provider_and_a_key_variable_pi_reads() {
        for row in KEY_PROVIDERS {
            assert!(!row.id.is_empty() && !row.name.is_empty(), "a row has an id and a name");
            assert!(row.key_env.ends_with("_API_KEY") || row.key_env == "HF_TOKEN" || row.key_env == "COPILOT_GITHUB_TOKEN", "{}", row.key_env);
            assert_eq!(KEY_PROVIDERS.iter().filter(|other| other.id == row.id).count(), 1, "{} is listed once", row.id);
        }
        // Ids Pi knows, as its `set_model` wants them.
        for id in ["anthropic", "openai", "deepseek", "openrouter", "opencode-go", "google"] {
            assert!(provider_row(id).is_some(), "{id}");
        }
    }

    #[test]
    fn only_providers_with_a_single_key_are_listed() {
        // These need more than a key (a resource, a region, an account id) or are
        // sign-in only, so a key alone would not make them work.
        for id in ["azure", "amazon-bedrock", "google-vertex", "cloudflare-workers-ai", "openai-codex", "radius"] {
            assert!(provider_row(id).is_none(), "{id} needs more than a key");
        }
    }

    #[test]
    fn a_missing_or_unknown_provider_is_a_plain_refusal() {
        let error = named_provider(&json!({"provider": "nope"})).expect_err("an unknown provider is refused");
        assert!(error.1.message.contains("Pi takes a key for"), "{}", error.1.message);
        assert!(named_provider(&json!({})).is_err());
    }

    #[test]
    fn a_key_shaped_word_never_leaves_a_detail_sentence() {
        let text = format!("provider said {}", "sk-".to_owned() + &"a1".repeat(30));
        assert_eq!(safe_detail(&text), "[hidden]");
        assert_eq!(safe_detail("Model not found"), "Model not found");
    }

    #[test]
    fn keys_are_stored_under_their_own_names_and_the_family_conflict_is_found() {
        // The store is in memory under `cfg(test)`, so this never touches the
        // owner's real credentials.
        let first = credential_name("opencode");
        let second = credential_name("opencode-go");
        assert_ne!(first, second, "each provider keeps its own entry");
        // A conflict is only reported when the other provider really has a key.
        let shared = KEY_PROVIDERS.iter().find(|row| row.id == "opencode-go").unwrap();
        assert_eq!(shared.key_env, "OPENCODE_API_KEY");
        let family: Vec<&str> = KEY_PROVIDERS.iter().filter(|row| row.key_env == "OPENCODE_API_KEY").map(|row| row.id).collect();
        assert_eq!(family, ["opencode", "opencode-go"]);
        assert!(env_conflict("opencode-go", "OPENCODE_API_KEY").is_none(), "nothing stored yet");
        crate::secrets::store::write(&first, "sk-fixture-shared").unwrap();
        assert_eq!(env_conflict("opencode-go", "OPENCODE_API_KEY"), Some("opencode"));
        assert_eq!(env_conflict("opencode", "OPENCODE_API_KEY"), None, "the provider itself is not a conflict");
        crate::secrets::store::delete(&first).unwrap();
        assert!(env_conflict("opencode-go", "OPENCODE_API_KEY").is_none());
    }

    #[test]
    fn a_pi_environment_carries_only_the_keys_that_are_stored() {
        crate::secrets::store::write(&credential_name("deepseek"), "sk-fixture-deepseek").unwrap();
        let mut env = BTreeMap::new();
        add_keys_to_environment(&mut env).unwrap();
        assert_eq!(env.get("DEEPSEEK_API_KEY").map(String::as_str), Some("sk-fixture-deepseek"));
        assert!(!env.contains_key("OPENAI_API_KEY"), "an absent key is absent, not empty");
        let dumped = format!("{env:?}");
        assert!(dumped.contains("sk-fixture-deepseek"), "the child environment is the one place a key appears");
        crate::secrets::store::delete(&credential_name("deepseek")).unwrap();
        let mut empty = BTreeMap::new();
        add_keys_to_environment(&mut empty).unwrap();
        assert!(empty.is_empty());
        forget_installed();
    }
}
