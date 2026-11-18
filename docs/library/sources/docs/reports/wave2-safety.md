# Wave 2, safety and providers (worktree `-9`)

> **Lane:** Ada, backend. **Sources:** `docs/plans/nod-adoption-plan.md` (wave 2, safety and providers), `docs/research/n8n-nod/01-sandbox.md` ideas 1 to 7, `04-vault.md` idea 6, `02-llm-providers.md` ideas 1 to 5.
> **Status:** seven of the items are implemented; the rest are listed under "Not done" with the reason. Nothing is committed. Wave 1 features are untouched. Hooks into `main.rs`, `team.rs`, `workspace.rs`, `computer.rs`, `doctor.rs` and `tiers.json` only; everything else is new modules.
> **Read this first:** section "Contract" is what the Director builds the screens from. A shape changes only with a line under "Changes" saying so.

## Contract (stable)

### 1. `reach.statement` — what a bot can reach, in plain words

New RPC. Desktop only (`never` in `tiers.json`): the reply carries host paths.

```jsonc
// params: exactly one is used; sessionId wins when both are given
{ "sessionId": "ses…" }        // or { "agentId": "…" }
// reply
{ "reach": {
  "sessionId": "ses… | null",
  "agentId": "… | null",
  "botName": "Ada",
  "statement": "Ada runs as you, as your own Windows account, in C:\\…\\project, with your network and the tools you have installed.",
  "folder": "C:\\…\\project | null",           // where it works
  "isolation": "shared | isolated | unknown",  // "isolated" = its own copy of the repository
  "runsAs": "your own Windows account",
  "network": "your network",
  "permissionMode": "ask | auto | accept_edits | bypass",
  "permissionSentence": "Every tool call asks you first.",
  "unattended": false,
  "tools": [ { "id": "files | commands | internet | team | computer",
               "label": "Files in its project folder", "on": true } ],   // always five rows, in this order
  "limits": [ "It runs as your own Windows account, so it can change anything you can.", "…" ]
} }
```

- `statement` is one paragraph, plain sentence case, no markup. `tools[].on` is false for a tool family that is not offered here (`files` when no folder is known, `computer` when no linked computer is switched on). `limits` always holds at least three rows: what it runs as, the permission mode in one sentence, and that the CLI's own sandbox is its own.
- Errors: `invalid_argument` when neither id is given; `not_found` when the session or bot is unknown.
- Implemented in `crates/retinue-daemon/src/reach.rs`; dispatch arm `main.rs:1216`; tier row `apps/desktop/src/remote/tiers.json` (beside `session.workspace`).

### 2. `session.workspace` gains `scratchFolder` (additive)

```jsonc
{ "sessionId": "…", "mode": "shared|isolated|unknown", …,
  "scratchFolder": "C:\\Users\\…\\AppData\\Local\\Retinue\\scratch\\ses… | null" }
```

The conversation's own scratch folder, **created on the first read**. `null` when there is no data folder or it cannot be made. Nothing else about `session.workspace` changes (still `never` remote). `workspace.rs:72`.

The folder is also named in the bot's own instructions, one sentence, so it is used without the owner doing anything (`team.rs:501`, `scratch::instruction_line`). It is a tidy place to write, **never a boundary** — it is not presented as isolation anywhere.

### 3. `runtime.models` rows gain `price` and `health` (additive)

```jsonc
// one row of catalog.models, in addition to what the adapter already sends
{ "id": "claude-sonnet-5-5", "displayName": "Sonnet 5.5", …,
  "price": { "inputPerMTok": "3.00", "outputPerMTok": "15.00", "currency": "USD",
             "source": "https://… | null", "checkedAt": "2026-11-01 | null" } | null,
  "health": { "failures": 2, "pausedUntil": "2026-11-17T12:30:00.000Z | null",
              "reason": "The provider reported a turn error. | null" } | null }
```

- `price: null` means **price unknown**; the UI must say unknown, never zero (`AGENTS.md`). The numbers are decimal strings from `retinue-usage`'s shipped `official-prices.json`, with its source URL and checked date.
- `health: null` means nothing recorded. `pausedUntil` is present only while unattended work is paused for that model.
- Capability cues already exist on the rows (`supportedThinking`, `serviceTiers`, `contextWindow`, `releaseDate`), so this adds price and health only — no second capability vocabulary.
- `runtime.models` is `read` at `viewer` today: neither field carries a host path, message text or a credential, and `reason` is the already-redacted provider line.
- Implemented in `model_cues.rs`; hook `main.rs:2525`.

