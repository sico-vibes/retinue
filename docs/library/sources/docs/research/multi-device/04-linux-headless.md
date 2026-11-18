# A headless Linux `retinued`: what T3 does, what Retinue needs, and the VPS

Part of `01-t3-vs-retinue.md`. Date 17 November 2026. Retinue read at `b7c3a32` plus the working-tree edits (see `01`, header). T3 Code at `b387e3a`. Research only: nothing built, nothing run.

This file answers the Director's addition, part 1: **how T3 runs headless on a Linux server, and what a headless Linux `retinued` would need**. Part 2 of the addition — verifying the Director's own findings — is section 8, at the end.

---

# 1. What is on the VPS today, and why it cannot host sessions

The owner's VPS runs Harbor: one persistent Linux desktop **in a container**, reached over HTTP with a bearer token, which the daemon links to and drives (`crates/retinue-daemon/src/computer.rs:1-19`). The daemon's messages are explicit that this is a *tool*, not a host: "Harbor manages one persistent Linux desktop in a container", and it is woken and slept on demand (`computer.rs:3-6`). It holds the token in Credential Manager (`computer.rs:9-15`), and every method is `never` for a device (`computer.rs:17-19`).

Three facts make the container the wrong home for a daemon that must own sessions:

1. **It sleeps.** Harbor's whole point is wake and sleep; a daemon that sleeps takes its conversations, worktrees and running agent processes with it.
2. **Its lifetime is Harbor's.** Container lifecycle, ports and storage belong to Harbor, not to Retinue; a container recreated from an image loses whatever lived only inside it.
3. **Its network position is Harbor's.** Remote access must be reachable as an HTTPS origin on the tailnet; a container behind Harbor's own proxying is one more mapping to keep alive.

So: **Retinue's daemon belongs on the VPS host, beside the container, not inside it.** The container stays what it is for today — the employee's own Linux desktop, browser and GUI, reached by tools from a conversation that runs anywhere (`crates/retinue-daemon/src/browser.rs:1-20` runs Playwright *inside* the container, driven by the daemon).

---

# 2. How T3 runs headless on a Linux server

Five stages, each with a command the owner types.

| Stage | What happens | Evidence |
|---|---|---|
| 1. Install the CLI | `curl -fsSL https://t3.codes/install.sh \| sh` puts `t3` in `~/.local/bin`; the installer warns about `PATH` | `docs/user/install.md:11-29` |
| 2. Choose exposure | `t3 up` asks how other devices reach the machine (T3 Connect, local network, or both) and saves the answer; `t3 up --lan`, `--connect`, `--no-*` are the scripted forms | `docs/user/remote-access.md:12-27`, `:66-77` |
| 3. Run it as a service | `t3 service install` installs a per-user service; `status`, `restart`, `uninstall`, and `t3 update` for the release | `docs/user/background-service.md:11-43`; `apps/server/src/cli/service.ts:186` names "Linux with systemd, macOS with launchd, Windows with Task Scheduler" |
| 4. Sign in the agent CLIs | The provider CLIs must be on the **server's** `PATH`, each signed in there; `sh -lc "command -v claude codex"` is the check | `docs/user/install.md:211-232`, `docs/user/remote-access.md:252-257` |
| 5. Pair a device | `t3 up` prints a browser link and a short code to approve from any device; approval can also happen from a connected device or with `t3 auth connect-device approve <code>` | `docs/user/remote-access.md:29-64` |

**The service is a systemd user unit, and T3 treats it as a small systems problem.** The template is in code, not in a doc: `renderBootServiceUnit` builds `~/.config/systemd/user/t3code.service` (`apps/server/src/cloud/bootService.ts:100-145`; the path appears in `apps/server/src/cli/service.test.ts:24`). Its important lines, and the reason for each, which are worth copying:

```
[Unit]
StartLimitIntervalSec=300
StartLimitBurst=5
[Service]
Type=simple
WorkingDirectory=%h
Environment=T3CODE_HOME=…
ExecStart=<absolute path to the launcher>
KillMode=mixed
OOMPolicy=continue
Restart=always
RestartSec=5
CPUWeight=1000
IOWeight=1000
MemoryLow=1536M
OOMScoreAdjust=100
ManagedOOMPreference=avoid
StandardOutput=append:<log file>
[Install]
WantedBy=default.target
```

