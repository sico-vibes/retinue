# Multi-device, step 1: a live link, the machine's conversations, "started from"

> **Lane:** Ada, backend, worktree `backend-task-from-the-director-claude-15`, `main` at `b7c3a32`. Nothing is committed.
> **Sources:** `docs/research/multi-device/03-design-and-order.md` section 5, step 1 (pieces 1, 2, 3), pieces 4 dropped (UI only); `docs/research/many-machines-2026-11.md`; `docs/remote/THREAT_MODEL.md` section 16.
> **Read this first:** the Contract below is what the Director builds the screens from. A reply shape changes only with a line under **Changes**, dated.
> **No owner rule is amended by this lane.** Nothing here needs amendments A1 to A4: `machine.*` stays `never` remote, `machine.session.changed` is `drop`, and every new value is content-free.

## Contract (stable)

Every method name and every key below is what the daemon answers today. `…` is an opaque id. Line numbers are in `crates/retinue-daemon/src` at the time of writing and will drift; the module names will not.

### 1. `machine.stream.status` — is this link live?

New RPC, desktop only (`machine.*` is `never` in `tiers.json`, so nothing here is reachable from a paired device). Answers from memory; no outbound call.

```jsonc
// params
{ "machineId": "…" }
// reply
{ "connected": true, "lastSequence": 1234 }
```

- `connected` is true while a socket to that machine's `/api/events` is open and has passed its liveness probe.
- `lastSequence` is the far machine's own event sequence the cursor has reached, `0` before the first event. Memory only: it is `0` again after a daemon restart, and the next connect replays from there.
- Errors: `invalid_argument` when `machineId` is missing or empty; `not_found` when that machine is not linked.
- Implemented in `machine_stream.rs`; dispatch arm in `main.rs` beside the other `machine.*` methods.

### 2. `machine.sessions` — the other machines' conversations, from memory

New RPC, desktop only. Returns the rows the 20-second loop already holds. **No new outbound call**: a row appears after the next read of that machine (at most 20 s, or immediately for a machine just linked, because a pairing wakes the loop).

```jsonc
// params: both shapes are valid
{}                                   // every linked machine
{ "machineId": "…" }                 // one machine; not_found when it is not linked
// reply
{ "sessions": [ {
  "machineId": "…",
  "machineName": "Home PC",
  "kind": "windows",                 // "windows" | "linux" | "macos" | null (an older far daemon)
  "id": "…",                         // the far machine's session id
  "title": "Invoice parser",
  "agentId": "…",
  "agentName": "Ada",                // null when the bot is gone or hidden on that machine
  "projectName": "Retinue",          // null when the conversation has no project
  "state": "working",                // the far machine's own session state word
  "updatedAt": "2026-11-17T09:14:02.114Z",
  "archived": false,
  "startedFrom": { "deviceId": "…", "deviceKind": "machine", "machineName": "Laptop" }  // 18 Nov: only when a paired device started it
} ] }
```

- One machine's rows keep the order its own `session.list` gave; machines come in `machine.list` order.
- `archived` is the far row's own flag. It is `false` in practice today, because the 20-second read asks `session.list` without `includeArchived`; the field exists so the UI does not have to change when that read widens.
- An offline machine keeps its last known rows (nothing is cleared), so the UI can grey them out; `machine.list`'s `state` says whether they are current.
- `startedFrom` (added 18 November 2026) is the far `session.list` row's own `link.startedFrom`, copied only when it is an object: three keys, `deviceId`, `deviceKind` and `machineName`, and nothing else. A conversation started at that machine's own window, by an automation or by a handoff has no field at all.
- Errors: `not_found` for an unknown `machineId`; no error for an offline machine.

### 3. The link row gains two fields (`machine.list`, `machine.info` of a link, `machine.pair.*` job rows)

