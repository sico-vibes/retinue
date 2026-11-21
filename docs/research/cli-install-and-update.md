# Installing and updating coding-agent CLIs from inside the app

> **What this is:** research from the Director's brief, 21 November 2026 — how other desktop apps install and update coding-agent CLIs without asking the owner to run commands, and the design proposed for Retinue, Windows and the headless Linux host. It changes no product code.
> **Status:** research and a recommendation; nothing here is built.
> **Owner's question, in one line:** today an update started in the app opens a visible console window that looks like CMD; the owner wants no window visible, ever.

Everything below is either **[code]** (a file and line in this repository or in the project named), **[doc]** (a published page) or **[measured]** (a command or request I ran today, with the answer). Anything I could not check is marked **unverified** and listed in section 2.3.

---

## 1. Summary and recommendation

1. **The window has one cause, and it is small.** `harness_update` in `apps/desktop/src-tauri/src/harness_updates.rs` starts `powershell.exe` with no creation flags (`:88`, and again at `:136` for the version read), from a shell process that is built as a GUI application (`apps/desktop/src-tauri/src/main.rs:1`). A console program started by a process with no console gets a console window of its own — the rule this repository already recorded for the daemon in `docs/history/fix-35/c5-windows.md:9`. That is the window the owner sees. **[code]**
2. **The fix is not only a flag.** The update runs `& claude update` (or `codex update`, `opencode upgrade`) through PowerShell, which both needs a console and re-parses the command. Spawning the resolved CLI directly, with piped stdio and `CREATE_NO_WINDOW`, does the same job with no window and honest error text, and it is the pattern discovery already uses (`crates/retinue-runtime/src/lib.rs:183-205`). **[code]**
3. **Installing is a different, bigger question, and the answer exists in two places I could read.** Orca downloads a **digest-checked release** into its own per-user folder, keeps each version in its own folder with a receipt, verifies the binary answers `--version`, and only updates its own copy — never the owner's. T3 Code keeps the owner's own install, works out **which installer owns the CLI** from the executable's real path, and runs that installer. Neither shows a console: Orca's process layer sets `windowsHide: true` and `shell: false` on every spawn with the comment "GUI-subsystem parents otherwise spawn visible consoles that steal foreground input" (`src/packages/process-host/src/spawn-resolution.ts`). **[code]** **[doc]**
4. **Recommendation:** do both, in this order.
   - **Now:** remove the window (direct spawn, `CREATE_NO_WINDOW`), and finish the sweep of every spawn that can show one (section 9).
   - **Then:** a daemon-owned installer with two modes per provider — **adopt** (run the installer that owns an existing CLI) and **manage** (download a pinned, digest-checked build into `<data>/agents/`). Claude Code, Codex, OpenCode and Pi get one-click Install/Update; Cursor, Grok, Antigravity and Qwen get a verified link or a verified download only where the publisher publishes a digest (section 8.2).
5. **New reach, new rules.** This is the first download-and-execute path in the daemon. It needs: an HTTPS-only host allowlist, a digest compared before anything runs, no script from any archive, versioned folders that never overwrite a running binary, a consent card per provider, a content-free activity line, and `never` for every paired device — section 8.9 and the proposed threat-model section 25.

---

## 2. How this was checked, and what is not settled

### 2.1 Method

- Read the repository's own paths end to end: discovery (`crates/retinue-runtime/src`), the daemon's provider table (`crates/retinue-daemon/src/provider_gate.rs`), the shell's update and terminal commands (`apps/desktop/src-tauri/src/harness_updates.rs`, `lib.rs`), the UI (`apps/desktop/src/ui/HarnessUpdatesNotice.tsx`, `SignInHelp.tsx`, `ProvidersPage.tsx`, `PiPage.tsx`, `providerBrand.tsx`), the remote policy (`apps/desktop/src/remote/tiers.json`), the invariants page and the threat model.
- Scanned **every** `Command::new` in `crates/` and `apps/desktop/src-tauri/src/` (tests excluded) and read six lines either side of each to see whether a creation flag is set (section 9).
- Read the source of Orca (`stablyai/orca`) and T3 Code (`pingdotgg/t3code`) from GitHub, and Synara's published documentation.
- Queried the npm registry, the Claude release manifest and three GitHub releases for versions, asset names, sizes and whether a digest is published (Appendix A). Each of these was a request I made today, not a remembered figure.
- Checked Pi's mark and licence by `HEAD` request and by reading `LICENSE`; I did not download the image.

### 2.2 Reproducing the requests

```
https://registry.npmjs.org/<package>/latest            # version, dist.integrity, optionalDependencies
https://downloads.claude.ai/claude-code-releases/stable # a version string, then .../<version>/manifest.json
https://api.github.com/repos/<owner>/<repo>/releases/latest   # asset names and sizes
HEAD https://pi.dev/logo-auto.svg                      # 200, image/svg+xml
```

### 2.3 Not settled here