(`bootService.ts:105-144`.) The comments say why: the user manager has no reliable network-online target so the server retries networking itself (`:104`); the launcher marks an explicit stop before signalling, and systemd still SIGKILLs the cgroup on a timeout (`:120-122`); helpers share the cgroup, so the default `OOMPolicy` would stop the unit when one is OOM-killed (`:123-124`); the weights favour the server over agent processes (`:127-135`). `t3 service protect` is a separate opt-in that adds drop-in files so agents are the first to lose memory (`service.ts:401-459`).

**Two smaller facts worth knowing.** The unit cannot rely on the user's shell or `PATH` — that is stated as a rule in the code (`bootService.ts:99`, `:151`). And the server is Node, so the host downloads a pinned runtime into the T3 home rather than using whatever `node` is installed (`apps/server/src/cloud/pinnedRuntime.ts:32-60`; the unit's `ExecStart` points at a versioned path in tests, `pinnedRuntime.test.ts:599`). Retinue would not need that mechanism: `retinued` is a single Rust binary.

**What T3's headless host is not.** No terminal emulator beyond the agent's own subprocesses, no desktop, no GUI of any kind: the AUR package (`packaging/aur/t3code-bin/PKGBUILD:1-40`) is the *desktop* app, and the headless host is the CLI plus whatever the browser feature downloads (a headless Chrome, `docs/user/remote-access.md:280-283`).

---

# 3. What a headless Linux `retinued` actually needs

## 3.1 The numbers, verified

| Measure | Count |
|---|---|
| `retinue-daemon`: `#[cfg(windows)]` | 50 |
| `retinue-daemon`: `cfg(all(windows, …))` | 1 |
| `retinue-daemon`: `target_os = "windows"` | 2 |
| `retinue-daemon`: `cfg(unix)` or `cfg(not(windows))` | 18 |
| All crates: Windows-ish markers | 93 |
| All crates: Unix-ish markers | 50 |

The Director's "about 42 `cfg(windows)` sites against 16 non-Windows" is the right shape and slightly under the count: 50 exact `#[cfg(windows)]` in the daemon alone, 53 including `all(windows…` and `target_os`, against 18 Unix markers. The difference does not change the conclusion.

**What matters more than the count: most Windows sites are cosmetic, tests, or already mirrored.** Well over half of the daemon's 53 sites fall into one of these groups:

- `creation_flags(0x0800_0000)` (CREATE_NO_WINDOW) before spawning git or a CLI, so no console flashes: `experiments.rs:18-23`, `repository.rs:256-257`, `project_clone.rs:247-248`, `project_setup.rs:236`, `pull_request.rs:396`, `remote/tailscale.rs:105-111`. Harmless to keep behind `cfg`; the Unix side simply does not need it.
- Windows-only conveniences with no Linux equivalent needed: `explorer.exe /select` (`media.rs:936-946`), `netstat -ano` (`preview.rs:344-371`), `icacls` (`sandbox.rs:26-99`), the firewall prompt (`remote/connectivity.rs:513`, `:658`), the parent job object (`main.rs:6214`).
- Tests that need Windows to prove Windows behaviour — the largest single block, 12 of them in `crates/retinue-daemon/src/team_p10_tests.rs`, plus `phase1b_tests.rs:621`, `phase3_tests.rs:109`, `secrets_tests.rs:62`, `crates/retinue-daemon/tests/parent_watchdog.rs`, `crates/retinue-process/tests/process_tree.rs` and `worktree.rs:452-470`.

Several real blockers are **already solved**: ACLs have a Unix branch that sets `0700`/`0600` (`sandbox.rs:53-57`, `:119-123`); certificate keys get `chmod 0600` (`remote/native.rs:392-397`); process groups are set for Unix (`retinue-process/src/lib.rs:13-31`); executable extensions are `[""]` off Windows (`retinue-runtime/src/lib.rs:66-70`); and the hook bridge already chooses `retinue-hook` without `.exe` and quotes its path in a way `sh` accepts (`outside.rs:77`, `:86-88`).

