# Setting a headless host up from its own terminal

Date 20 November 2026 (work of 19–20 November 2026). Branch `retinue/headless-console-commands-from-the-direc`. Lane: backend.

## Why

Pairing alone did not make a Linux host usable. Two things were missing:

- **Projects from other machines** is two settings, `projects.devices.enabled` and `projects.devices.folder`, and before this change the only way to write them was the desktop's Settings page — a page a headless host does not have. Worse, the refusal `project.browse` and `project.createFromFolder` gave said "Turn them on in Settings › Machines", which sends the owner to a window that does not exist on that machine.
- **A bot could not be created at all.** `agent.create` is a window RPC; on a headless host there was no way to hire the bot the phone or laptop would then talk to.

The contract is unchanged in every other way: the desktop RPCs, their events and their sentences are still the ones a window uses.

## What the console answers now

All of these are in `console.rs`, routed by `console::route` from `main_rpc`, before the capability check, and open only with the token in `<data>/console.json`:

| Method | Params | Reply |
|---|---|---|
| `console.settings.get` | `{}` or `{key}` | `{"settings": {…}, "allowlist": ["projects.devices.enabled", "projects.devices.folder"]}` — read of one allowlisted key or of both |
| `console.settings.set` | `{key, value}` | `{"saved": true, "key", "value"}` — the stored value, which for the folder is its canonical path |
| `console.agents` | `{}` | `{"agents": [{provider, name, installed, version, status, enabled, authState, signedIn, signInCommand, runtimeIds}]}` |
| `console.bot.list` | `{}` | `{"bots": [{id, name, runtimeId, model, thinking, approvalMode, projectId, provider}]}` |
| `console.bot.create` | `{name, agent, model?, effort?, project?}` | `{"agent": {…the same summary as one row of `console.bot.list`}}` |

The CLI beside them:

```sh
retinued settings get [key]
retinued settings set <key> <value>      # true/false become JSON booleans; the rest is text
retinued agents
retinued bot add --name N --agent A [--model M] [--effort E] [--project F]
retinued bot list
```

`retinued settings …`, `retinued agents` and `retinued bot …` were added to `daemon_main`'s argument match as well as to `run_cli`; without that line the command falls through to the daemon-start path (the first draft of this change had exactly that bug — see the skill's lessons entry of 2026-11-20, and `tests/console_cli.rs`, which runs every verb and fails if a daemon starts).

## The rules it follows

- **The allowlist is the whole reach.** A key outside `SETTINGS` is refused on read and on write, with a sentence naming both allowed keys, so the console cannot read a value it holds (a preference, a credential, a secret) or write one.
- **The folder is checked on the disk.** It must exist and be a folder on this host, and the stored value is its canonical path in the form `project_share::display` keeps (`\\?\` stripped, so git and the project row agree). An empty value clears it, as the desktop allows.
- **The value is written through `settings.set`**, the desktop's own arm, so validation, the `settings.changed` event and the stored shape cannot drift. One content-free activity line per change (kind `console`, no path, no value).
- **A bot is made by `agent.create`**, so the name rule, the catalog check, the events, the team placement and `auto_hire` are the desktop's. Two deliberate differences, both about a terminal: the coding agent must be one the discovery reports installed here (`runtime_for`: exact runtime id, else a running copy of that provider, else any installed copy), and the bot always gets `approvalMode: "ask"` — the window's new-bot default is Full access, and there is no card here to approve it with.
- **`--project` registers the folder** through `project.create` (the desktop's New project path, repository identity included) when no row matches its canonical path, then places the bot through `agent.create`'s own `projectId` handling.
- **No credential, executable path or launch argument** is in any reply. `console.agents` is `provider_gate::list`'s rows, filtered to the fields a terminal needs.

## What changes for existing callers

- `console.agents`'s `signInCommand` is now a command for `claude`, `codex` and `opencode` (`claude`, `codex login`, `opencode auth login`), where the discovery's own table answered `null`. The words match `apps/desktop/src/ui/providerGuides.ts` and `sign_in_hint` in `main.rs`; no desktop RPC changed.
- `project.browse` / `project.createFromFolder` refuse with a console command **only on a headless host** (`st.console.headless_host()`, recorded once at startup). A desktop's "Settings › Machines" sentences are unchanged.
- Everything else is additive: new methods, new CLI verbs, one new field on `Console` (`headless`), and `pub(crate)` sentence helpers in `project_share.rs`.

## Verification

Run in this worktree against the warm target of the main checkout (`CARGO_TARGET_DIR`, `TEMP`/`TMP=C:\tmp`, `RUST_MIN_STACK=33554432`):

- `cargo check -p retinue-daemon --all-targets` — clean, no warnings.
- `cargo test -p retinue-daemon -- setting_the_host_up_needs_the_console_token the_console_changes_only_the_two_settings a_bot_created_from_the_console agents_lists_what_is_installed a_host_with_no_window_is_told` — **5 passed, 0 failed**. The first run had one failure (`agents` expected a sign-in command for Codex and the discovery reports none); the code now fills the three main providers' commands and the test pins `codex login`.
- `cargo test -p retinue-daemon --test console_cli` — **1 passed**: every verb (`settings get`, `settings set`, `agents`, `bot list`, `status`) exits 3 with "No headless daemon is running here." and never starts a daemon. The first link was refused by Smart App Control (`os error 4551`, "never executed"); deleting the test `.exe` and relinking was allowed, as the verification notes describe. The same five commands were also run against `target\debug\retinued.exe` by hand and gave exit 3 and that sentence.
- `node scripts/check-format.mjs` — passed (1,508 files).
- `node scripts/check-architecture.mjs` — passed.
- `node scripts/docs-library.mjs build` then `check` — 445 documents, all hashes and snapshots match.
- `cargo clippy -p retinue-daemon --all-targets -- -D warnings` — clean; no warning in the log, 2m44s on a warm target.

**Not proved here:** a Linux build (CI's `linux` job is the proof; nothing on this PC can cross-check), and a real headless host end to end (systemd unit, `retinued settings` against a running daemon). Smart App Control on this PC refused the newly linked `console_cli` test binary once (`os error 4551`, "never executed"); deleting it and relinking was allowed and the test then ran.

## Open, for the Director

- `console.bot.create` goes through `agent.create`, which asks the catalog for a **fresh** read when none is cached. On a host with a slow CLI that is a provider probe inside the create, bounded only by the console client's 30-second timeout. The desktop's hire does the same; if it is too slow on the VPS, the fix belongs in `catalog::get`'s call, not here.
- The console still cannot start a conversation or send a message: the laptop or phone does that over the gateway once the bot exists.
- Extending the console's reach means editing `SETTINGS` in `console.rs` deliberately; nothing reads the allowlist from a file.
- Skill updated: `references/codebase-map.md` (the console's new methods and CLI, and the `daemon_main` match), `references/lessons.md` (2026-11-20, the match trap), and `docs/reference/invariants.md` row 20.