```jsonc
{
  "id": "…", "name": "Home PC", "url": "https://…", "auth": "password",
  "fingerprint": "AB:CD:…", "tier": "operator", "pairedAt": "…", "lastSeenAt": "…",
  "state": "online", "detail": null, "version": "0.1.0-beta.60",
  "bots": [ … ],
  "kind": "windows",     // new: the far machine's OS family, null before the first read or on an older far daemon
  "live": true           // new: a socket to that machine is open right now
}
```

`kind` is the value the far machine reports in `machine.info`; the UI maps it to a laptop, desktop or server icon. `live` is the same value `machine.stream.status` gives.

### 4. `machine.info` gains `platform`

```jsonc
// reply
{ "machineId": "…", "name": "Home PC", "version": "0.1.0-beta.60",
  "handoffVersion": 1,
  "platform": "windows" }     // new: std::env::consts::OS — "windows", "linux", "macos", or another target's word
```

This is the machine-readable OS family, and it is what the link row's `kind` copies. An older far daemon omits it and `kind` stays null.

### 5. Event `machine.session.changed` — local, content-free

```jsonc
{ "machineId": "…", "sessionId": "…", "revision": 42 }
```

- Emitted on this machine (not stored from the far machine, not sent to any device: `drop` in `tiers.json`) when a **relevant** event arrives on a link's socket: `session.changed`, `session.deleted`, `message.completed`, `turn.completed`, `turn.cancelled`, `turn.error`, `permission.requested`, `permission.resolved`.
- `revision` is the far event's `sequence`, so a later event for the same conversation always has a higher number. The UI may ignore a `revision` at or below the one it holds.
- No title, no text, no tool name, no path: the window re-reads `machine.conversation` for a conversation it is showing (the far gateway redacts that as it does for a phone) and `machine.sessions` for the list.
- Not emitted at all when the conversation on this machine is recorded as moved to a **different** machine, or a move lease is live (`refuse_if_moved_elsewhere`): the machine that gave a conversation away does not report it as live.
- `message.delta` is deliberately not forwarded: a half-typed message is content, not a change to a row, and the open conversation is re-read on its own timer.

### 6. Event `machine.changed` gains `live`

```jsonc
{ "machineId": "…", "state": "online", "live": true }
```

Fires when a socket opens or closes, and (as today) from the 20-second read when `state`, `detail`, `version`, `kind`, `bots` or `tier` changed. Still ids and words only.

- `live` is the current value whenever the event fires. A connect or disconnect always fires it once; a 20-second read never fires because of `live` alone, so a flaky link cannot make the poll chatty.

### 7. Session rows gain `link.startedFrom`

A conversation created **through a gateway** carries, in its own `link` object:

```jsonc
"link": { "startedFrom": { "deviceId": "…", "deviceKind": "machine", "machineName": "Laptop" } }
```

- Written by the gateway on the machine that runs the conversation, when `agent.conversation.create` succeeded for a paired device. `machine.start` is exactly that call.
- `deviceId` is that machine's own device row id for the caller (an opaque id, not a token); `deviceKind` is `machine`, `phone`, `tablet`, `browser` or `computer`; `machineName` is the name the caller paired with.
- Present on every row that already carries `link`: `session.list`, `session.get`, `app.snapshot`, `agent.conversations`, and so `machine.conversations` / `machine.conversation` on the machine that linked. Redaction drops `link.worktree` for devices as it does today and keeps `startedFrom`, exactly as it keeps `movedTo`.
- A conversation the owner started at that machine has no `startedFrom` at all. It is **not** added by a later `session.prompt` from another machine: the value says where the conversation was created, and stamping an owner's own conversation because a phone sent it a message would be a lie.

### 8. The pairing request reports the real OS