## 3.2 The inventory, by area

| Area | What is Windows-only today | Why it matters on a headless host | Linux equivalent | Effort |
|---|---|---|---|---|
| Data folder | `%LOCALAPPDATA%/Retinue/retinue.db`, else the temp dir (`main.rs:6247-6254`) | Without `RETINUE_DB_PATH` the daemon writes to `/tmp` and loses everything on reboot | `$XDG_DATA_HOME/retinue`, else `~/.local/share/retinue`; keep `RETINUE_DB_PATH` as the override | S |
| Secrets and vault | Windows Credential Manager only; every call errors off Windows (`secrets.rs:155-165`); the vault stores values through the same module (`vault.rs:151-164`) | Bot secrets, webhook secrets, the Harbor token, vault items and Pi keys all fail | **Owner's decision**: a `0600` file under `<data>/secrets/`, or `systemd-creds`/`LoadCredentialEncrypted` where systemd has it (encrypted at rest, bound to the host); or declare bot secrets unsupported there | M |
| Build and CI | `windows-latest` only, for CI and the release (`.github/workflows/ci.yml:31`, `:98`; `release.yml:34`); the release builds `retinued` and `retinue-hook` as separate steps already (`release.yml:63-66`) | Nothing proves a Linux build exists | A Linux job: `cargo check -p retinue-daemon` + tests, then a second release job producing a tarball | S to M |
| Tailscale | Looks for `tailscale.exe` on `PATH`, then under `Program Files` (`remote/tailscale.rs:78-90`) | Remote cannot start on the VPS, which is the whole point of pairing it | Look for `tailscale` (no extension) in `PATH`, then `/usr/bin`, `/usr/local/bin`; `systemctl start tailscaled` guidance in the error text | S |
| Headless pairing | `remote.pair.begin` and `remote.pair.confirm` are window RPCs (`tiers.json:1440-1457`); the pairing request hard-codes `"platform": "windows"` (`machines.rs:726`, `:738`) | No way to approve the first device on a machine with no window | A console path: `retinued pair` prints a code (Tailscale) or takes the password (Native); `retinued pair confirm --tier chat` approves; the code is never logged. The platform field becomes `std::env::consts::OS` | S to M |
| Service and updates | Desktop autostart via `tauri_plugin_autostart` in the shell (`src-tauri/src/lib.rs:3509`); updates via the Tauri updater/MSIX (`src-tauri/src/distribution.rs:1-7`) | A headless host has no shell, so no autostart and no updater | A shipped `retinued.service` (user unit, `Restart=always`, `WantedBy=default.target`) plus an env file; update = replace the binary and `systemctl --user restart retinued`; optionally `retinued service install/status` mirroring T3 | M |
| Interactive terminal | `openpty` lives in the Tauri shell (`apps/desktop/src-tauri/src/pty.rs:346`), not the daemon | A headless host has no terminal feature; acceptable, but it must be absent cleanly rather than broken | None needed; the window's terminal only exists on a desktop | — |
| Dictation | `retinue-speech` runs Cactus's `needle.exe` (`crates/retinue-speech/src/needle.rs:1-6`) and is not a daemon dependency (`crates/retinue-daemon/Cargo.toml:10-56`) | Dictation is a desktop feature and stays there | None; the phone can still use its own. Note the cloud-dictation path would work from a Linux desktop later | — |
| Owner's computer | `owner_computer.rs` drives Windows UI Automation through a PowerShell bridge (`owner_computer.rs:1-20`), with no Windows API imports of its own | It would compile off Windows and then fail at run time, offering a tool that cannot work | Refuse at run time off Windows with the existing sentence ("works only on the owner's Windows computer"), and do not offer the MCP spec there; no Linux driver is in scope | S |
| Browser | Playwright runs **in the Harbor container**, driven from the daemon (`browser.rs:1-20`, `:75-142`) | Unaffected: a headless daemon still needs a linked computer for browser work | None | — |
| Preview ports | `netstat -ano` (`preview.rs:344-371`) | The preview panel loses its "which port is this" list on Linux | `/proc/net/tcp` + `/proc/<pid>/fd` scan, or accept the loss at first | S |
| Reveal in folder | `explorer.exe /select` (`media.rs:936-946`) | "Show in folder" does nothing | `xdg-open <dir>` | S |
| Native method | Firewall rule through the Windows prompt (`remote/connectivity.rs:513`, `:658`) | Native on a VPS would need a rule the owner adds himself | Refuse the firewall step with a sentence naming `ufw`/`nft`; UPnP (`igd-next`) already works cross-platform; TLS key permissions already handled | S |
| Sign-in copy | `accounts.rs` emits a **PowerShell** one-liner for the owner to paste (`accounts.rs:59-78`) | Wrong shell on a Linux host | A `sh` variant; the window should print what the host's OS needs, using the platform the link now reports | S |
| Front-end asset | The gateway serves the phone PWA from `RETINUE_REMOTE_WEB_DIR` (`remote/web.rs:1008-1028`), which the shell sets (`apps/desktop/src-tauri/src/lib.rs:415`) | A headless host must set it itself or the phone gets no app | Ship the built PWA in the tarball (`npm run build:remote`) and set the variable in the unit | S |
| WSL | `retinue-wsl` is a shell feature, not a daemon dependency (`crates/retinue-daemon/Cargo.toml:10-56`) | Nothing to do | Leave it out of the Linux build | — |