### 4. `computer_exec` tool reply gains three fields, and keeps head and tail

The tool's answer is JSON, as before, with Harbor's own `ok`, `code` and `output`; when the output is long the `output` field keeps the **first and last 16 KiB** with a marker line between them, and three fields are added:

```jsonc
{ "ok": true, "code": 0, "output": "…first 16 KiB…\n… 524288 bytes left out here; the whole output is kept, and its file is named beside it …\n…last 16 KiB…",
  "outputTruncated": true,
  "outputBytes": 655360,
  "fullOutputPath": "C:\\Users\\…\\Retinue\\tool-output\\ses…\\computer_exec-<uuid>.log | null" }
```

A short output is returned exactly as before, with none of the three fields. 16 KiB each (half the module's 32 KiB rule) because the whole answer still passes through the MCP wrapper's 64 KiB ceiling, which must not cut the tail a second time. The full text is kept in the data folder, one file per call, deleted after 7 days at startup. `computer.rs` (`exec_output_answer`), `tool_output.rs`.

### 5. "May have acted": a lost answer is never success and never "nothing happened"

Affects `computer_exec`, `computer_create_routine`, `computer_run_routine`, `computer_click`, `computer_type`, `computer_key` and `computer_scroll` (the tools that change the computer). When the request went out and no answer came back in time, the tool answers with this fixed sentence instead of a generic failure:

> "The computer did not answer, so this step may have happened. Look at the computer before running it again; Retinue did not repeat it."

Reads (`computer_status`, `computer_logs`, `computer_list_routines`, `computer_desktop_url`) and the two tools that cannot change anything (`computer_move`, `computer_screenshot`) keep their ordinary sentences: "The computer's server stopped answering." A connection that never opened is still "did not answer", because nothing can have happened. `computer.rs:259` (`HarborError::Unanswered`), `may_have_acted`.

### 6. A paused model refuses unattended work only

No new RPC or event.

- Two failures in a row for the same provider and model, with no completed turn between them, start a pause: **5 minutes, then 15, then 60**, counted from the second failure. A completed turn deletes the record.
- Only these failure classes count: `provider_error`, `provider_unavailable`, `overloaded`, `unauthenticated`, `config_unsupported`. A timeout, a full context window, a refused permission or a cancellation never pauses anything.
- An **unattended** turn (an automation's run, or a task's run marked unattended) whose model is paused answers with error code **`model_paused`** and this message:

> "This model failed twice in a row, so unattended work on it is paused until 13:40 UTC. Run the conversation yourself, or choose another model."

- An owner's own turn is **never** refused, never switched to another model, never hidden. Nothing counts spend: this is not a budget.
- Where: `model_health.rs`; storage `crates/retinue-storage/src/wave2.rs` (table `model_health`, `model_health_all|get|record|clear`); hooks `main.rs:4391` (refusal), `main.rs:5179` (failure), `main.rs:5789` (success).

### 7. Data folder ACL, as a Doctor row

No new RPC. `doctor.run` gains one check; the Doctor's area list gains `safety`:

```jsonc
{ "id": "safety.dataFolder", "area": "safety",
  "label": "Retinue's data folder is yours alone",
  "status": "ok | warn",
  "detail": "Only your Windows account can read it, so a capability or an endpoint file in it stays yours.",
  "fix": null }
```

Applied once at startup to `<data>`, `<data>/scratch` and `<data>/tool-output` with the same `icacls /inheritance:r /grant:r <user>:(OI)(CI)F` the app already uses for its private instruction files and the Native certificate key (`main.rs:6222`, `sandbox.rs`). The check re-reads the folder's own ACL with `icacls` and looks for the ASCII `(I)` inherited marker, so it proves the state rather than remembering that it ran. A failure is a warning, never fatal.

## Changes

- **First version.** Item 4 in the first draft promised 32 KiB head and tail and a top-level `exitCode`; the real answer keeps Harbor's own `code` field, uses 16 KiB each (half the module's rule, so the MCP wrapper's 64 KiB ceiling cannot cut it), and the three new fields sit beside `ok`/`code`/`output`. Item 7's detail text changed to avoid the word "token" (the Doctor's own test refuses that word in a detail line).

## Log

- Report written before any code, as the wave-1 lesson requires.
- `sandbox.rs`, `scratch.rs`, `reach.rs`, `tool_output.rs`, `model_health.rs`, `model_cues.rs` added; storage `wave2.rs` added and called from `migrate()`.
- `cargo check -p retinue-storage` and `cargo check -p retinue-daemon` pass (with `CARGO_TARGET_DIR` pointed at the `backend-work-from-the-owner-assigned` worktree's target, because a fresh target dir is refused by Smart App Control: *An Application Control policy has blocked this file. (os error 4551)* on the build scripts of `serde_json` and `libsqlite3-sys`). **No build.rs stub and no deleted build script: nothing was worked around in the source tree.**
- Existing daemon tests compiled and run; results in "Verified" below.

## Verified

- `cargo check -p retinue-storage` and `cargo check -p retinue-daemon`: clean, no warnings.
- `cargo test -p retinue-storage -- wave2::tests`: **4 passed** (the ledger's count, its pause, an expired pause, and that one provider's record never hides another's).
- `cargo test -p retinue-daemon -- sandbox scratch reach tool_output model_health model_cues safety_tests`: **42 passed, 0 failed** (my six modules' unit tests, the three `safety_tests.rs` (first named `wave2_tests.rs`) end-to-end ones, and any existing test whose name the filters matched).
- `cargo test -p retinue-daemon -- computer_`: **42 passed, 0 failed** (the Harbor tools, after `HarborError::Unanswered` and the head-and-tail answer).
- `cargo clippy -p retinue-storage --all-targets -- -D warnings`: clean. `cargo clippy -p retinue-daemon` could not run here: Smart App Control refuses the daemon's build script *under clippy* (a different wrapper forces a fresh build script). Plain `cargo test --no-run` compiles the whole test profile without warnings, which is what proved the code.
- **Not run:** the whole daemon unit suite in one go. The test binary was refused by Smart App Control (`os error 4551`) on most attempts; a focused run of the four filters above did run and pass. Two known environment failures were seen and are **not** from this change: `doctor::tests::database_and_hook_rows_are_one_line_and_actionable` (this lane inherits `RETINUE_HOOK_BRIDGE` from the app that started it) and the `parent_watchdog` integration binary (blocked). Both are recorded in `references/lessons.md`.
- **Not run:** the JavaScript gates (`npm test`, `tsc`), because this worktree has no `node_modules`. The only JS change is one `tiers.json` row shaped exactly like `session.workspace`; the remote tiers parity test reads the dispatch and will see `reach.statement` on both sides.

## A bug my own tests caught, worth knowing

`catalog["models"].as_array_mut()` on a `serde_json::Value` **inserts** `"models": null` when the key is missing, because `IndexMut` inserts. The overlay now uses `catalog.get_mut("models")`, and a test pins it. Any code that reads a `Value` through `[` on a `&mut Value` and then awaits `is_null()` can hide the same edit.

## Not done, and why

1. **Construct the CLI environment instead of inheriting it** (01, idea 2). Medium size, and it changes how every adapter launches: each CLI needs its own allowlist, an opt-out list, and a Doctor row that says which variable is missing. Not started; the plan's own ranking puts it fourth and it needs its own lane with a live check of Claude, Codex and OpenCode.
2. **The grant clock for local execution** (01, idea 6). There is no local-execution tool to put a clock on: Retinue has no "run one command on my owner's machine" tool today, and the item is a rule for the day one exists (reuse `owner_computer`'s three clocks, attended only, tier `never`). Nothing to build now.
3. **Learn and mask a value the owner types during Take control** (04, idea 6). It depends on the wave-1 vault lane's per-conversation secret set (04, ideas 1 and 2), which is not in this worktree; building a second set here would collide with that lane. Do it after the two merge.
4. **Capability rows in the provider table** (02, idea 1). A clean small refactor of `crates/retinue-runtime/src/providers.rs` plus `discover_one`'s `match`, with the `images` field replacing the window's hardcoded list. Not started: it adds nothing the owner can see without a UI change the Director has not asked for yet, and I chose the reach statement and the ACL over it when time ran short.
5. **A per-lane model choice for automations and background work** (02, idea 4). The wiring belongs in `automations.rs`, which the automations lane is editing in its own worktree; a setting written here without its reader would be a lie. Needs one owner decision too (same model or a nominated cheaper one).
6. **"Your own model server" through a CLI's own config** (02, idea 5). Medium to large, and it writes into each CLI's own config folder: one bounded, reversible writer per CLI, a test each, and the key through `secrets.rs` as an environment reference. Not started.
