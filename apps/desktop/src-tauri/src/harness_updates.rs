use std::{path::PathBuf, process::Stdio, time::Duration};

use serde::Serialize;
use serde_json::Value;
use tauri::{command, State};
use tokio::process::Command;

use crate::{connection, redact_log_line, rpc_call, AppState};

const REGISTRY: &str = "https://registry.npmjs.org";
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);
const UPDATE_TIMEOUT: Duration = Duration::from_secs(300);
/// How much of the CLI's own last line a failure carries. Longer output is
/// dropped: the CLI decides the sentence, Retinue only passes it on.
const FAILURE_LINE_CHARS: usize = 200;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessUpdateResult {
    pub provider: String,
    pub version: String,
}

fn package_and_updater(
    provider: &str,
) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    match provider {
        "claude" => Some(("@anthropic-ai/claude-code", "claude", &["update"])),
        "codex" => Some(("@openai/codex", "codex", &["update"])),
        "opencode" => Some(("opencode-ai", "opencode", &["upgrade"])),
        _ => None,
    }
}

fn parse_version(value: &str) -> Option<semver::Version> {
    let value = value.trim();
    let start = value.find(|ch: char| ch.is_ascii_digit())?;
    let candidate = value[start..]
        .split_whitespace()
        .next()?
        .trim_start_matches('v');
    semver::Version::parse(candidate).ok()
}

/// The CLI's own last line, redacted and capped: never its whole output, never a
/// credential the CLI happened to print.
fn failure_line(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let line = [stderr, stdout]
        .into_iter()
        .find_map(|bytes| {
            String::from_utf8_lossy(bytes)
                .lines()
                .map(str::trim)
                .rev()
                .find(|line| !line.is_empty())
                .map(str::to_owned)
        })?;
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let redacted = redact_log_line(&collapsed);
    let capped: String = redacted.chars().take(FAILURE_LINE_CHARS).collect();
    (!capped.is_empty()).then_some(capped)
}

