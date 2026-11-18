# Multi-device, step 4: the phone works through its host's machine links

> **Lane:** Ada, backend, worktree `backend-task-from-the-director-claude-20`, `main` at `b7c3a32`. Nothing is committed.
> **Sources:** `docs/research/multi-device/03-design-and-order.md` section 2, piece 5, and section 3 amendment A1; the owner's amendment "Retinue on many machines" (17 November 2026, `AGENTS.md` in the main checkout); `docs/remote/THREAT_MODEL.md` section 16; the live lane's contract (`docs/reports/multi-device-live.md`, worktree `-15`).
> **Read this first:** the Contract below is what the Director builds the phone screens from. A reply shape changes only with a line under **Changes**, dated.
> **What this lane does not touch:** the phone screens (the Director's), pairing, linking, renaming, removing machines, moving a conversation, another machine's Remote settings. Those stay refused for every paired device, exactly as they are.

## The rule this implements

The owner, 17 November 2026: *"The phone works through its host. The phone pairs with one host and may watch, start and steer work on the machines that host is linked to, at the lower of the phone's tier and the link's tier. It may never pair, link, rename or remove machines, move a conversation, or manage another machine's devices."*

So the phone keeps one pairing, one cookie and one revocation, and gains reach into whatever that host has linked. Two ceilings apply, and the far gateway is still the authority:

```
what the phone may do on machine M = min(phone's tier on the host, host's tier on M)
```

## Contract (stable)

Every method name and every key below is what the daemon answers today. `…` is an opaque id. Line numbers are in `crates/retinue-daemon/src` at the time of writing and will drift; the module names will not.

### 0. The `linked` class, and who it is for

A new class in `apps/desktop/src/remote/tiers.json`: `"linked"`, granted to device kinds **`phone`, `tablet` and `browser`** and refused to **`machine`** (a link calling another link is how a mesh forms by accident) and to **`computer`**. The host's quiet hours refuse its work methods (`machine.start`, `.prompt`, `.steer`, `.cancel` and `.permission.reply`); reading the machines and their conversations passes, as a read does.

| Method | `minTier` | The far side's own class | What the phone gets |
|---|---|---|---|
| `machine.list` | `viewer` | — (memory on the host) | the names, kinds, states and bots of the host's links |
| `machine.sessions` | `viewer` | — (memory on the host) | the links' conversations, as the live lane reads them |
| `machine.conversation` | `viewer` | `session.get` at `viewer` | one conversation, redacted by the far gateway |
| `machine.start` | `chat` | `agent.conversation.create` at `chat` | `{sessionId, machineId}` |
| `machine.prompt` | `chat` | `session.prompt` at `chat` | the far side's own reply |
| `machine.steer` | `chat` | `session.steer` at `chat` | the far side's own reply |
| `machine.cancel` | `chat` | `session.cancel` at `chat` | the far side's own reply |
| `machine.permission.reply` | `operator` | `permission.reply` at `approve` | `{resolved, rule?}`, only when the far machine allows approvals from remote |

Still `never`, so a phone cannot reach them: `machine.self`, `machine.self.rename`, `machine.info` (it is `read`, unchanged), `machine.probe`, `machine.pair.begin`, `machine.pair.cancel`, `machine.pair.list`, `machine.rename`, `machine.remove`, `machine.refresh`, `machine.takeOver*`, `machine.projects`, `handoff.*`, every `remote.*`, `settings.set`, and a whole family row still refuses any `machine.*` method nobody listed above.

The kind check is the daemon's, in `remote/web.rs`: a `machine`-kind device that calls a `linked` method is refused `forbidden_remote` with "Only a paired phone, tablet or browser can do this through its host." — the mirror of the sentence `handoff.*` already gives a phone.

### 1. `machine.list` — the host's links, for a phone

Existing RPC. A phone caller gets the same rows the desktop's Machines page reads, with four fields removed: `url`, `fingerprint`, `auth` and `pairedAt`. They are the far machine's address and its certificate fingerprint, and a phone has no use for either.

```jsonc
// params
{}
// reply
{ "machines": [ {
  "id": "…",                          // the far machine's own machine id
  "name": "Home PC",
  "kind": "windows",                  // "windows" | "linux" | "macos" | null (an older far daemon)
  "state": "online",                  // "online" | "offline" | "revoked"
  "detail": null,                     // a plain sentence when the link is away, or null
  "live": true,                       // the live lane's socket: true while events arrive from it
  "tier": "operator",                 // this host's tier on that machine
  "lastSeenAt": "2026-11-17T09:14:02.114Z",
  "version": "0.1.0-beta.60",         // the far daemon's version, null before the first read
  "bots": [ {
    "id": "…", "name": "Ada", "role": "Backend engineer",
    "color": "#rrggbb", "outfit": "…", "look": { … },
    "provider": "codex", "model": "gpt-6-luna",
    "busy": false, "activeSessionId": null,
    "conversations": 3
  } ]
} ] }
```

- `url`, `fingerprint`, `auth` and `pairedAt` are **absent** for a phone (the desktop still gets them).
- An offline machine keeps its last known row; `state` and `detail` say whether it is current.
- Errors: none of its own.
- `kind` and `live` come from the live lane (worktree `-15`). Until that lane merges, both are absent, and the phone UI must treat a missing `kind` as "unknown machine" and a missing `live` as "polling".

### 2. `machine.sessions` — the links' conversations

The live lane's RPC (their report, section 2), reached by a phone. The reply is exactly theirs; this lane only grants it and caps it:

```jsonc
// params: {} or { "machineId": "…" }
{ "sessions": [ { "machineId": "…", "machineName": "Home PC", "kind": "windows",
  "id": "…", "title": "Invoice parser", "agentId": "…", "agentName": "Ada",
  "projectName": "Retinue", "state": "working",
  "updatedAt": "2026-11-17T09:14:02.114Z", "archived": false } ] }
```

- Errors: `not_found` for an unknown `machineId`; nothing for an offline machine.
- `machine.sessions` exists in the live lane's worktree, not in this one. See **Open at merge**.

### 3. `machine.conversation` — one conversation on a linked machine

```jsonc
// params
{ "machineId": "…", "sessionId": "…" }
// reply
{ "session": { … } }   // the far host's `session.get`, redacted by its gateway, then by this host
```

Errors: `not_found` (unknown machine or conversation), `provider_unavailable` (away or revoked link), the far side's own code and sentence otherwise.

### 4. `machine.start` — start a conversation there

```jsonc
// params
{ "machineId": "…", "agentId": "…", "title": "optional", "projectId": "optional", … }
// reply
{ "sessionId": "…", "machineId": "…" }
```

- Every parameter except `machineId` is forwarded to the far side's `agent.conversation.create`.
- Errors, in this order: `invalid_argument` (a missing id), `not_found` (an unlinked machine), `provider_unavailable` (away; or revoked, with the "pair it again" sentence), `tier_required` (either ceiling), then the far side's own refusal in its own words.

### 5 to 7. `machine.prompt`, `machine.steer`, `machine.cancel`

```jsonc
// machine.prompt — params
{ "machineId": "…", "sessionId": "…", "text": "…", "delivery": "queue"?,
  "clientMessageId": "…"?, "command": "…"?, "conversationRefs": ["…"]? }
// reply: the far side's `session.prompt`
{ "turnId": "…", "accepted": true, "state": "working" }

// machine.steer — params
{ "machineId": "…", "sessionId": "…", "text": "…" }
// reply, either
{ "steered": true, "turnId": "…" }
{ "steered": false, "queued": true, "item": { … } }

// machine.cancel — params
{ "machineId": "…", "sessionId": "…" }
// reply, either
{ "cancellationRequested": true, "cascaded": false }
{ "cancellationRequested": false, "reason": "turn_changed" }
```

- Attachments are refused on every remote path, for a phone as for the desktop: only an empty `attachments` array passes (`invalid_params`, "Attachments stay on the device that has them."). The phone sends text.
- A conversation this host recorded as moved elsewhere is refused `conflict` with "This conversation moved to <name>. Take it over to continue." — a phone cannot take over.
- Errors: as section 4, plus `conflict` for a move.

### 8. `machine.permission.reply` — answer an approval raised there

```jsonc
// params
{ "machineId": "…", "permissionId": "…", "choice": "allow" | "deny" | "allow_once",
  "remember": false?, "rememberScope": "bot" | "project"? }
// reply
{ "resolved": true, "rule": { "id": "…", "label": "…", … }? }
```

- Needs **both** ceilings at `operator`, and the far machine's own "approvals from remote" switch on. The far gateway answers `approvals_disabled` (403, "approvals from remote are turned off on this host") when it is off; this host cannot know that switch, so it does not guess — it forwards, and the refusal reaches the phone unchanged.
- A proposal's card is refused as on the desktop (`invalid_argument`, "Decide a proposal with proposal.decide.").
- Errors: `tier_required`, `approvals_disabled`, `not_found`, `invalid_argument`.

### 9. Event `machine.session.changed` — reachable by a phone

The live lane emits it locally when a linked machine's conversation changes. It now crosses to `phone`, `tablet` and `browser` devices, and to nothing else:

```jsonc
{ "v": 1, "sequence": 42, "type": "machine.session.changed",
  "payload": { "machineId": "…", "sessionId": "…", "revision": 42 } }
```

- Content-free: ids and a revision. No title, no text, no tool name, no path.
- `revision` is the far machine's event sequence; a phone ignores a revision at or below the one it holds.
- A `machine`-kind device never receives it (see section 0). A `computer` device never receives it.
- What the phone does with it: re-read `machine.sessions` for the list and `machine.conversation` for the one it is showing. Nothing else arrives.
- **Not yet relayed:** `machine.changed` (a machine's state changed) is still `drop`, so a phone's picker learns that a machine went offline only from its next `machine.list`. One line in `tiers.json` and one in the parity test would change that; it is the Director's call, because it widens what a phone sees beyond this piece.

### 10. The refusal sentences a phone shows, unchanged

The near side answers before any request when it can, and the far side's own words reach the phone whenever it answers:

| Code | HTTP | When | Sentence |
|---|---|---|---|
| `forbidden_remote` | 403 | a `machine`/`computer` device, or a method nobody granted | "Only a paired phone, tablet or browser can do this through its host." |
| `tier_required` | 403 | either ceiling below the method's tier | "<Action> on <machine> needs <Tier> there. This device is <tier> and this computer is linked as <tier>, so it works as <tier>. Change the tier in <machine>'s Remote settings." |
| `approvals_disabled` | 403 | the far machine's approvals switch | the far machine's own sentence |
| `quiet_hours` | 403 | the host's quiet hours, on a work method | "quiet hours are on; this host only allows reading right now" |
| `provider_unavailable` | 503 | the link is away, or revoked | "<machine> can't be reached right now." / "<machine> removed this computer's pairing. Pair it again." |
| `conflict` | 409 | the conversation moved | "This conversation moved to <machine>. Take it over to continue." |

### 11. What the phone does *not* learn

No host path, no certificate fingerprint, no machine address, no device token, no `machine.id` of the phone's own host, no tool output. The far gateway's redaction is applied to everything the phone reads, as it is for any device today.

## Changes

- **2026-11-17 — one new private parameter on `machine.*` calls.** The gateway adds `remoteTier` to a `linked` method's params (always overwriting a client's own value) so the daemon can apply the lower ceiling. Nothing else changes shape; the far machine never sees it, and the window neither sends nor receives it. A client that sends one is ignored — which is the point of the test that sends `viewer` to itself and `operator` the other way round.

## Running log

### 2026-11-17 — report written

Nothing implemented yet. Everything above is the contract I am building to; a shape that changes gets a dated line under **Changes**.

### 2026-11-17 — the class, the kind check and the two ceilings are in

Where the code is, so review has the map:

- **The class.** `apps/desktop/src/remote/tiers.json`: a `linked` row for each of the eight methods, replacing the four `never` rows for `machine.start`, `.prompt`, `.steer`, `.cancel` and `.permission.reply`; `machine.projects` stays `never`, and the `machine.*` family row still refuses everything not named. `machine.session.changed` is `linked`.
- **The one policy, two languages.** Rust `crates/retinue-daemon/src/remote/policy.rs`: `MethodClass::Linked`, `EventPolicy::Linked`, `linked_kinds`, `linked_is_work`, `redact_event_for(event, kind)`, and `redact_result` walks a `linked` result with the field watcher as it does a read. TypeScript `apps/desktop/src/remote/remoteTiers.ts` mirrors all of it (`MethodClass`, `LINKED_WORK_METHODS`, `EventPolicy`, `redactEventForRemote` accepts a `linked` type because a client is only ever a phone, tablet or browser). Four cases in `fixtures/policy-goldens.json` pin the two readings to each other.
- **Who may ask.** `remote/web.rs` refuses a `linked` method to any device kind but `phone`, `tablet` and `browser`, with "Only a paired phone, tablet or browser can do this through its host."
- **The stamp.** The gateway puts the calling device's tier into the call as the private parameter `remoteTier` (always written, so a client's own value is discarded), and `machines.rs` reads it: `require_tier` now works at the lower of it and the link's tier, with a sentence that names both. `far_params` strips it, so the other machine never sees it.
- **What a phone sees.** `machine.list` for a paired device drops `url`, `fingerprint`, `auth` and `pairedAt`.
- **Quiet hours.** A `linked` method that drives work is refused during the host's quiet hours, in both languages; reading still passes.
- **The activity list.** A phone's `machine.start`, `.prompt`, `.steer`, `.cancel` writes one content-free line with kind `prompt`; `.permission.reply` writes kind `approval`.
- **The relay.** The event socket sends an event through `redact_event_for(event, kind)`, so `machine.session.changed` reaches a phone, tablet or browser socket and never a `machine` or `computer` one.