**What is not known, and cannot be known from reading:** whether `cargo check -p retinue-daemon --target x86_64-unknown-linux-gnu` passes today. The stubs and Unix branches suggest somebody thought about it (`sandbox.rs`, `retinue-process`, `retinue-runtime`, `outside.rs`), but CI has never tried, and there are two specific risks in the code: `secrets.rs` puts only its `wincred` backend behind `cfg(all(windows, not(test)))` (`secrets.rs:155`) while its callers are unconditional, and `retinue-daemon` pulls in `rusqlite` with `bundled` and `rustls` with `ring`, both of which need a C toolchain for the target.

**The cheapest possible check, and it should be step 0:** add a Linux job to CI running `cargo check -p retinue-daemon -p retinue-process -p retinue-runtime` and `cargo test -p retinue-process`. It answers the question in one commit, and it is the difference between a three-week job and a six-week one. Two build-environment facts the runner must have before the result means anything: `rusqlite` is used with `bundled` (`Cargo.toml:45`), so a C compiler is required, and `rustls` is used with `ring` (`Cargo.toml:47`), which also builds C and assembly — a failure without `build-essential` (or `clang`) says nothing about the code. Attempting a cross-`check` from the Windows checkout was not done here: there is no Linux target or C toolchain installed, so a failure would say nothing about the code either.

---

# 4. How it is installed, updated and kept running

Modelled on T3's service, adapted to a Rust binary.

**One recommendation: a systemd *user* unit, with the option of a system unit for a server with no logged-in user.**

```ini
# ~/.config/systemd/user/retinued.service
[Unit]
Description=Retinue daemon
StartLimitIntervalSec=300
StartLimitBurst=5

[Service]
Type=simple
Environment=RETINUE_DB_PATH=%h/.local/share/retinue/retinue.db
Environment=RETINUE_REMOTE_WEB_DIR=%h/.local/share/retinue/remote-web
ExecStart=%h/.local/bin/retinued
Restart=always
RestartSec=5
KillMode=mixed
StandardOutput=append:%h/.local/share/retinue/retinued.log
StandardError=append:%h/.local/share/retinue/retinued.log

[Install]
WantedBy=default.target
```

Why a *user* unit: it needs no root, it matches T3's shape, and the daemon's model is one owner's data. Why a system unit may be needed instead: a VPS where the owner never logs in has no user manager at boot unless `loginctl enable-linger <user>` is set; the install notes must say so, and that is the one line most likely to be missed.

**Install:**

1. A tarball per release: `retinued`, `retinue-hook`, the built phone PWA, the unit file, a checksum list. No signing is required on Linux, which removes the SignPath step the Windows release needs (`release.yml:110-141`).
2. `retinued --version` and `retinued doctor` (the daemon already has a doctor, `main.rs:89`, `doctor.rs`) as the post-install check.
3. The agent CLIs installed and signed in as the service user, in the service's `PATH` — the same requirement T3 states (`docs/user/remote-access.md:252-257`).