/// The executable the daemon's discovery runs for this provider, with its launch
/// arguments. Read rather than re-derived: an npm `.cmd` shim arrives here already
/// rewritten to `node.exe <script>`, so the update runs the same binary a bot turn
/// runs, and no shell has to parse the command line.
async fn discovered_cli(state: &State<'_, AppState>, provider: &str) -> Option<(PathBuf, Vec<String>)> {
    let rpc = connection(state).ok()?;
    let listed = rpc_call(&rpc, "runtime.list", serde_json::json!({})).await.ok()?;
    let row = listed["runtimes"].as_array()?.iter().find(|row| {
        row["provider"].as_str() == Some(provider)
            && row["executablePath"].as_str().is_some_and(|path| !path.is_empty())
    })?;
    let executable = PathBuf::from(row["executablePath"].as_str()?);
    if !executable.is_file() {
        return None;
    }
    let launch = row["launchArgs"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    Some((executable, launch))
}

/// One CLI invocation, run directly: same binary, same arguments, piped stdio and
/// no console window. A provider discovery has not found yet falls back to the
/// bare name on `PATH`.
async fn cli(state: &State<'_, AppState>, provider: &str, fallback: &'static str, args: &[&str]) -> Command {
    let (executable, launch) = discovered_cli(state, provider)
        .await
        .unwrap_or_else(|| (PathBuf::from(fallback), Vec::new()));
    let mut command = Command::new(executable);
    retinue_process::hide_window(&mut command);
    command.args(launch).args(args);
    command
}

async fn latest_version(package: &'static str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(LOOKUP_TIMEOUT)
        .user_agent("Retinue")
        .build()
        .map_err(|error| format!("Could not prepare update check: {error}"))?;
    let response: Value = client
        .get(format!("{REGISTRY}/{package}/latest"))
        .send()
        .await
        .map_err(|error| format!("Could not check for harness updates: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not check for harness updates: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Could not read the harness version: {error}"))?;
    response["version"]
        .as_str()
        .filter(|version| parse_version(version).is_some())
        .map(str::to_owned)
        .ok_or_else(|| "The package registry returned an invalid version.".into())
}

#[command]
pub async fn harness_latest_version(provider: String) -> Result<String, String> {
    let (package, _, _) = package_and_updater(&provider)
        .ok_or_else(|| "Updates are not available for this harness.".to_owned())?;
    latest_version(package).await
}

#[command(rename_all = "camelCase")]
pub async fn harness_update(
    provider: String,
    current_version: String,
    state: State<'_, AppState>,
) -> Result<HarnessUpdateResult, String> {
    let (package, executable, args) = package_and_updater(&provider)
        .ok_or_else(|| "Updates are not available for this harness.".to_owned())?;
    let current = parse_version(&current_version)
        .ok_or_else(|| "The installed harness version could not be read.".to_owned())?;
    let latest = latest_version(package).await?;
    let latest_parsed = parse_version(&latest)
        .ok_or_else(|| "The package registry returned an invalid version.".to_owned())?;
    if latest_parsed <= current {
        return Err("This harness is already up to date.".into());
    }

    let mut command = cli(&state, &provider, executable, args).await;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(UPDATE_TIMEOUT, command.output())
        .await
        .map_err(|_| "The harness update timed out. Retry from Settings → Agents.".to_owned())?
        .map_err(|error| format!("Could not start the harness updater: {error}"))?;
    if !output.status.success() {
        let detail = failure_line(&output.stdout, &output.stderr);
        return Err(match detail {
            Some(line) => format!("The harness updater reported an error: {line}"),
            None => "The harness updater reported an error. Retry or update it with its own CLI.".into(),
        });
    }

    let installed = read_installed_version(&state, &provider, executable).await?;
    let installed_parsed = parse_version(&installed).ok_or_else(|| {
        "The updater finished, but the installed version could not be verified.".to_owned()
    })?;
    if installed_parsed < latest_parsed {
        return Err(format!(
            "The updater finished, but the harness still reports version {installed}."
        ));
    }

    Ok(HarnessUpdateResult {
        provider,
        version: installed_parsed.to_string(),
    })
}

async fn read_installed_version(
    state: &State<'_, AppState>,
    provider: &str,
    executable: &'static str,
) -> Result<String, String> {
    let mut command = cli(state, provider, executable, &["--version"]).await;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .map_err(|_| "Timed out while verifying the installed harness version.".to_owned())?
        .map_err(|error| format!("Could not verify the installed harness version: {error}"))?;
    if !output.status.success() {
        let detail = failure_line(&output.stdout, &output.stderr);
        return Err(match detail {
            Some(line) => format!("The updater finished, but the harness version check failed: {line}"),
            None => "The updater finished, but the harness version check failed.".into(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The updater is the CLI's own verb, never a shell: the table is the command.
    #[test]
    fn each_provider_runs_its_own_update_verb_directly() {
        assert_eq!(package_and_updater("claude"), Some(("@anthropic-ai/claude-code", "claude", &["update"][..])));
        assert_eq!(package_and_updater("codex"), Some(("@openai/codex", "codex", &["update"][..])));
        assert_eq!(package_and_updater("opencode"), Some(("opencode-ai", "opencode", &["upgrade"][..])));
        assert_eq!(package_and_updater("cursor"), None);
    }

    /// A failure carries the CLI's last line, collapsed, redacted and bounded.
    #[test]
    fn a_failure_line_is_the_last_one_redacted_and_capped() {
        let stderr = b"starting\nfetching 100%\nerror: report from https://example.test\n";
        assert_eq!(failure_line(b"", stderr).as_deref(), Some("error: report from https://example.test"));
        assert_eq!(failure_line(b"stdout only\n", b"  \n").as_deref(), Some("stdout only"));
        assert_eq!(failure_line(b"", b""), None);
        let secret = failure_line(b"", b"failed with token=supersecretvalue123456\n").unwrap();
        assert!(!secret.contains("supersecretvalue123456"), "{secret}");
        assert!(secret.contains("[redacted]"), "{secret}");
        let long = format!("{}\n", "x".repeat(500));
        assert!(failure_line(b"", long.as_bytes()).unwrap().chars().count() <= FAILURE_LINE_CHARS);
    }
}