`machine.pair.begin` sends `"platform": "<label>"` where the label was hard-coded to `"windows"`. It is now `Windows`, `Linux`, `macOS` (or the target's own word, capitalised) — the text a person reads in the approval dialog. No shape changes; a Linux daemon no longer claims to be Windows.

### 9. The work log: a `machine` device reads it, a phone does not (18 November 2026)

Owner decision, 17 November 2026, `AGENTS.md` "Retinue on many machines": *"A device of kind `machine` may receive tool titles, commands and output; phones and browsers keep today's redaction."* Done in `remote/policy.rs` and its TypeScript mirror `apps/desktop/src/remote/remoteTiers.ts`, through one new option that the gateway sets from the calling device's kind.

```jsonc
// RedactOptions, both languages (camelCase in JSON, snake_case in Rust)
{ "includeToolOutput": false,        // unchanged: the desktop's own debug switch
  "deviceKind": "machine" }          // new: "machine" | "phone" | "tablet" | "browser" | "computer"
```

- `deviceKind: "machine"` keeps `tools[].title` and `tools[].detail` (the command and the output) in `session.get`, `session.list`, `app.snapshot`, `agent.*` and every other `read` result, and in every event payload the gateway sends — the same walk, so nothing else changes.
- Everything else is dropped for a machine exactly as for a phone: thinking (never present in a tool row), environment values, `envRefs`, the settings allowlist, the sensitive-key rule, `providerSessionId`, `overrides`, `worktree`, and every `owner_*`/`computer_*` value.
- A value inside a tool's `detail` that is *itself* a host path is cut to its last two segments, the rule a permission card already follows. A path inside a command's text is left as the command ran, because the log must say what actually ran. `includeToolOutput` keeps its old behaviour byte for byte (no reduction), so the existing goldens still pin it.
- `redact_event(event, options)` and `redactEventForRemote(event, options)` take the options now; the gateway builds them once per socket from `DeviceAuth.kind`.
- New goldens, in `apps/desktop/src/remote/fixtures/policy-goldens.json`, for both kinds: `machine-session-get-tools-kept`, `phone-session-get-tools-generic`, `machine-session-get-tool-detail-value-paths-reduced`, `machine-session-get-still-drops-secrets`, `machine-session-changed-tools-kept`, `phone-session-changed-tools-generic`. Event cases gained an optional `options`, read by both ports and by `policyGoldens.test.ts`.

### 10. `machine.permissions` and the far read behind it (18 November 2026)

New, desktop only (`machine.*` is `never` in `tiers.json`).

```jsonc
// near side: machine.permissions
{ "machineId": "…", "sessionId": "… (optional)" }
// reply: the far gateway's own `permission.pending` answer, unchanged
{ "permissions": [ {
  "id": "…",
  "sessionId": "…",
  "kind": "shell",              // the normalized tool kind: shell, read, edit, notebook_read, unknown
  "title": "SHELL: npm test",   // the card's own summary sentence
  "detail": "npm test",         // the command or the target, as the card shows it
  "choices": ["allow", "deny"]  // this card's own labels; answer with one of these
} ] }
```

- **Which fields exist, exactly:** `id`, `sessionId`, `kind`, `title`, `detail`, `choices`. There is no `toolName` and no `summary` field: `kind` is the tool family and `title` is the summary sentence, so a screen that reads `toolName`/`summary` should read `kind`/`title`.
- Not in the reply, ever: the provider's `raw` payload, the remembered-rule `matchKey`, `rememberable`, `rememberLabel`, project ids and names. That is exactly what `app.snapshot`'s `permissions` already gives a phone.
- `sessionId` filters to one conversation; without it the reply is every pending card of every unarchived conversation on that machine, oldest first. An unknown `sessionId` is an empty list; an unknown `machineId` is `not_found`.
- The far method is `permission.pending { sessionId? }`, class `read` at `viewer` in `tiers.json`, redacted by the same field watcher as `app.snapshot` (so a phone may call it too, and sees the same cards it already saw in the snapshot).
- **Answering:** `machine.permission.reply { machineId, permissionId, choice }`, where `choice` is one of the card's own `choices` (`allow`, `allow_once`, `accept`, `yes` … or `deny`, `reject`, `no`, `deny_once`, `deny_always`, and for an `ask_user` card `answer`/`dismiss`). Optional `remember: true` with `rememberScope: "bot" | "project"` is passed through for a `rememberable` card. It needs the far side at `operator` *and* "approvals from remote" on there; the reply is the far gateway's own refusal sentence when either is missing.
- **Live updates:** `machine.session.changed` now also fires for `permission.requested` and `permission.resolved` (both payloads carry `sessionId`; `permission.resolved` gained one on every emit site for this), so a screen showing the card list re-reads `machine.permissions` for that conversation when either arrives.

## Running log

### 2026-11-18 — Resumed, and the whole lane runs green

Ran in order after the owner's pause, on the build described under "Paused":

- `cargo test -p retinue-storage -p retinue-daemon --bin retinued --no-run` (shared target, build script stubbed for compiling only, my crate roots touched so another lane's artifacts could not stand in). The linked binary was copied to `C:\tmp\retinued-r2.exe` and `--list` held every test named below before anything was believed.
- `remote::policy::tests::goldens` — **4 passed**, `goldens_results`, `goldens_snapshots`, `goldens_events`, `goldens_authorize`. The one failure on the first run was my own new case `machine-session-get-still-drops-secrets`: I had written its `expect` with `file_path` unchanged, while the machine rule cuts a value that is itself a path to its last two segments. The implementation was right; the case's `expect` now says `src/lib.rs`, regenerated from the implementation's answer, and nothing in the redaction code changed for it.
- `remote::tests::` — **90 passed, 0 failed**, including `a_machine_device_reads_the_work_log_and_a_phone_does_not`, `the_event_socket_sends_the_work_log_to_a_machine_and_not_to_a_phone`, `permission_pending_is_read_by_a_machine_and_by_a_phone`, and `the_tier_table_matches_the_gateway_every_remote_method_is_never` (which pins the new `permission.pending` row against the dispatch).
- `machines_tests::` — **76 passed, 1 failed**, the failure being `one_offline_machine_does_not_hold_up_another`, the test `verification.md` records as flaky under full-suite concurrency. Run alone: **passed** in 3.9 s. So 77 of 77 pass, one only when not sharing the suite.
- `cargo clippy -p retinue-storage -p retinue-daemon --all-targets -- -D warnings` — clean.
- `node scripts/check-format.mjs` — passed, 1,480 files. `node scripts/check-architecture.mjs` — passed.
- `build.rs` and `skill_library.rs` are restored (`git status` shows neither); the copied binaries stay in `C:\tmp`, outside the tree. Still not run here: `npm test` (no `node_modules` in this worktree), which is the TypeScript half of the goldens.

### 2026-11-18 — PAUSED (owner paused all work)

**Done and verified by running** (the previous test binary, before the last edit): pieces 1, 2 and 3 (the live socket, `machine.sessions`, `link.startedFrom`, the real pairing platform, the two threat rows) — 76 `machines_tests` and 143 `remote` tests passed, clippy clean, architecture and format gates pass.

**Done for A and B, compiled but not yet run:**

- A. Kind-aware redaction: `RedactOptions.device_kind` / `deviceKind` in `remote/policy.rs` and `remoteTiers.ts`, the six new goldens, `redact_event(event, options)` with the gateway passing `RedactOptions::for_kind(&device.kind)` for both results and the event socket, and the section 16 row. Three new gateway tests were written **and passed** against the build before the last change: `remote::tests::a_machine_device_reads_the_work_log_and_a_phone_does_not`, `theme_event_socket_sends_the_work_log_to_a_machine_and_not_to_a_phone`, `permission_pending_is_read_by_a_machine_and_by_a_phone`.
- B. `permission.pending` (far, `read`/`viewer`), `machine.permissions` (near pass-through), `sessionId` added to every `permission.resolved` emit, and `machines_tests::the_other_machines_pending_approvals_are_read_through_the_link`, which also passed.
- **The one edit made after that passing run** is small and only narrows the machine rule: the host-path reduction inside a tool's `detail` now applies to `deviceKind: "machine"` alone and not to `includeToolOutput`, so the pre-existing goldens (`fix39-activity-fields-allowed`, `session-get-tool-output`) keep their byte-for-byte answers. It compiled (test binary linked, `C:\tmp\retinued-b2.exe`) but **the goldens have not been re-run since**. If anything fails on resume it will be in `goldens_results`/`goldens_snapshots`, and the expected fix is in the golden file, not the code (the TypeScript side was changed the same way in the same edit).

**Not started:** nothing from the briefs is left. The report's contract sections 9 and 10 are written.

**To resume, in order:**

```powershell
# 1. Stub the daemon's build script for compiling only (Smart App Control refuses it):
#    move crates/retinue-daemon/build.rs aside, replace the include! in
#    src/skill_library.rs with an empty LIBRARY_FILES, and restore both at the end.
# 2. One invocation, so the test-profile build of retinue-storage is mine:
$env:CARGO_TARGET_DIR='C:\Users\Sico Vibes\Desktop\DevCN\Retinue-retinue\backend-work-from-the-owner-assigned\target'
$env:TEMP='C:\tmp'; $env:TMP='C:\tmp'; $env:RUST_MIN_STACK='33554432'
cargo test -p retinue-storage -p retinue-daemon --bin retinued --no-run
# 3. Copy the newest deps\retinued-*.exe to C:\tmp\ (a fresh link is often blocked;
#    delete it and relink until one runs), then:
C:\tmp\retinued-<n>.exe remote::policy::tests::goldens
C:\tmp\retinued-<n>.exe remote::tests::
C:\tmp\retinued-<n>.exe machines_tests::
# 4. Gates: cargo clippy -p retinue-storage -p retinue-daemon --all-targets -- -D warnings
#    node scripts/check-format.mjs ; node scripts/check-architecture.mjs
# 5. npm test from a checkout with node_modules (the goldens' TypeScript half).
```

The worktree holds no half-written file and no conflict marker; `git status` lists only the real edits, and nothing is committed.

### 2026-11-17 — report written

Nothing implemented yet. Everything above is the design I am building to; a shape that changes gets a dated line here.

### 2026-11-17 — the gateway's event socket takes a machine-kind device (confirmed)

`03-design-and-order.md` section 7 lists this as unconfirmed. It is confirmed by reading `remote/web.rs`:

- `events_get` (`remote/web.rs:879`) authenticates with the device cookie only — `authenticate` (`:351`), which checks the token digest in constant time, revocation, idle expiry, the Tailscale login header when present, and the device's tier (`:387`). **There is no device-kind check on the socket**, and no tier check beyond "the device has a valid tier", so a `machine`-kind device at `viewer` opens it exactly as a phone does.
- What it then receives is `policy::redact_event` (`remote/policy.rs:638`), which drops every event whose type is not `allow` in `tiers.json`. That allowlist is per event type, not per device kind, so a machine link at `viewer` sees exactly what a phone at `viewer` sees.
- The socket requires a matching `Origin` on the handshake (`require_origin`, `:117`), which the machine client sends as its link's own origin, the same value its RPC calls already send.
- One live socket per device (`register_socket`, `remote/mod.rs:534`): a second socket for the same device **kicks the first**. A machine link holds one socket per link, so the only way to open a second is to open it twice from this daemon; the stream supervisor holds exactly one per machine.

### 2026-11-17 — pieces 1, 2, 3 built

Files, so the Director can read along:

- `crates/retinue-daemon/src/machine_client.rs`: `events_socket` / `events_socket_direct`. The one place a socket to another machine is opened: link origin for `Origin`, `wss://<origin>/api/events`, the stored device cookie, the same `Trust` (pin or web PKI) as every call, a connect timeout, and a 401 mapped to `Revoked` so the link flips to `revoked` as the RPC path does.
- `crates/retinue-daemon/src/machine_stream.rs` (new): the cursor, the supervisor, one follower per online link, the probe, the backoff, and `machine.session.changed`. `Timing` defaults: idle probe 25 s, probe wait 10 s, healthy 60 s, backoff 1 s → 5 min with up to 250 ms of jitter, sweep 5 s.
- `crates/retinue-daemon/src/machines.rs`: the snapshot now keeps the far machine's conversations and OS family; `machine.sessions` and `machine.stream.status`; `machine.changed` carries `live`; `machine.info` carries `platform`; the pairing request reports the real OS; `refuse_if_moved_elsewhere` is `pub(crate)` for the stream.
- `crates/retinue-daemon/src/main.rs`: `mod machine_stream` and the two dispatch arms.
- `crates/retinue-daemon/src/remote/web.rs`: `stamp_started_from`, called after a successful `agent.conversation.create`.
- `crates/retinue-storage/src/lib.rs`: `set_session_started_from` (one lock, one read, one write: the 17 November vault lesson).
- `apps/desktop/src/remote/tiers.json`: `"machine.session.changed": "drop"` beside `machine.changed`. No method rows were added: `machine.sessions` and `machine.stream.status` are covered by the existing `machine.*` `never` row, which is what the parity test matches on.
- `docs/remote/THREAT_MODEL.md` section 16: two rows and the live-socket paragraph.

### 2026-11-17 — what was run, and what it showed

Two real daemons in one process, the far one behind its Native TLS gateway on loopback with its own self-signed certificate, paired through the real `machine.pair.begin` → desktop approval flow: that is the harness every test below uses (`machines_tests::live_link`). The socket therefore crosses a real pinned-certificate TLS connection and a real WebSocket upgrade.

| Test | What it proves | Result |
|---|---|---|
| `machine_stream::tests::*` (4) | the backoff reaches exactly the five-minute cap and never passes it; a sequence at or below the cursor is dropped and each machine keeps its own; the connected flag is announced once per change; an unusable address is the one error that ends a follow | pass |
| `machines_tests::a_live_socket_reports_a_change_on_the_other_machine_as_ids_only` | `machine.list` and `machine.stream.status` say live at `viewer`; a rename on the far machine arrives as `machine.session.changed` with the machine id, the session id and a revision, containing no title and no bot name; the cursor moves; `redact_event` drops the event for a device | pass |
| `machines_tests::a_dead_socket_comes_back_by_itself_and_nothing_is_reported_twice` | the socket dies when the far gateway stops (no frame, no probe answer), `connected` goes false, the retry reconnects at the same address on its own with one `wake`, the replay from the cursor reports nothing twice, and the next real change has a higher revision | pass |
| `machines_tests::a_conversation_that_moved_away_from_the_other_machine_is_not_reported_live` | the far sequence still advances the cursor, but no `machine.session.changed` is emitted for a conversation this machine recorded as moved elsewhere | pass |
| `machines_tests::the_other_machines_conversations_are_listed_from_memory` | `machine.sessions` returns both rows with `machineId`, `machineName`, `kind`, `agentName`, state, time and `archived`; one machine's rows; `not_found` for an unknown id; empty before the first read; `kind` on the link row and `platform` on `machine.info` are the OS family | pass |
| `machines_tests::a_conversation_started_through_the_gateway_says_where_it_came_from` | the pairing request reports the real platform label; `machine.start` writes `link.startedFrom {deviceId, deviceKind, machineName}` on the far row; the mark comes back over the gateway with exactly three keys; a conversation started at that machine has none | pass |
| `machines_tests::an_older_machine_reports_no_os_family_and_still_links` | a far gateway without `platform` still links, with `kind: null` rather than a guess | pass |
| the whole `machines_tests::` module, including `handoff_tests.rs` | nothing else in the machine surface changed: **76 passed, 0 failed** | pass |
| the whole `remote::` module | the gateway and its policy are unaffected, including `the_tier_table_matches_the_gateway_every_remote_method_is_never`: **143 passed, 0 failed** | pass |
| `retinue-storage` `a_started_from_mark_joins_the_link_and_keeps_it` | the merge keeps `link.kind` and replaces only `startedFrom`, and an unknown conversation is an error | **written, not compiled or run here**: the storage crate's *test* target cannot be built on this PC right now (`serde`'s build script and `serde_derive.dll` are refused by Smart App Control; the library itself compiles). CI compiles and runs it |

Gates:

- `node scripts/check-architecture.mjs` — passed.
- `node scripts/check-format.mjs` — passed, 1,480 files. (`cargo fmt` is not a gate in this repository.)
- `cargo clippy -p retinue-storage -p retinue-daemon --all-targets -- -D warnings` — finished with no warning.
- **Could not run here:** `npm test` / `npx vitest` (this worktree has no `node_modules`, and installing would race the other lanes). The TS `remoteTiers.parity.test.ts` therefore did not run; by hand, `EVENT_POLICY` is generated from `tiers.json` (so the new `drop` row is picked up), the daemon event scan reads `.emit("machine.session.changed"` from `machine_stream.rs`, and `machine.sessions` / `machine.stream.status` are matched by the existing `machine.*` family row. The Director's `npm test` run is the check that proves it.
- **Smart App Control, as the 17 November notes describe:** the daemon's `build.rs` cannot run on this PC, so for compiling and testing only I moved it aside and stubbed the `include!` in `skill_library.rs`; **both are restored** (`git status` shows neither) and the test binary I ran was linked from that stub, so built-in-skill tests were not part of my runs (they do not touch this change). A newly linked test binary is refused by the policy; it took relinks until one ran (`C:\tmp\retinued-run5.exe`), and the numbers above come from that binary.
- **A private target is left behind:** `C:\Users\Sico Vibes\Desktop\DevCN\Retinue-retinue\backend-task-claude-15-target` (about 9 GB, `debug\deps` seeded from the shared one). Delete it once this lane's diff is reviewed. Its `serde` build script is now refused, so it is only good for reading; the shared target is untouched.

## What is deliberately not done

- **Piece 4** (the approval card in the machine's conversation) — the brief leaves it to the Director's screens; `machine.permission.reply` already exists at `operator`.
- **A `startedFrom` mark on `session.prompt`** — the design says "`agent.conversation.create` or `session.prompt` on a new conversation". I stamp only the create. A `session.prompt` cannot tell a conversation this gateway started from one the owner started at that machine, and marking the owner's own conversation as "started from the laptop" would be false. If the Director wants the wider rule, it needs a stored "created by this device" fact, not a guess at the prompt.
- **Archived far conversations** — `machine.sessions` reports `archived`, but the 20-second read still asks `session.list` without `includeArchived`, so the flag is false in practice. Widening that read is one parameter, but it changes the bot counts the sidebar shows, so it is the Director's call.
- **Message text or tool titles from another machine** — a content rule (open question 9 in the design), an owner decision. The socket carries ids only for that reason.
- **A socket to a machine that is `offline` or `revoked`** — never opened. It starts when the 20-second read first says `online`, and the loop stops it on removal.

## Notes for the Director

- **T3 code.** Nothing was copied or translated from `_research/t3code`. The backoff shape (exponential, capped, up to 250 ms of jitter, reset only after a healthy period) is the one our own phone client already uses in `apps/desktop/src/remote/remoteTransport.ts`; the dead-socket probe is ours (a WebSocket ping, which axum answers with a pong automatically). `docs/legal/third-party-notices.md` needs no change.
- **`Charts`/UI:** the conversation dialog can stop its 3-second timer for a machine whose `live` is true and re-read on `machine.session.changed`; `machine.sessions` is what a merged list should render, and `machine.list`'s `kind` picks the icon.
- **Order of arrival:** a `machine.session.changed` for a conversation the window is not showing is still a row change; the list should refresh from `machine.sessions` rather than from `machine.conversations`.
- **What the socket does not carry:** `message.delta` (half a message is content) and `tool.changed` (`drop` for devices). A running turn's step still arrives through the far side's redaction on a re-read, so the laptop sees `Tool` with no detail until open question 9 is answered.
- **Skill:** I updated `.claude/skills/retinue-development`: `references/codebase-map.md` (the live-link module and its RPCs, event, and row fields) and `references/lessons.md` (2026-11-17, "A shared `CARGO_TARGET_DIR`…": a lane can silently compile against another worktree's build of a crate it just changed, and how to seed a private target so that cannot happen).