**Update:** replace the binary, `systemctl --user daemon-reload` if the unit changed, `systemctl --user restart retinued`. A running turn dies on restart, exactly as T3 warns ("Restarting interrupts running agent turns… so it asks first", `docs/user/background-service.md:25-32`); the daemon already recovers restart state (`main.rs:6269-6274`), so the honest wording is "it resumes, but a turn in flight is lost". Do not build an in-app updater for Linux in the first slice: a tarball plus a restart is enough, and it is what the release job can produce with certainty.

**Kept running:** `Restart=always` plus `RestartSec=5`; the log file; a `retinued doctor` line for "the data folder is not owner-only" (the check exists: `sandbox.rs:119-123`). Consider one T3 idea and not the rest: `OOMScoreAdjust` + `MemoryLow` matter when agents share the host with the daemon; the weights do not, because Retinue's agents are children of the daemon, not siblings.

---

# 5. How the owner pairs it

Two paths, because the certificate decides which one the linking machine offers (`machines.rs:596-621`).

- **Tailscale, web PKI → a code.** The laptop probes `https://<vps>.tailnet.ts.net`, the certificate is trusted, so it posts to `/api/pair` with a code (`machines.rs:722-727`). The code comes from `remote.pair.begin`, which today only the host's window can call. On the VPS: `retinued pair` prints the code and the origin, waits, and prints "approved" or "expired"; the code is never written to a log or the database, and the same rule as the device token applies (user-only file, read once).
- **Native, pinned certificate → the password.** The laptop shows the fingerprint, the owner compares it with `retinued status` on the VPS, and the laptop signs in with the host's username and password (`machines.rs:728-760`). Then the host's owner must approve the pending request with a tier: `retinued pair confirm --tier chat`. The pending request already carries name, kind, platform and login (`remote/pairing.rs:281-295`); printing it needs no new storage.

**Fix on the way:** the request says `"platform": "windows"` for every host (`machines.rs:726`, `:738`). On the VPS that sentence would be a lie in the owner's own approval dialog, and the field is what the console printout and the laptop's list would show.

**The free check worth using before any of this:** once a Linux daemon runs with Remote on, the laptop's browser can open `https://<vps>.tailnet.ts.net` and drive that machine today, because the gateway already serves the PWA (`remote/web.rs`, `StaticFiles::from_env`). That proves the daemon end to end before the machine-link half is finished.

---

# 6. Effort

One backend lane, in the order I would run it.