### 2026-11-18 — done, and what was run

178 Rust tests pass on the final source (8 + 13 + 70 + 87), clippy is clean with `-D warnings`, the TypeScript `src/remote` suite passes (243), and both hygiene checks pass. The one clippy finding was real: `redact_event` became dead code once the socket moved to the kind-aware function, so the kind-aware function now calls it for the `allow` case.

The numbers come from a private target in this worktree (`<worktree>\target`, copied from the shared one) because the shared target hands out another lane's executable under the same name. The details, and the two relinks Smart App Control refused before one ran, are in the table above and in `lessons.md`.

Nothing is committed. The contract at the top of this file did not change while it was built: every shape I wrote on the first pass is the shape the daemon answers.



## Open at merge

Two files this lane and the live lane (worktree `-15`) both touch. Both are one-line reconciliations, and neither changes a shape:

1. **`machine.sessions`'s `tiers.json` row.** The live lane adds no row for it (the `machine.*` family row covers it as `never`). This lane adds an exact `linked` row, which needs a dispatch arm to exist so the parity test is not "stale" — so this lane also adds `"machine.sessions"` to the `machine.*` arm in `main.rs`. At merge there are two identical arms: keep one. Until the live lane's implementation lands, a call answers `not_found` "unknown machine method", which is what it answers today for every method not in `machines::rpc`.
2. **The `machine.session.changed` event row.** The live lane sets it `drop`. This lane sets it `linked` (a new event policy value, accepted by both languages). Keep `linked`, or the phone never gets the relay.