- **Whether a coding-agent process started from this daemon shows a console today.** Every CLI child is prepared by `retinue_process::prepare_command`, which sets `CREATE_SUSPENDED` (`0x4`) and **not** `CREATE_NO_WINDOW` (`crates/retinue-process/src/lib.rs:21,37`); it pauses the child, attaches the job object, then resumes it (`:206-236`). By the rule the FIX35 note records, that flag set means a console window may be allocated for each CLI process. The owner reports one window from an update, not one from every turn, so something else may be suppressing it (the daemon's own `CREATE_NO_WINDOW` at `apps/desktop/src-tauri/src/lib.rs:519`, or an inherited console). **This needs one five-minute check on the owner's machine before anyone claims it:** start a bot turn on the current build and watch the screen; nothing needs changing first. Either way the design puts `CREATE_NO_WINDOW` on every spawn in one place, which makes the answer moot.
- **Whether `pi-windows-x64.zip` contains a Node runtime.** Range requests to the asset are refused (`501`), so I could not list the archive without downloading 62 MB, and I did not. Pi's npm package requires Node `>=22.19.0` (`engines`, Appendix A), so the npm route needs Node; the zip's contents are unverified.
- **Cursor's Windows installer digest.** `cursor.com/install?win32=true` is a script (`apps/desktop/src/ui/providerGuides.ts:69`); I found no published manifest with a digest for it. Antigravity, by contrast, publishes a sha512 manifest (section 8.2).
- **The exact channel Claude's own updater follows** — the CLI's `autoUpdatesChannel` (Orca reads it, `src/main/claude/managed-runtime/claude-managed-runtime.ts`). Retinue does not read it today.
- **T3 Code's behaviour when its own desktop app starts the server headless on Windows.** Its HTTP/process layer hides windows for probes (`windowsHide: true` in `SpawnWorker.ts` and `packages/shared/src/shell.ts`), and its provider updater inherits the server's console; I could not find the desktop's spawn of the server to say whether that console exists. Treat the "no terminal" claim as "the update runs inside the server process, which has no window of its own when it runs as a background service" rather than a proven flag.

---

## 3. What Retinue does today

### 3.1 Discovery: PATH only, with an honest gate

`crates/retinue-runtime/src/lib.rs:76-98` walks `PATH` and, for a `.cmd`, rewrites the shim into `node.exe <script>` (`:100-151`) so no `cmd.exe` is needed. One provider's probes are: `--version`, a `--help` that must advertise the protocol token, then a sign-in check, then capabilities (`discover_one`, `:269-339`). The runtime id hashes **the executable's path**: `format!("rt_{id}_{}", short_hash(&resolved.executable…))` (`:272`). That detail matters later: a new install path means a new runtime id.

Providers and their install hints are one table: `crates/retinue-runtime/src/providers.rs:34-84` (Qwen, Cursor, Grok, Antigravity as ACP rows) and `:86-99` (Pi's own constants, `PI_INSTALL_HINT` at `:96`). `install_hint` and `sign_in_command` are read by `provider.list` (`crates/retinue-daemon/src/provider_gate.rs:174-175`) and shown as plain sentences.

### 3.2 Install: a visible terminal running the publisher's script

`SignInHelp` (`apps/desktop/src/ui/SignInHelp.tsx:40-64`) calls `runtimeTerminal(provider, 'install')`. The shell builds a **fixed table** of commands (`apps/desktop/src-tauri/src/lib.rs:2140-2153`):

| Provider | What the app runs |
|---|---|
| Claude Code | `irm https://claude.ai/install.ps1 \| iex` |
| Codex | `irm https://chatgpt.com/codex/install.ps1 \| iex` |
| OpenCode | `npm install -g opencode-ai` (refused if `npm.cmd` is not on PATH) |

and opens it in a Windows Terminal tab, or in `powershell.exe -NoExit -Command …` with `CREATE_NEW_CONSOLE` (`:2187-2232`, `:2252-2260`). The window is deliberate here. The security properties are not: the command is downloaded and executed **with no version, no digest and no record**, and the app's only knowledge afterwards is "the owner says they are done" (`SignInHelp.tsx:45-53`). **[code]**

### 3.3 Update: PowerShell with no creation flags — the reported window

`apps/desktop/src-tauri/src/harness_updates.rs` is the update path the UI calls `harnessUpdate` (`apps/desktop/src/ui/HarnessUpdatesNotice.tsx:59`), shown in Settings › Agents (`ui/SettingsSheet.tsx:763`).

- `harness_latest_version` reads `https://registry.npmjs.org/<package>/latest` (`:8`, `:40-61`). The package table is `:19-28`: `@anthropic-ai/claude-code`, `@openai/codex`, `opencode-ai`.
- `harness_update` refuses when the registry is not newer (`:82-84`), then runs, on Windows:

  ```rust
  let mut command = Command::new("powershell.exe");
  command.args(["-NoProfile", "-NonInteractive", "-Command"]);
  command.arg(format!("& {executable} {}", args.join(" ")));   // "& claude update"
  ```

  (`:86-106`). **No `creation_flags`, no `windowsHide`.** The same shape, without the PowerShell wrapper, on other platforms (`:96-100`), and again for the version read (`:133-146`).
- It waits up to 300 s (`:10`), decides success by exit status (`:111-115`), re-reads `--version` and fails if it is still older (`:117-125`).

Three consequences beyond the window:

1. **A console window**, because the shell is `windows`-subsystem (`apps/desktop/src-tauri/src/main.rs:1`) and PowerShell is a console program.
2. **PowerShell as a second parser**, for a command that needs none. If `claude` resolves to an npm `.cmd`, PowerShell runs `cmd.exe` underneath.
3. **A version comparison across two release trains.** Today npm `latest` for `@anthropic-ai/claude-code` is **2.1.330** [measured], while Claude's own native `stable` channel is **2.1.323** [measured]. Whichever the owner installed, one of the two numbers is the wrong yardstick: the notice can offer an update the native updater will not install, or miss one it would. (Orca solves this per provider: npm for npm, GitHub tags for Codex, Claude's own manifest for Claude — `src/main/agent-cli-update/agent-cli-update-agents.ts`.) **[code]** **[measured]**

There is no `runtime.install` or `runtime.update` in the daemon: the only daemon-side mentions of install are `installHint` strings. The Pi page shows `pi-install` as a `<code>` block (`apps/desktop/src/ui/PiPage.tsx:60-62`), not a button.

### 3.4 The Pi mark is a letter

`apps/desktop/src/ui/providerBrand.tsx:7-19` maps a provider to `{name, mark, file}`, and looks for `apps/desktop/src/assets/providers/<file>[-light|-dark].<ext>`. That folder holds `claude`, `codex`, `opencode`, `cursor`, `grok`, `qwen`, `antigravity` (9 files, measured). There is **no `pi` entry and no Pi file**, so `providerBrand('pi')` falls through to `mark: 'P'` (`:23`, `:33`). That is the "P" the owner wants replaced.

### 3.5 What the UI can show today

`ProvidersPage.tsx:36-41,66-70` shows a row per provider with `Installed · <version>` or `Not installed. <installHint>` and the on/off switch — **no Install or Update button**. Install exists only in onboarding/scan recovery (`FirstRunOnboarding.tsx:163`, `ScanRecovery.tsx:81`) and sign-in help, all through the terminal. Update exists only in `HarnessUpdatesNotice`.

---

## 4. Orca (`stablyai/orca`)

Orca is an Electron desktop app for a fleet of CLI agents. Two separate mechanisms, and the distinction is the most useful thing in this whole report.

### 4.1 The owner's own CLI: find it, work out what installed it, run that

`src/main/agent-cli-update/agent-cli-own-install.ts` resolves the command through the same PATH a new chat sees, takes `realpath` of it, and **returns `null` for Orca's own copies** (anything under `<userData>/agent-runtimes`). So the owner's install and Orca's copy never compete.

`src/main/agent-cli-update/agent-cli-update-service.ts` then:

- reads the installed version from `--version`, cached by a stamp of the *file* (so an unchanged file is not re-probed, `installedVersion`);
- looks up the latest release per provider (`agent-cli-latest-release.ts`): Claude's own channel, `registry.npmjs.org/<pkg>/latest`, `api.github.com/.../releases/latest` with a tag prefix, or the ACP registry (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`), each cached for an hour, one request in flight, a failure waits out the hour;
- **runs the update** with a 10-minute limit, 64 KiB of output kept and streamed as a tail, then re-reads the version and fails if it is still behind ("a package manager behind the release (e.g. Homebrew) exits cleanly on the old version");
- one run per provider at a time (concurrent callers join the same promise).

`agent-cli-update-agents.ts` is the per-provider table: `claude update`; `codex update` only from 0.126.0, otherwise the manual command (npm, brew, or the standalone installer) chosen from the real path; `opencode upgrade` (1.x vs 2.x package); `cursor update`; `kimi update --yes`; `goose update`; and honest **manual-only** entries where the self-updater is unsafe — `hermes update` is left to the user because it restarts the gateways serving open chats, `auggie upgrade` because it installs a prerelease.

### 4.2 Orca's own copies: digest-checked downloads into its data folder

`src/main/claude/managed-runtime/claude-managed-runtime.ts` fetches `https://downloads.claude.ai/claude-code-releases/<channel>`, then `<version>/manifest.json`, which carries, per platform, `binary`, **`checksum` (sha256)** and `size`. It builds a pin `{version, url, archiveSha256, archiveBytes, format: 'file', members: [{path, sha256, bytes}]}` and installs into `<dataDirectory>/agent-runtimes/claude/<version>/`. After installing it **runs the binary with `--version` and requires the pinned version back** (`requireRuntimeAnswersVersion`).

`src/main/antigravity/agy-managed-runtime.ts` does the same from Google's manifest, **sha512**, and states why it does not use the vendor installer: "it is unpinned and `agy install` edits shell profiles and PATH", and it sets `AGY_CLI_DISABLE_AUTO_UPDATE=true` because "agy's own updater would replace it unverified". `omp-managed-runtime.ts` uses the GitHub release asset plus `SHA256SUMS.txt`.

`src/main/node-runtime/managed-runtime-release.ts` is the bookkeeping that makes this safe:

- each release lives in `<root>/<version>/` with a `.install-complete.json` receipt holding the **release id (the archive digest)**, version, platform, arch and member sizes;
- a folder without a whole receipt is reported **damaged** and never used; a republished build with the same version and a different digest is not mistaken for the one installed;
- the receipt is written **before** an atomic rename, so a published release cannot carry a partial receipt;
- `prepare`/`validate` hooks run before the copy is published.

`src/shared/agent-launch/installation/managed-runtime-install.ts` is the wire contract the UI uses: phases `downloading` → `extracting` → `verifying` → `succeeded`/`failed`/`cancelled`, `downloadedBytes`/`totalBytes`, a `revision` that rises on every change, `installedCurrent`, `updatesAutomatically: true`, `damaged`, `canRemove`. `ManagedRuntimeInstallRow.tsx` renders it; there is an explicit **Cancel**.

### 4.3 No console

`src/packages/process-host/src/spawn-resolution.ts`:

```ts
// GUI-subsystem parents otherwise spawn visible consoles that steal foreground input.
windowsHide: true,
…
// shell:true loses argument escaping and disables windowsHide.
shell: false,
```

and `windows-cmd-shim-resolution.ts` resolves an npm/pnpm `.cmd` shim to `node.exe <script>` directly, avoiding `cmd.exe` entirely — the same idea Retinue already has in `resolve_npm_cmd` (`crates/retinue-runtime/src/lib.rs:100-151`). Task kill uses `windowsHide: true` too (`SpawnWorker.ts`).

### 4.4 Failures, admin rights, Pi

- Failures are a state on the row with the installer's own output (bounded), never a dialog with a command to copy; a manual-only provider answers `{ok:false, output: "Run this to update: …"}`.
- **Never admin:** everything is written under the user's data folder; no elevation path exists in the managed installer.
- **Pi is not managed by Orca.** `AGENT_CLI_UPDATE_AGENTS` lists claude, codex, grok, opencode, opencode2, omp, copilot, kimi, cursor, hermes, goose, aug, kilo, mimo-code, qwen-code — **no `pi`**. Orca launches the owner's `pi` and maps its JSONL session events. (A community issue in the repository claims Orca "bundles the pi runtime" and names a `src-tauri/src/pi_rpc_bridge.rs`; Orca is Electron and has no such file, so I treat that issue text as unreliable.) `omp` (oh-my-pi) is managed.
- Managed set today: Claude, Codex, Antigravity, OMP, Copilot, Grok, Kimi, and ACP-registry agents.

---

## 5. T3 Code (`pingdotgg/t3code`)

### 5.1 The update runs inside the server, not the window

Provider updates are a server feature: `apps/server/src/provider/providerMaintenanceRunner.ts` runs the command with `ChildProcess.make(resolved.command, resolved.args, { shell: resolved.shell, stdin: "ignore" })` and, in its own words, "Nothing answers an update command, so one that reads stdin must see EOF, not wait". The desktop app is a client of that server, so an update started from a phone or another computer runs where the CLI lives.

Scale facts worth copying:

- `UPDATE_TIMEOUT_MINUTES = 30`, "Native updaters (`claude update`) download a ~300 MB binary without printing progress, and killing one mid-download leaves a truncated binary that the next run treats as installed";
- `UPDATE_OUTPUT_MAX_BYTES = 10_000`;
- progress sampled at most once a second, 200 characters, "because every progress update resends the provider list to all clients";
- a partial line is capped at 4 KiB;
- after the run it re-reads versions and requires `installedVersion` to match what was expected, and it says when OpenCode needs its local server restarted.

### 5.2 Which command: the installer that owns the CLI

`packages/provider-core/src/server/maintenanceResolver.ts` inspects the command's real path and classifies it: npm global, pnpm global, yarn global, bun global, Volta, Vite+, mise (shims, installs, wrappers), Homebrew/Caskroom, Chocolatey, WinGet, and the Nix store ("read-only, so no updater can replace a CLI that lives there"). Each class yields the command that owns the install (`npm install -g <pkg>@latest`, `brew upgrade`, `mise upgrade`, `choco upgrade`, …). Providers it cannot classify are **manual only**, and the UI says so. `apps/server/src/provider/providerInstallation.ts` adds **managed** installs for two providers (Codex via ChatGPT, Antigravity), whose releases it downloads.

The user-facing contract is `docs/user/install.md` § Providers: "**Update now** runs the installer that owns the CLI (Homebrew, mise, or a global npm, pnpm, Yarn, Bun, Volta, or Vite+ install), or the CLI's own update command when T3 Code cannot tell. Claude, Codex, and OpenCode installs T3 Code cannot identify stay manual."

### 5.3 No console

- The process layer sets `windowsHide: true` for probes and task kills (`apps/server/src/process/SpawnWorker.ts`, `packages/shared/src/shell.ts`), and `shell: false` in `resolveSpawn` (`packages/shared/src/shell.ts`) — with the same reason Orca gives.
- The update path itself relies on the server process: run as a background service (`t3 service install`, `docs/user/install.md`) there is no window, and run in the owner's terminal there is the one they already have. **I did not verify** what happens when the desktop app starts that server on Windows (2.3).

### 5.4 Where T3 and Retinue's situations differ

T3's model needs a server process that is not the GUI shell. Retinue has exactly that: `retinued`, started with `CREATE_NO_WINDOW` (`apps/desktop/src-tauri/src/lib.rs:519`) and also able to run headless (`crates/retinue-daemon/src/paths.rs:26-31`). So T3's "do it in the daemon" is the right shape for Retinue, and the flag work is Retinue's own.

---

## 6. Synara (`trysynara.com`)

Synara **does not install or update anything**. `docs/providers` says the runtimes are "installed on your machine" and gives, per provider, "Install the official runtime — Use the provider's official installation method… Authenticate outside Synara — Start the runtime in a normal terminal… Verify the executable". It publishes install commands in its guides (for example `npm install -g @anthropic-ai/claude-code`) and warns that "an old CLI can be installed and authenticated while still being too old for the integration Synara expects". [doc]

So Synara is Retinue's status quo with better documentation: a hint, and a terminal. It is worth citing to the owner as the alternative that does not solve the problem. (Paseo, a similar app, is the same: "If the CLI is missing, the page of the provider says **Not installed**, and **Install** opens its install instructions." [doc])

---

## 7. Pi's mark and licence

- **The official mark is published at `https://pi.dev/logo-auto.svg`** — a `HEAD` request answered `200` with `content-type: image/svg+xml` [measured]. The npm package `@earendil-works/pi-coding-agent` uses exactly this file as its "Pi logo" in its README image (`https://pi.dev/logo-auto.svg`, wrapped in GitHub's camo URL) [doc]. `https://pi.dev/logo.svg` also answers `200 image/svg+xml` [measured]; I did not download either.
- In the source repository, the mark is drawn by `apps/p2/src/web/PiLogo.tsx`, and the web app's icons live in `apps/p2/public/icons/` (`favicon.svg`, `icon-192.png`, `icon-512.png`) — `github.com/earendil-works/pi`, tree filtered for `logo|brand|press|icon` [code]. There is **no press-kit folder** in the repository.
- **Licence: MIT.** `LICENSE` begins "MIT License / Copyright (c) 2025 Mario Zechner" [measured]; the repository README and the npm page both say MIT. The package's `license` field is `MIT` [measured].
- **Caveat for the notices file:** MIT covers the software. A trademark or logo is not normally granted by an MIT licence, so using the Pi mark to name the Pi agent is **nominative use** (identifying the product the bot runs on), and it should be recorded as such in `docs/legal/third-party-notices.md` with the source URL, the fetch date and a sentence saying Retinue claims no rights in the mark. There is precedent in that file for recording provenance this way (`:13-18`).

**What the code change would be:** add `apps/desktop/src/assets/providers/pi.svg` (or `pi-light`/`pi-dark` if the mark needs both), and add `pi: { name: 'Pi', mark: 'P', file: 'pi' }` to `providerBrand.tsx:7-19`. `ProviderLogo` already prefers `<file>-light`/`<file>-dark` and falls back to the plain file (`:30-31`), so one file is enough if it reads on both looks. **The owner's rule for bot art applies to vendor marks as it does to everything else** (design-language: no recolouring that misrepresents a brand), so the file is used as published, not tinted.

---

## 8. Proposed design

### 8.1 Where the code lives

**The daemon**, in a new module `crates/retinue-daemon/src/agent_install.rs`, with RPCs on the `runtime.*` family (which is already `never` for devices):

| Method | Purpose |
|---|---|
| `runtime.install.plan {provider}` | what installing would do: mode, source host, version, bytes, digest prefix, target folder, whether a turn is running |
| `runtime.install {provider, confirm: true}` | start the download/update; answers a job id |
| `runtime.install.status {jobId}` | phase, bytes done/total, the last line of installer output (bounded), error |
| `runtime.install.cancel {jobId}` | cancel, removing the staging folder |
| `runtime.install.remove {provider, confirm: true}` | delete a managed copy (never the owner's own) |

Reasons: one implementation for the window and the headless host; the golden rule that the daemon owns decisions; the console client (`crates/retinue-daemon/src/console.rs`) can then offer `retinued agents install <provider>` with no second implementation; and the running daemon already owns the process trees that must not be disturbed. The shell keeps only a button and a progress row.

Why not the shell, where `harness_updates.rs` sits: the shell cannot serve a headless host, it is the process that must never show a window, and it would put download-and-execute in the GUI process, which is the worse of the two places to put it.

An HTTP client in the daemon needs an allowlist entry with a reason in `scripts/check-architecture.mjs` (`ALLOWLIST`, `daemon-network`, `:64-116`; the check fails on a missing or a stale entry). **[code]**

### 8.2 Per-CLI sources, pins and digests

"Per-user, no admin" is guaranteed by construction for **managed** copies: everything is written under Retinue's data folder (`crates/retinue-daemon/src/paths.rs:66-68`), which is the owner's own `%LOCALAPPDATA%` or `~/.local/share`.

| Provider | Source for a managed copy | Digest published | Windows build | Needs Node? |
|---|---|---|---|---|
| **Claude Code** | `downloads.claude.ai/claude-code-releases/<channel>/manifest.json` → `platforms["win32-x64"].binary` | **sha256** per platform, plus `size` [measured] | `claude.exe`, 367,634,080 B for 2.1.323 [measured] | no |
| **Codex** | GitHub `openai/codex` release `rust-v*` → `codex-x86_64-pc-windows-msvc.exe` (or `codex-package-x86_64-pc-windows-msvc.tar.gz`) | `codex-package_SHA256SUMS` [measured] | 407,421,232 B raw, 214,982,795 B tar.gz [measured] | no |
| **OpenCode** | npm `opencode-ai` (1.x) / `@opencode/cli` (2.x): `dist.integrity` (sha512) + `dist.tarball`; the GitHub zip exists but **publishes no checksum** [measured] | sha512 via npm `integrity` | `opencode-windows-x64.zip` 81,943,749 B, no checksum asset [measured] | yes (the package is a launcher; the platform binary is an optional dependency) |
| **Pi** | GitHub `earendil-works/pi` release → `pi-windows-x64.zip` | **`SHA256SUMS`**, `f5defb6c… pi-windows-x64.zip` [measured] | 61,995,847 B [measured] | **yes for npm** (`engines.node >= 22.19.0`); the zip's contents are unverified |
| **Antigravity** | Google's manifest `<run.app>/manifests/<platform>.json` → sha512 + a URL under `storage.googleapis.com/antigravity-public/` (as Orca does) | **sha512** [doc] | `agy.exe` | no |
| **Grok CLI** | npm `@xai-official/grok` + platform optional dep; or the CLI's own `grok update` | sha512 via npm `integrity` | 18 KB launcher + platform package [measured] | yes |
| **Cursor CLI** | the vendor's PowerShell installer, or the ACP registry version check + a manual command | **none found** | — | — |
| **Qwen Code** | npm `@qwen-code/qwen-code` | sha512 via npm `integrity` [measured] | `cli-entry.js` | yes |

Two structural findings from the registry metadata **[measured]**:

1. **Every npm CLI except Pi ships its real binary in a platform-specific optional dependency** — `@anthropic-ai/claude-code` → `@anthropic-ai/claude-code-win32-x64`; `@openai/codex` → `@openai/codex-win32-x64`; `opencode-ai` → `opencode-windows-x64`; `@xai-official/grok` → `@xai-official/grok-win32-x64`; `@opencode/cli` → `@opencode/cli-windows-x64`. Downloading only the top-level tarball gives a launcher that cannot find its binary. A managed npm install must fetch the platform package too (each with its own `dist.integrity`), or use the publisher's native release instead — which is why the table above prefers native releases wherever a digest exists.
2. **Pi is a plain JavaScript package** (`bin: dist/bundle/cli.js`, 24.8 MB unpacked, 22 dependencies). Its npm route needs Node 22.19+; its own installer (`https://pi.dev/install.sh`) installs Node interactively when it is missing and runs `npm install` under the hood with `--min-release-age=0`, so it is not a digest-verified download either.

**Recommendation for v1:** ship one-click **Install/Update** for **Claude Code, Codex, OpenCode and Pi** from the sources in the table, and leave Cursor, Grok, Antigravity and Qwen on "open the install page" until their digest story is settled — except Antigravity, which is cheap to add because Google publishes a sha512 manifest. The optional providers are off by default anyway (`crates/retinue-runtime/src/providers.rs:13-15`), so v1 covers what almost every owner uses.

### 8.3 Layout on disk, and the receipt

```
<data>/agents/<provider>/
    <version>/                        # read-only payload, never overwritten
    staging/<uuid>/                   # download and extract here; deleted on any failure
    current.json                      # {version, entry, digest, bytes, installedAt, sourceHost}
    lock                              # one operation per provider at a time
```

- The download goes to `staging/<uuid>/file.part`, is hashed **while streaming** and compared with the pinned digest before anything is unpacked. A mismatch deletes the staging folder and answers one sentence ("The download did not match the publisher's checksum. Nothing was installed.").
- Archives are unpacked entry by entry: an entry name is joined to the staging folder, `canonicalize`d, and **refused if it leaves it**; absolute names, `..`, symlinks and hard links are refused; a total unpacked-size cap applies (zip-bomb). No script from an archive is ever run — not `install.js`, not `postinstall`. (`project_share.rs` already uses this canonicalise-and-compare discipline for paths; the same helper shape.)
- `current.json` is written **last**, temp-and-rename, and is the only thing discovery reads. A version folder without a receipt is **damaged**: listed, not launched, repairable by reinstalling.
- On Linux: `0755` for the executable, `0644` for the rest, and a `0600` staging folder (the convention `secrets.rs` and `console.rs` already use).

### 8.4 Two modes: adopt, or manage

- **Adopt** — the owner already has the CLI. Retinue works out what owns it from the resolved real path, the way T3 does: an npm global prefix under the user's profile (`%APPDATA%\npm`), a native install folder, a WinGet link under `%LOCALAPPDATA%\Microsoft\WinGet`, a Homebrew prefix. Then it runs **that** owner's update (`npm install -g <pkg>@<version>`, or the CLI's own `update`/`upgrade`), windowless, and verifies the version after. If the owner is machine-wide (`C:\Program Files\nodejs`, `C:\ProgramData\chocolatey`), it **refuses** rather than raising a UAC prompt, and offers Manage.
- **Manage** — Retinue downloads a pinned build into `<data>/agents/…` and launches it by path. No admin, no PATH change, no interference with the owner's copy, and a version Retinue can state exactly.

Which one a row uses is a stored choice (`agents.<provider>.mode`), defaulting to Adopt when an install was found and Manage otherwise. The owner can switch, and **Remove** only ever deletes a managed copy.

The reason to keep Adopt at all: it keeps one install. The owner runs `claude` in a terminal; a managed copy would be a second, differently versioned Claude on the same machine — the confusing situation Orca's `findOwnAgentCli` exists to avoid.

### 8.5 Discovery and the runtime id

Discovery must learn one step: after `PATH`, look in `<data>/agents/<provider>/current.json` for the managed entry. Prefer **PATH first** — the owner's own install wins, as in Orca — and use the managed copy only when nothing was found or when the row's mode is Manage.

Two details that will bite if unplanned:

1. **The runtime id hashes the executable path** (`crates/retinue-runtime/src/lib.rs:272`). A new version folder would give every update a new `runtimeId`, and every bot, session and `agents.runtimeId` pointing at the old one would look orphaned (`ui/SettingsSheet.tsx:549` already lists orphans). Fix: for a managed copy, hash **the provider's managed root** (`<data>/agents/<provider>`), which never changes, while `executablePath` still names the versioned file. No junction, no migration, no link privileges.
2. **Session start reads the current snapshot.** `exec_options_for_session` (`crates/retinue-daemon/src/main.rs:2380`) builds the environment; the adapter resolves the runtime and spawns. So after a flip, the *next* process start uses the new version — which is not the same as the next turn (8.6).

### 8.6 Protecting a running bot

- **Never overwrite a file a process is running.** Each version has its own folder; the swap is a rewrite of `current.json`. On Windows an in-use `.exe` cannot be deleted, so the old folder simply stays until it can be — the sweep runs at daemon start and after each install, tries to remove, and leaves anything that is in use.
- **A turn in flight keeps its process.** The Microsoft Job Object already kills a whole tree with its owner (`crates/retinue-process/src/lib.rs:110-204`), so nothing about an update can reach a running turn.
- **The shared adapter process is the gap.** The Codex and ACP adapters reuse one child per provider+environment (`crates/retinue-daemon/src/main.rs:2389-2391` describes the key). After a flip, that idle child keeps the old version until it exits. The install must, after publishing `current.json`: `runtime.refresh`, close idle shared processes for that provider, and **leave a running one alone** — the row then says "takes effect after this turn".
- **Keep one previous version** for "Go back", until the next successful install; sweep the rest only when nothing can be running them.
- **When to refuse.** An install while that provider has a running turn is safe in Manage mode, because the version folders never collide, and unsafe in Adopt mode, because it writes into the owner's only copy. So: Manage proceeds and says "takes effect after this turn"; Adopt waits or refuses with one sentence.

### 8.7 Never a console window

**One helper, used everywhere.** Extend `retinue_process::prepare_command` (`crates/retinue-process/src/lib.rs:12-25`) to set `CREATE_NO_WINDOW | CREATE_SUSPENDED` — `0x0800_0004`, exactly the pair `project_setup.rs:231-238` already uses, with its own comment explaining that the child is paused until the job is attached. Then every adapter, probe, git call and installer that goes through it is windowless by construction. For processes that are not job-owned there is a second tiny helper (`hide_window`) that sets only `0x0800_0000`.

**Then fix the sites that do not go through it** (section 9). The two that produce the owner's complaint are `harness_updates.rs:88` and `:136`.

**Add a gate, in the style the repository already uses.** `fix39_p12_tests::every_log_call_site_is_a_fixed_message` scans every `tracing::` call site; a sibling test can scan `Command::new(` in `crates/**/src/*.rs` and `apps/desktop/src-tauri/src/*.rs` and fail when the following lines contain no `prepare_command`, no `creation_flags(0x0800_0000`/`0x0800_0004` and no `hide_window`. That is cheaper and more durable than a runtime check, and it makes the next spawn correct by default.

### 8.8 Consent, progress and failure

- **Consent.** The first install per provider shows a card naming: provider, version, the **exact source host**, byte size, and the first 12 hex characters of the digest, plus where it goes. Stored as `agents.<provider>.consent = {version, digest, sourceHost, at}`. With `agents.<provider>.autoUpdate` off — the default — every new version gets its own card; only when the owner turns that switch on does a later version from the same host install without one. The running app's own updater (`apps/desktop/src-tauri/src/updates.rs`, signed manifest) is a different mechanism and stays as it is; this is a per-CLI download.
- **Progress.** A job row in Settings › Providers and in the notice: `Downloading 41% · 21 MB of 62 MB`, `Checking the download`, `Installing`, `Done · 1.6.0`, or a failure with **the installer's own last line**, redacted like every other line (`diagnostics::redact`) and capped at 200 characters. Phases as T3 and Orca both have them, because a 368 MB download with a spinner is indistinguishable from a hang.
- **Failure sentences** are fixed text plus the one bounded line: a checksum mismatch, a refused folder, no network, a timeout, "the CLI still reports the old version" (the Adopt case T3 and Orca both guard), and "this install belongs to a system-wide package manager; Retinue will not raise an admin prompt".
- **Logging.** Provider, version, host, phase, bytes and outcome — never the file's contents, never a whole URL with a query, never a path inside a secret folder. One content-free activity line on success and on refusal (the remote activity list is the wrong place; this is local, and `diagnostics::record_failure` covers the failure).
- **Disk.** Claude is a 368 MB download that is the one executable (its manifest publishes a bare `claude.exe`), and Codex's 215 MB tarball unpacks to a 407 MB executable. Keeping one previous version of each is roughly 2 GB under the data folder, beside the app's own installer. The plan call states the size and the free space, and refuses when there is not enough. This is the largest cost of the design and the owner should see it before deciding.

### 8.9 Remote devices and the headless host: proposed `never`

`provider.*` is already `never` (`apps/desktop/src/remote/tiers.json:264`, "Which coding agents are installed and switched on is the owner's, at the desktop"), and `runtime.refresh` (`:894`) and `runtime.auth.clear` (`:901`) are `never` for the same reason. Every new method follows:

- `runtime.install.plan`, `.install`, `.status`, `.cancel`, `.remove` — class **`never`**, each with its own exact row (so the parity test names them rather than hiding them under a family), and the events `runtime.install.changed` — **`drop`**.
- The Retinue MCP has no install tool, and none may be added: an outside session reaches exactly what the window reaches through `machine.*` only (`external.rs` `catalog()`).
- A machine link cannot call them: `machine.*` has no install pass-through, and none should be added.
- The headless host: a new console method `console.agent.install` in the `console.*` family (`never`, token file only, `crates/retinue-daemon/src/console.rs`), and a CLI verb `retinued agents install <provider>` registered in the `daemon_main` argument match beside `pair`/`remote`/`status` — `tests/console_cli.rs` fails when a verb is missing from that match, which is the trap `lessons.md` records (2026-11-20).

### 8.10 The threat-model rows to add

A new section **25. Installing and updating coding-agent CLIs from inside the app (21 November 2026)** in `docs/remote/THREAT_MODEL.md`, after the projects section, with the shape the file already uses (abuse, mitigation, residual risk):

| # | Abuse | Mitigation |
|---|---|---|
| A1 | **A new download-and-execute path on the host** | HTTPS only; a fixed host allowlist (`downloads.claude.ai`, `registry.npmjs.org`, `github.com` → its release-asset hosts, `cdn.agentclientprotocol.com`, Google's Antigravity manifest and bucket); no proxy; redirects only to named hosts; a size cap per provider and a 30-minute limit; one install per provider at a time |
| A2 | A digest that proves integrity but not intent (a compromised publisher account, a swapped CDN object) | The digest is the publisher's, fetched over TLS from the publisher's own host; it is compared **before** anything runs; the owner's card names the version and the first 12 hex characters; auto-update is off by default; a new version needs a click. Residual risk: a publisher who ships a bad build within its own digest is indistinguishable from a good one |
| A3 | A half-written download, or a republished build with the same version, being launched | Staging folder, receipt written last through a temp file and a rename, the receipt keyed by the **digest** not only the version; a folder without a receipt is damaged and never launched |
| A4 | An archive entry escaping the install folder | Every entry joined, canonicalised and compared; absolute names, `..`, symlinks and hard links refused; unpacked-size cap; no script from an archive is ever executed |
| A5 | Replacing the binary under a running bot, corrupting a turn | Versioned folders; nothing is overwritten; the new version is current only in `current.json`; a running child is owned by a Job Object and is never touched; the previous version is kept for a rollback; sweeping skips any file in use |
| A6 | An install raising an admin prompt or changing PATH | Everything is written under the owner's data folder; a machine-wide package manager is refused rather than elevated; PATH is never edited |
| A7 | A paired phone, browser or another machine starting an install | `runtime.install.*` are `never` at every tier for every device kind, with exact rows; no MCP tool; no `machine.*` pass-through; a console install needs the console token file, which exists only on a headless start |
| A8 | An install draining disk or bandwidth unattended | Installs start only from an owner click or a console command; no automation or task tool can reach one; the plan states the size and refuses without free space; one per provider |
| A9 | A downloaded file's path or contents reaching a log, an event, a backup or a diagnostics bundle | The log line is provider, version, host, phase and outcome; no file contents, no full URL, no folder beyond `<data>`; events carry `{provider, phase, version}` only |
| A10 | A bot talking the owner into installing a different agent (a prompt-injection path) | The install tool is not in a bot's tool list at all; the card is the owner's, with the version and digest, and no message text can preselect it |

Also add two rows to the **invariants** page (`docs/reference/invariants.md`), each with its test:

- "An install or update downloads only over HTTPS from an allowlisted host, compares the publisher's digest before executing anything, runs no script from an archive, and never overwrites a running binary." — `agent_install.rs`, `provider_install_tests.rs`.
- "No process Retinue starts shows a console window on Windows." — `retinue-process` (`prepare_command`), the new spawn-scan gate in the daemon tests, and `harness_updates`' own test.

### 8.11 Tests and gates

- `provider_install_tests.rs` with a local stub origin (`INSTALL_TEST_ORIGIN`, the pattern `GH_STUB`/`TEST_ROOTS` already use) serving a tiny archive and its digest: a good install, a wrong digest, a truncated download, a `..` entry, a symlink entry, a damaged receipt, an in-use previous version, a cancel, and `runtime.install` refused for a device.
- A goldens test for the per-provider plan table, so a new provider without a source cannot be added silently.
- The spawn-scan gate (8.7).
- `remoteTiers.parity.test.ts` and `policy.rs` goldens gain the five `never` rows and the `drop` event.
- Gates to run: `npm run typecheck`, `npm run test`, `cargo test -p retinue-daemon -p retinue-runtime -p retinue-process`, `npm run check:architecture` (the new `daemon-network` entry), `npm run check:format`, `npm run docs:build` (the docs library mirrors `docs/**`; a new report needs it), `node scripts/ci-scope.mjs` picks the jobs.
- What was actually run for this report: `node scripts/check-format.mjs` → "Formatting check passed (1526 files)". `node scripts/docs-library.mjs check` could not run in this worktree because `marked` is not installed here (`npm install` has not been run); the Director should run `npm run docs:build` and then `npm run docs:check` in the main checkout, because the new report is not in the library catalogue until the build runs.

### 8.12 Demo coverage and the docs the change makes stale

- The owner's rule of 21 November: every live screen and state is demoable in the preview. The install surfaces need fixtures — available, downloading with a percentage, verifying, damaged, failed with a line, "takes effect after this turn" — added to `preview/fixtureBridge.ts` as `?preset=agent-install|agent-install-progress|agent-install-failed|agent-install-damaged`, and a row in `docs/reports/preview-demo-inventory.md` (which already lists `?preset=harness-update` and `-progress` at `:336`).
- `docs/library/content/reference/runtimes-and-adapters.md`, `docs/reference/runtime-capabilities.md`, `.claude/skills/retinue-development/references/codebase-map.md` (the runtime section) and `docs/operations/` (the pinned version and licence of anything third-party we download, as the 14 November amendment requires for a supervised child) all go stale the day this ships.
- `docs/legal/third-party-notices.md` gains the Pi mark line (section 7).

---

## 9. The window-showing spawn inventory

Every `Command::new` in `crates/` and `apps/desktop/src-tauri/src/` (tests excluded), sorted by whether it can show a console window today.

### 9.1 No creation flag at all, and a console program — can show a window

| Where | Program | Why it matters |
|---|---|---|
| `apps/desktop/src-tauri/src/harness_updates.rs:88` | `powershell.exe` (the update) | **the owner's reported window**; a GUI-subsystem parent |
| `apps/desktop/src-tauri/src/harness_updates.rs:136` | `powershell.exe` (the `--version` read) | a second flash after every update |
| `apps/desktop/src-tauri/src/pty.rs:574` | `taskkill.exe` | the terminal dock's stop |
| `crates/retinue-daemon/src/diagnostics.rs:521` | `cmd /c ver` | the Doctor's OS line |
| `crates/retinue-daemon/src/owner_computer.rs:505` | `taskkill` | stopping the computer driver |
| `crates/retinue-daemon/src/owner_computer.rs:513` | the computer-use driver | a child per call, if the driver is a console app |
| `crates/retinue-wsl/src/lib.rs:5` | `wsl.exe` | WSL discovery |
| `crates/retinue-daemon/src/remote/native.rs:392` | `icacls` | the TLS key's permissions |

For all eight the fix is the same helper.

### 9.2 Flagged with `CREATE_SUSPENDED` only (`0x4`) — job-owned, but not `CREATE_NO_WINDOW`

`retinue_process::prepare_command` (`crates/retinue-process/src/lib.rs:21,37`) and every child that goes through it:

- probes: `crates/retinue-runtime/src/lib.rs:184`, `:761`; `crates/retinue-runtime/src/account.rs:176`, `:204`;
- Claude: `crates/retinue-adapter-claude/src/lib.rs:59`, `:790`, `:1031`, `:1097`;
- Codex: `crates/retinue-adapter-codex/src/lib.rs:310`, `:1337`, `:1411`, `:1493`;
- ACP: `crates/retinue-adapter-acp/src/lib.rs:329`, `:1367`; `src/catalog.rs:51`, `:121`;
- Pi: `crates/retinue-adapter-pi/src/lib.rs:158`, `:365`;
- the Pi page's probe: `crates/retinue-daemon/src/pi.rs:283`;
- the owner's computer: `crates/retinue-daemon/src/owner_computer.rs:513` (listed above too);
- `project_setup.rs:237` already adds `0x0800_0000` itself, which is the pair to copy.

Whether these flash today is the open question in 2.3. The design removes the question.

### 9.3 Correctly hidden today (do not regress)

`crates/retinue-daemon/src/repository.rs:256-257`, `repo_identity.rs:207-210`, `scaffold.rs:64-65`, `experiments.rs:18-24`, `project_clone.rs:272,339`, `project_setup.rs:237`, `sandbox.rs:45,101`, `worktree_attach.rs:84`, `media.rs:942`, `preview.rs:358`, `remote/tailscale.rs:136-137`, `remote/connectivity.rs:516-517,661-662`, `pull_request.rs`, `worktree.rs`, `worktree_cleanup.rs`, `crates/retinue-speech/src/needle.rs:121` (`0x0800_0000`), and the daemon itself at `apps/desktop/src-tauri/src/lib.rs:519`.

### 9.4 Visible by design (leave alone)

| Where | What | Why |
|---|---|---|
| `apps/desktop/src-tauri/src/lib.rs:2252-2260` | `wt.exe` / `powershell.exe` with `CREATE_NEW_CONSOLE` | the owner's sign-in terminal; the owner types into it |
| `apps/desktop/src-tauri/src/lib.rs:1921` | the configured editor | an editor, not a console |
| `apps/desktop/src-tauri/src/lib.rs:1957` | `explorer.exe` | no console of its own |
| `crates/retinue-daemon/src/media.rs:949` | `xdg-open` | Linux file manager |

The sign-in window stays, and should: Codex prints a URL and waits, Claude Code needs `/login` typed, and both may need a 2FA browser. Only the **install** half of `runtime_terminal` should disappear once installs are in-app (`SignInHelp.tsx:56-64`'s `mode="install"`).

---

## 10. Open decisions for the owner

1. **Adopt, manage, or both?** Recommendation: both, with Adopt tried first and Manage as the fallback (and the default when Retinue installed it). The alternative — managed only — is simpler but puts a second copy of Claude on the machine and leaves the owner's own `claude` untouched and still old.
2. **Auto-update per provider: off or on?** Recommendation: **off**, one click per update, with a per-provider "keep this one up to date" switch for later. A silent 368 MB download is a surprise on a metered connection.
3. **Which providers get one-click in v1?** Recommendation: Claude Code, Codex, OpenCode, Pi; Antigravity next (Google publishes a sha512 manifest); Cursor, Grok and Qwen stay "install page" until a digest exists.
4. **Pi without Node.** Recommendation: require Node 22.19+ for Pi in v1 (the machine already runs Node CLIs), and record "bundle a pinned Node" as a later option, the way Orca does (`src/main/node-runtime/`). If the owner wants Pi on a Node-less machine, verify first whether `pi-windows-x64.zip` is self-contained (2.3).
5. **The disk cost.** Claude (~368 MB) plus Codex (~215–407 MB) plus a previous version each is 1–2 GB under the data folder. Is that acceptable, and should the app keep one previous version or none?
6. **Claude's channel.** Follow the CLI's own `autoUpdatesChannel` (recommended, as Orca does) or pin to the app's own update channel?
7. **Does the headless Linux host install CLIs at all?** Recommendation: yes, through `retinued agents install <provider>` on the console token, because a VPS owner has no other way; the alternative is the current documentation-only hint.
8. **Does the terminal install button go away entirely?** Recommendation: yes for install; keep the terminal for sign-in.
9. **One wording point:** the notice today says "Harness update available" (`HarnessUpdatesNotice.tsx:70`). "Harness" is not a word this product uses anywhere else; "Coding agent update" fits the rest of the copy.

---

## 11. Sources

**Repository (this checkout).** `crates/retinue-runtime/src/{lib.rs,providers.rs,account.rs}`; `crates/retinue-daemon/src/{provider_gate.rs,pi.rs,paths.rs,console.rs,diagnostics.rs,owner_computer.rs,remote/native.rs,main.rs}`; `crates/retinue-process/src/lib.rs`; the five adapter crates; `crates/retinue-wsl/src/lib.rs`; `apps/desktop/src-tauri/src/{lib.rs,harness_updates.rs,updates.rs,pty.rs,main.rs}`; `apps/desktop/src/ui/{HarnessUpdatesNotice.tsx,SignInHelp.tsx,ProvidersPage.tsx,PiPage.tsx,providerBrand.tsx,providerGuides.ts,SettingsSheet.tsx}`; `apps/desktop/src/remote/tiers.json`; `scripts/check-architecture.mjs`; `docs/history/fix-35/c5-windows.md`; `docs/remote/THREAT_MODEL.md`; `docs/reference/invariants.md`; `docs/legal/third-party-notices.md`; `.claude/skills/retinue-development/references/{lessons.md,codebase-map.md}`.

**Orca** (`github.com/stablyai/orca`): `src/packages/process-host/src/spawn-resolution.ts`, `windows-cmd-shim-resolution.ts`, `SpawnWorker.ts`; `src/main/agent-cli-update/{agent-cli-update-service.ts,agent-cli-own-install.ts,agent-cli-update-agents.ts,agent-cli-latest-release.ts}`; `src/main/claude/managed-runtime/claude-managed-runtime.ts`; `src/main/antigravity/agy-managed-runtime.ts`; `src/main/omp/omp-managed-runtime.ts`; `src/main/node-runtime/{managed-runtime-release.ts,managed-runtime-version-check.ts}`; `src/shared/agent-launch/installation/{managed-runtime-install.ts,agent-cli-update.ts}`; `src/renderer/src/components/managed-runtime/ManagedRuntimeInstallRow.tsx`; README.

**T3 Code** (`github.com/pingdotgg/t3code`): `docs/user/install.md` (§ Providers); `apps/server/src/provider/{providerMaintenanceRunner.ts,providerInstallation.ts}`; `packages/provider-core/src/server/maintenanceResolver.ts`; `packages/shared/src/shell.ts`; `apps/server/src/process/{SpawnWorker.ts,windowsVerbatimSpawn.ts}`.

**Synara** (`trysynara.com`): `/docs/providers` ("Install the official runtime… Authenticate outside Synara… Updates: an old CLI can be installed and authenticated while still being too old"). Paseo (`paseo.sh/docs/supported-providers`) for the same pattern.

**Pi**: `github.com/earendil-works/pi` (`LICENSE`, `apps/p2/src/web/PiLogo.tsx`, `apps/p2/public/icons/`); `npmjs.com/package/@earendil-works/pi-coding-agent` (README logo `https://pi.dev/logo-auto.svg`); `https://pi.dev/logo-auto.svg` (HEAD 200, `image/svg+xml`); `https://pi.dev/install.sh`; release `v1.6.0` assets and `SHA256SUMS`.

**Publishers' release data**, fetched today: `https://downloads.claude.ai/claude-code-releases/{stable,latest}` and `/{version}/manifest.json`; `api.github.com/repos/{openai/codex,earendil-works/pi,sst/opencode}/releases/latest`; `registry.npmjs.org/{@anthropic-ai/claude-code,@openai/codex,opencode-ai,@opencode/cli,@earendil-works/pi-coding-agent,@xai-official/grok,@qwen-code/qwen-code}/latest`.

---

## Appendix A — what the requests answered today (21 November 2026)

| Package | `latest` | Integrity published | Platform binary | `engines` |
|---|---|---|---|---|
| `@anthropic-ai/claude-code` | 2.1.330 | sha512 `dist.integrity` | optional `…-win32-x64`; `bin: bin/claude.exe` | node ≥22 |
| `@openai/codex` | 0.171.0 | sha512 | optional `…-win32-x64`; `bin: bin/codex.js` | node ≥16 |
| `opencode-ai` | 1.19.6 | sha512 | optional `opencode-windows-x64`; `bin: bin/opencode.exe` | — |
| `@opencode/cli` | 2.3.2 | sha512 | optional `@opencode/cli-windows-x64` | — |
| `@earendil-works/pi-coding-agent` | 1.6.0 | sha512 | none (plain JS bundle, 22 deps, 24.8 MB) | **node ≥22.19.0** |
| `@xai-official/grok` | 1.0.92 | sha512 | optional `…-win32-x64` | node ≥20 |
| `@qwen-code/qwen-code` | 0.28.3 | sha512 | none (`cli-entry.js`) | node ≥22 |

| Release | Tag | Windows asset | Size | Digest file |
|---|---|---|---|---|
| Claude (native) | 2.1.323 stable on 2026-11-12 | `win32-x64/claude.exe` (from the manifest) | 367,634,080 | sha256 in `manifest.json` (`5064e6e0…`) |
| Codex | `rust-v0.171.0` | `codex-x86_64-pc-windows-msvc.exe`; `codex-package-x86_64-pc-windows-msvc.tar.gz` | 407,421,232; 214,982,795 | `codex-package_SHA256SUMS` (1,631 B) |
| OpenCode | `v1.19.6` | `opencode-windows-x64.zip` | 81,943,749 | **none** |
| Pi | `v1.6.0` | `pi-windows-x64.zip` | 61,995,847 | `SHA256SUMS`: `f5defb6c403ee698f5b3f8886db473e55b78aac670a8ce03620a47f5f0aec57c` |

`HEAD https://pi.dev/logo-auto.svg` → `200`, `image/svg+xml`. `HEAD https://pi.dev/logo.svg` → `200`, `image/svg+xml`. `LICENSE` in `earendil-works/pi` → MIT, "Copyright (c) 2025 Mario Zechner".