| Step | Work | Days |
|---|---|---|
| 0 | Linux CI job: `cargo check`/`cargo test` for the daemon and its two platform-shaped crates. Fix whatever it finds in `owner_computer`, `secrets` call sites and `rusqlite`. **This is the gate on the estimate.** | 1–2, or up to 8 if the compile surface is bad |
| 1 | XDG data folder + `--version`/`doctor` smoke test | 1 |
| 2 | Tailscale by `PATH`; Native firewall refusal; `OS` instead of `"windows"` | 1 |
| 3 | Console pairing (`pair`, `pair confirm --tier`) with the code never logged, and tests that prove it | 2–3 |
| 4 | **Secrets decision and store** (owner's choice, section 7 Q4) | 3–5 |
| 5 | Tarball release job + unit file + install/update notes + the lingering line | 2–3 |
| 6 | The small refusals and equivalents: owner-computer, preview ports, reveal-in-folder, `sh` sign-in copy, remote PWA path in the unit | 2–3 |
| 7 | Docs: `docs/operations/` install page, threat-model row for a Linux host, skill note | 1 |

**Total: about two to three weeks of one lane, of which the first two days decide whether it is three weeks or six.** The single largest uncertainty is step 0. The single largest owner decision is step 4.

**What this estimate excludes:** a Linux *desktop* app (no Tauri build for Linux, no tray, no dictation), a Linux installer package (`.deb`/AUR/`curl | sh`), and any signing or store presence. A headless daemon in a tarball is the smallest thing that makes the VPS a place where conversations run.

---

# 7. Container or beside it — recommendation

**Beside it.** Run `retinued` on the VPS host under systemd; keep Harbor's container exactly as it is. Reasons:

1. The container sleeps (`computer.rs:3-6`); a daemon's conversations must not.
2. A conversation's files, worktrees and git history must survive the container's lifecycle, which belongs to Harbor.
3. Remote access needs a stable HTTPS origin on the tailnet; the host has Tailscale, the container is behind Harbor's proxying.
4. The two are complementary, not competing: the daemon is *where work runs*, the container is *the employee's desktop and browser* (`browser.rs:1-20`).
5. Robustness: a daemon inside a container adds a second thing that can sleep, restart or be recreated to every failure investigation.

The one thing to avoid is the opposite mistake: putting the *browser* or the *computer* tools on the host, where there is no desktop session. They belong in the container, as they are.

---

# 8. The Director's findings, checked

| Claim in the addition | Verdict | Evidence |
|---|---|---|
| Machine links are daemon-to-daemon | **Confirmed** | `machines.rs:1-11`; the window never talks to the other machine |
| The window shows other machines in a separate sidebar group | **Confirmed** | `apps/desktop/src/ui/MachineBots.tsx:31-44` (`MachineGroups`) |
| A remote conversation opens in a separate panel that polls every 3 s | **Confirmed** | `MachineConversation.tsx:19` (`POLL_MS = 3000`), `:220-231` |
| It lists conversations every 15 s | **Confirmed** | `MachineConversation.tsx:216` |
| The daemon refreshes links every 20 s | **Confirmed** | `machines.rs:54` (`interval: 20 s`), loop at `:412-432` |
| Nothing streams live | **Confirmed**, with one addition: the link has no socket at all (`machine_client.rs:286-430`), and `tool.changed` is `drop` for devices (`tiers.json:2023`), so even a socket would not deliver steps by itself | as cited |
| `machine.start`, `machine.prompt`, `machine.steer`, `machine.cancel`, `machine.permission.reply` exist (chat or operator) | **Confirmed**, and two qualifications: they landed on 13 November (`ef0244b`, `375a40a`), and `machine.permission.reply` has **no UI caller**, so the transcript still says "Answer it on <machine>" (`MachineConversation.tsx:278`) | `machines.rs:922-950`; `tiers.json:593-639`, `:712-715` |
| `machine.takeOver` hands a conversation over | **Confirmed**, and it is a transfer, not a "continue here": it needs `operator`, a copy or a remote, and it commits and pushes the conversation's own `retinue/` branch | `handoff.rs:1-20`; `MachineConversation.tsx:39-121` |
| Every `machine.*` is `never` remote, so the phone sees only its host | **Confirmed** | `tiers.json:1769-1815`; the phone client is bound to one origin (`remoteTransport.ts:244-245`) |
| No remote folder picker or remote clone | **Confirmed**, and stronger than "not built": the far gateway refuses `project.create`, `project.clone` and `project.discover` for **every** device | `tiers.json:824-827`, `:1321-1345`; no caller for `machine.projects` (`machines.rs:950`) in `apps/desktop/src` |
| T3's model is one server per machine, every client connected to all of them | **Confirmed**; worth adding how far it goes: routes are an ordered list per machine (`docs/internals/remote.md:32-56`), and "Balance new threads across machines" is off by default (`docs/user/remote-access.md:178-194`) | as cited |

**One correction that matters for the plan.** The addition says the phone "sees only the one host it paired with"; that is true, but the client cannot hold a catalogue at all, because it is served *by* the host and reads `location.host`. So "let the phone pair with each machine" would mean three separate installed apps, not one app with three machines. That is why `03-design-and-order.md` recommends the host-as-aggregator shape (option B) rather than T3's client catalogue.

**One addition the Director did not list, which changes effort on R2:** the laptop's view of another machine's work is redacted to phone strength. Tool rows arrive with the title `Tool` and no detail (`remote/policy.rs:428-431`, `:469-477`), and the transcript filters tool rows out entirely (`MachineConversation.tsx:239`). Matching T3's *feel* on the laptop therefore needs an owner decision about content (open question 9 in `03`), not just a socket.