The live lane also rewrites `machines.rs` (a `Snapshot` that holds sessions, `machine.sessions`, `machine_stream.rs`) and `main.rs`'s `machine.*` arm. This lane's changes there are separate functions (`require_tier`'s new parameter, `row_for_device`, `list_value`, `far_params`) and one arm line, so the merge is additive apart from the arm.

## Verification

What I ran, on `b7c3a32` plus this change, in this worktree:

| Gate | Result |
|---|---|
| `node scripts/check-format.mjs` | passed, 1480 files |
| `node scripts/check-architecture.mjs` | passed |
| `npx vitest run src/remote` (from `apps/desktop`, through a `node_modules` junction to the main checkout) | **13 files, 243 tests passed**, including `remoteTiers.parity.test.ts` (reads this lane's `main.rs` arm), `policyGoldens.test.ts` (the four new `linked` cases, read by both languages), and `remoteBuild.test.ts` (the phone PWA still builds inside its size budget) |
| `rustfmt --check` on the six touched Rust files | all six parse |
| `cargo test -p retinue-daemon --bin retinued --no-run`, in a private target copied from the shared one | the daemon **and its test module compiled from this worktree** and linked; the executable holds this lane's strings |
| `retinued machine_phone_tests::` | **8 passed, 0 failed** (the two ceilings, the stamp and its stripping, the reduced list, the kind refusal through the gateway, the phone's calls through the gateway with the activity line, the relay on a phone socket and not on a machine's) |
| `retinued remote::policy::tests::` | **13 passed** (the two new `linked` tests and the four golden cases, reading this lane's `tiers.json`) |
| `retinued machines_tests::` | **70 passed** (the machine-link suite, unchanged by this lane's `require_tier` and `machine.list` edits) |
| `retinued remote::tests::` | **87 passed** (the gateway suite, including the socket replay and the activity lines) |
| `cargo clippy -p retinue-daemon --all-targets --no-deps -- -D warnings` | **passed, exit 0, no warnings.** It first caught a real one: `redact_event` became dead code once the socket moved to `redact_event_for`; the kind-aware function now calls it for the `allow` case, which keeps one definition of "only `allow` crosses" and leaves the function in place for the live lane's stream code |
| The whole daemon suite in one run | not run: the four filters above are every area this lane touches, and a full run costs another half hour on a machine whose shared target is fought over |

All four numbers are from the binary built from the final source (after clippy's one finding was fixed). Smart App Control refused two freshly linked test binaries ("An Application Control policy has blocked this file") before the third link ran — the documented loop: delete the `.exe`, relink, try again; a copy to `C:\tmp` is blocked too, because the block follows the content.

**The shared target is not usable for a verdict.** `cargo test --no-run` in `backend-work-from-the-owner-assigned\target` reported an executable that held 1,236 test names, none of this lane's, and none of this lane's strings, including the `tiers.json` embedded with `include_str!`: another lane's build of the same unit had replaced the file. Even a `cargo check` there is not proof of this lane's code. The numbers above come from `<this worktree>\target`, a 55 GB copy of that target (robocopy; two crates and one proc-macro DLL had to be rebuilt inside it before cargo could use it). That directory is left in place so the run can be repeated; the release cleanup should delete it.



