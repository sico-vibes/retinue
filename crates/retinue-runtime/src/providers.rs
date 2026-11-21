//! The ACP providers `retinue-adapter-acp` serves, from one table.
//!
//! OpenCode is not a row here: it has its own discovery path (a stored
//! credential read, a sign-in probe against its background service) and its own
//! launch flags, and its behaviour must not change. A row is a provider that
//! works through the same adapter with nothing provider-specific left to do.
//!
//! Qwen Code takes `--acp`. Its own `config.ts` today still accepts
//! `--experimental-acp`, but prints "deprecated ... use --acp instead", so the
//! table names the flag it tells you to use. Gemini CLI was removed on
//! 3 November 2026 (owner decision); Retinue no longer looks for it.
//!
//! The three main providers (claude, codex, opencode) are always on; every
//! provider in this table is optional and off until the owner turns it on
//! (11 November 2026).

/// One provider the ACP adapter runs.
#[derive(Debug, Clone, Copy)]
pub struct AcpProvider {
    /// The provider id stored on runtimes, bots and sessions.
    pub provider: &'static str,
    pub display_name: &'static str,
    /// The executable name looked up on `PATH`.
    pub command: &'static str,
    /// The arguments that put the CLI into ACP mode, after the adapter's own.
    pub args: &'static [&'static str],
    /// What the owner runs in a terminal to sign in. It holds no secret.
    pub sign_in_command: &'static str,
    pub install_hint: &'static str,
    /// Text that the CLI's `--help` prints when it can run in ACP mode: the proof discovery looks for.
    pub probe_token: &'static str,
}

pub const ACP_PROVIDERS: &[AcpProvider] = &[
    AcpProvider {
        provider: "qwen",
        display_name: "Qwen Code",
        command: "qwen",
        args: &["--acp"],
        sign_in_command: "qwen",
        install_hint: "Install Qwen Code with npm install -g @qwen-code/qwen-code",
        probe_token: "acp",
    },
    // Cursor's CLI is called `agent`; its docs start the ACP server with `agent acp`.
    AcpProvider {
        provider: "cursor",
        display_name: "Cursor CLI",
        command: "agent",
        args: &["acp"],
        sign_in_command: "agent login",
        install_hint: "Install the Cursor CLI from cursor.com/cli",
        probe_token: "acp",
    },
    // xAI's Grok Build CLI runs as an ACP agent with `grok agent stdio`.
    AcpProvider {
        provider: "grok",
        display_name: "Grok CLI",
        command: "grok",
        args: &["agent", "stdio"],
        sign_in_command: "grok login",
        install_hint: "Install the Grok CLI from x.ai/cli",
        probe_token: "agent",
    },
    // Google's Antigravity CLI documents `agy acp` as an Agent Client Protocol
    // server over stdin/stdout (antigravity.google/docs/ide/extensions/acp), and
    // `agy acp --login` as the in-terminal sign-in.
    //
    // VERIFIED ON WINDOWS (21 November 2026, agy 1.5.13): `agy --help` lists the
    // `acp` subcommand, and `agy acp` answers `initialize` with `authMethods`
    // (four Google sign-in methods) whether or not anyone is signed in. The
    // sign-in evidence is `session/new`: it opens a session and reports the
    // account's tier under `_meta.antigravity.tier` when the CLI holds a
    // keyring sign-in, and a server with nothing signed in refuses with `-32000`
    // "Authentication required". `retinue-runtime` asks exactly that, without
    // reading the keyring.
    //
    // Its sign-in is the CLI's own keyring flow: Retinue never reads it, and the
    // provider is single-account (the machine's own sign-in).
    AcpProvider {
        provider: "antigravity",
        display_name: "Antigravity",
        command: "agy",
        args: &["acp"],
        sign_in_command: "agy acp --login",
        install_hint: "Install the Antigravity CLI from antigravity.google/docs/getting-started",
        probe_token: "acp",
    },
];

/// Pi is not an ACP row: it speaks its own JSONL RPC on stdin and stdout
/// (`pi --mode rpc`), so it has its own adapter (`retinue-adapter-pi`). It is
/// still a coding agent Retinue discovers, offers and can switch off like every
/// optional provider, so its label and install hint live here beside the rows.
pub const PI_PROVIDER: &str = "pi";
pub const PI_DISPLAY_NAME: &str = "Pi";
/// The proof discovery looks for in `pi --help` (16 November 2026, package
/// `@earendil-works/pi-coding-agent` 1.5.2, `docs/cli.md`): the CLI selects the
/// long-lived protocol with `--mode rpc`.
pub const PI_PROBE_TOKEN: &str = "--mode";
pub const PI_INSTALL_HINT: &str = "Install Pi with npm install -g @earendil-works/pi-coding-agent (it needs Node 22.19 or newer), or with the installer at pi.dev.";
/// What the owner runs to sign in to Pi's own providers. Retinue's Pi page
/// stores keys instead; this is for the CLIs' other providers.
pub const PI_SIGN_IN_COMMAND: &str = "pi auth login";

/// The providers that are always on (owner decision, 11 November 2026): Claude
/// Code, Codex and OpenCode. Every other provider is optional and off until the
/// owner turns it on; `runtimes.disabled` cannot switch these three off.
pub const MAIN_PROVIDERS: [&str; 3] = ["claude", "codex", "opencode"];

/// Whether a provider is one of the three that are always on.
pub fn is_main_provider(provider: &str) -> bool {
    MAIN_PROVIDERS.contains(&provider)
}

/// Every provider behind the owner's switch: all of them but the three main ones.
pub fn optional_providers() -> Vec<&'static str> {
    discovery_ids().into_iter().filter(|id| !is_main_provider(id)).collect()
}

/// The row for a table provider.
pub fn acp_provider(provider: &str) -> Option<&'static AcpProvider> {
    ACP_PROVIDERS.iter().find(|row| row.provider == provider)
}

/// Whether a provider is served by `retinue-adapter-acp`: OpenCode, or a table
/// row. This is the single answer for "does this provider speak ACP", used by
/// discovery, capabilities, attachments and images.
pub fn is_acp_provider(provider: &str) -> bool {
    provider == "opencode" || acp_provider(provider).is_some()
}

/// The arguments that put a provider's CLI into ACP mode. OpenCode 1.x and 2.x
/// take the `acp` subcommand; a table row names its own flag.
pub fn acp_launch_args(provider: &str) -> &'static [&'static str] {
    match acp_provider(provider) {
        Some(row) => row.args,
        // OpenCode, and the adapter's own default: the `acp` subcommand.
        None => &["acp"],
    }
}

/// Every provider Retinue looks for on `PATH`, in display order.
pub fn discovery_ids() -> [&'static str; 8] {
    ["claude", "codex", "opencode", PI_PROVIDER, ACP_PROVIDERS[0].provider, ACP_PROVIDERS[1].provider, ACP_PROVIDERS[2].provider, ACP_PROVIDERS[3].provider]
}

/// The sentence Settings shows for a provider that is not installed, or that
/// needs a sign-in command of its own. ACP rows carry their own; Pi is not a row.
pub fn install_hint(provider: &str) -> Option<&'static str> {
    match acp_provider(provider) {
        Some(row) => Some(row.install_hint),
        None if provider == PI_PROVIDER => Some(PI_INSTALL_HINT),
        None => None,
    }
}

/// The command the owner runs in a terminal to sign in to the provider's CLI.
pub fn sign_in_command(provider: &str) -> Option<&'static str> {
    match acp_provider(provider) {
        Some(row) => Some(row.sign_in_command),
        None if provider == PI_PROVIDER => Some(PI_SIGN_IN_COMMAND),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_names_the_flags_the_clis_ask_you_to_use() {
        assert_eq!(acp_launch_args("qwen"), &["--acp"]);
        assert_eq!(acp_launch_args("cursor"), &["acp"]);
        assert_eq!(acp_launch_args("grok"), &["agent", "stdio"]);
        // OpenCode keeps the subcommand it has always used.
        assert_eq!(acp_launch_args("opencode"), &["acp"]);
        for row in ACP_PROVIDERS {
            assert!(!row.args.is_empty() && !row.probe_token.is_empty());
            assert!(row.sign_in_command.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-'), "the sign-in command has no shell syntax");
            assert!(!row.display_name.is_empty() && !row.install_hint.is_empty());
        }
    }

    // Antigravity (11 November 2026): the command line is Google's, not a run.
    #[test]
    fn antigravity_is_an_optional_acp_provider_launched_as_its_documentation_says() {
        let row = acp_provider("antigravity").expect("Antigravity is a table row");
        assert_eq!(row.command, "agy");
        assert_eq!(row.args, &["acp"]);
        assert_eq!(acp_launch_args("antigravity"), &["acp"]);
        assert_eq!(row.sign_in_command, "agy acp --login");
        assert!(is_acp_provider("antigravity"));
        assert!(discovery_ids().contains(&"antigravity"));
        assert!(!is_main_provider("antigravity"), "it is behind the owner's switch");
    }

    #[test]
    fn the_first_three_providers_are_the_only_ones_that_are_always_on() {
        assert_eq!(MAIN_PROVIDERS, ["claude", "codex", "opencode"]);
        for main in MAIN_PROVIDERS { assert!(is_main_provider(main), "{main}"); }
        for optional in ["qwen", "cursor", "grok", "antigravity", "gemini", ""] {
            assert!(!is_main_provider(optional), "{optional}");
        }
        assert_eq!(optional_providers(), ["pi", "qwen", "cursor", "grok", "antigravity"]);
    }

    #[test]
    fn only_opencode_and_the_table_rows_speak_acp() {
        assert!(is_acp_provider("opencode"));
        assert!(is_acp_provider("qwen") && is_acp_provider("cursor") && is_acp_provider("grok"));
        for provider in ["claude", "codex", "", "gemini", "gemini-cli", "pi"] {
            assert!(!is_acp_provider(provider), "{provider}");
            assert!(acp_provider(provider).is_none(), "{provider}");
        }
        assert_eq!(discovery_ids(), ["claude", "codex", "opencode", "pi", "qwen", "cursor", "grok", "antigravity"]);
    }

    // Pi (16 November 2026): its own RPC protocol, so not an ACP row, but it is a
    // provider Retinue lists and the owner can switch off.
    #[test]
    fn pi_is_a_provider_without_an_acp_row() {
        assert!(discovery_ids().contains(&PI_PROVIDER));
        assert!(!is_acp_provider(PI_PROVIDER));
        assert!(!is_main_provider(PI_PROVIDER));
        assert!(optional_providers().contains(&PI_PROVIDER));
        assert_eq!(install_hint(PI_PROVIDER), Some(PI_INSTALL_HINT));
        assert_eq!(sign_in_command(PI_PROVIDER), Some(PI_SIGN_IN_COMMAND));
        // The ACP rows keep their own answers, and an unknown provider has none.
        assert_eq!(install_hint("qwen"), Some(ACP_PROVIDERS[0].install_hint));
        assert_eq!(install_hint("gemini"), None);
        assert_eq!(sign_in_command("gemini"), None);
    }
}
