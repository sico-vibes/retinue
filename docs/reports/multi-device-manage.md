# Piece 7: a linked machine manages another machine's devices

> **Lane:** Ada, backend. Worktree `backend-task-from-the-director-claude-18`, branch `retinue/backend-task-from-the-director-claude-18`. Nothing is committed.
> **Status:** done and run, both sides, on the source below. Every number and outcome is in "Running log". No shape below changed after it was first written.
> **Design:** `docs/research/multi-device/03-design-and-order.md` piece 7. **Rule it needs:** amendment A4, which is now the owner's 17 November amendment, "A machine may manage another machine's devices" (`AGENTS.md`). **Threat rows:** `docs/remote/THREAT_MODEL.md` section 16.
> **Read this first:** "Contract" is what the Director builds the screens from. A shape changes only with a line under "Changes" saying so.

Scope, as briefed: the far side (a device-facing, machine-only subset of `remote.*`), the near side (seven thin pass-throughs in `machines.rs`), and tests through `dispatch` and through the real gateway as a machine device and as a phone device.

---

## Contract (stable)

Nothing here starts work, carries message text or names a host path. Every method is a pass-through: the near-side reply is the far side's reply, unchanged.

### 1. The far side: `manage` class, machine devices only

Seven `remote.*` methods change class in `apps/desktop/src/remote/tiers.json` from `never` to **`manage`** (`minTier: "operator"`), plus one new method. Every other `remote.*` row stays `never`, and a phone at `operator` is refused all eight by the gateway's own kind check — the class alone never lets a phone through.

| Method | Class | Reads / changes | What it answers |
|---|---|---|---|
| `remote.status` | manage | read | the same status object the owner's window gets (below) |
| `remote.devices.list` | manage | read | `{devices: [...]}` |
| `remote.devices.update` | manage | **change** | `{device}` |
| `remote.devices.revoke` | manage | **change** | `{revoked: true}` |
| `remote.pair.pending` | manage | read, **new** | `{pending: {...} \| null}` |
| `remote.pair.confirm` | manage | **change** | `{approved: true, device}` or `{approved: false}` |
| `remote.activity.list` | manage | read | `{entries: [...]}` |

`manage` is not denied during the host's quiet hours. Quiet hours spare only reads today; a manage call starts no work, and revoking a lost phone must not have to wait for 07:00. The row's `why` says so, and the report's "Changes" records it.

**Refusals only the gateway can make** (each answers `403 forbidden_remote`, and each writes a `denied` activity line):

| Case | Sentence |
|---|---|
| device kind is not `machine` (a phone, tablet, browser or computer at any tier) | `Only a linked Retinue machine can manage another machine's devices.` |
| `remote.devices.update` or `.revoke` naming the caller's own row | `A machine link can't change or remove its own pairing.` |
| `remote.pair.confirm` where the waiting request is of kind `machine` | `A machine link can't approve another machine. Only the owner at that computer can.` |
| `remote.pair.confirm` where the waiting request came from the caller itself | `A machine link can't approve its own pairing request.` |

The waiting request records which device asked for it: `POST /api/pair` reads the caller's device cookie when one is valid, and keeps its id with the pending request (in memory, never in a reply). That is the only way "a request from the caller itself" is an identity and not a name comparison.

### 2. `remote.status` — unchanged object

The whole status object the desktop's own `remote.status` returns (`remote/mod.rs` `status_value`), with no new fields:

```jsonc
{ "enabled": true, "link": "online|starting|error|off", "url": "https://host.tailnet.ts.net| null",
  "port": 48620, "servePort": 443, "tailscale": "ready|not_installed|funnel_on|port_in_use|…",
  "hostName": "DESKTOP-7", "deviceCount": 3, "connectedCount": 1, "keepAwake": false,
  "error": "…|null", "method": "tailscale|native",
  "native": { "port": 48621, "urls": ["https://…"], "fingerprint": "AB:CD:…", "accountSet": true,
              "username": "owner", "listening": true, "loginPaused": false, "pausedUntil": null,
              "lastReachedAt": "2026-11-17T…Z|null", "lastReachedFrom": "this network|1.2.3.4",
              "upnp": {"consent": false, "state": "off|mapped|error", "error": "…"},
              "firewall": "unknown|present|absent", "externalIp": "1.2.3.4|null",
              "externalIpSource": "upnp|manual|null", "cgnat": false, "cgnatReason": "…|null" } }
```

What a machine device may therefore learn about another of the owner's computers: whether Remote is on, on which method, at which address and port, the certificate fingerprint it already pinned, the Native account label, and the last source it was reached from. No host path, no message text, no token. The sending machine is another desktop the owner physically controls, at `operator`, with a pinned certificate — the same trust section 16 already records for reading that machine's conversations.

### 3. `remote.devices.list` — the rows the owner's own window sees

```jsonc
{ "devices": [
  { "id": "uuid", "name": "Sam's phone", "kind": "phone|tablet|browser|computer|machine",
    "platform": "iOS 19", "tier": "viewer|chat|operator", "login": "sam@example.com|null",
    "pairedAt": "2026-11-17T…Z", "lastSeenAt": "…|null", "revokedAt": null, "signedOutAt": null,
    "online": true } ] }
```

Revoked, signed-out and idle-expired rows are left out, as for the desktop. `online` is derived from the live event socket. The caller's own row is in the list (a UI shows it as "this computer" by matching `id` with its own `machine.list` `deviceId`… which the near side does not expose — see "Open questions").

### 4. `remote.devices.update` — rename or change a tier

```jsonc
// params: deviceId required; name and tier optional, at least one of them
{ "deviceId": "uuid", "name": "Sam's phone", "tier": "chat" }
// reply
{ "device": { …the row above… } }
```

Refused: `invalid_argument` when `deviceId` is missing, `tier` is not one of the three names, or the row cannot take the update; `not_found` for an unknown id; `forbidden_remote` for the caller's own row.

### 5. `remote.devices.revoke`

```jsonc
{ "deviceId": "uuid" }        // reply: { "revoked": true }
```

Also deletes that device's push subscription and closes its live socket, exactly as the desktop's revoke does. `not_found` for an unknown id, `forbidden_remote` for the caller's own row.

### 6. `remote.pair.pending` — new read

```jsonc
// params: {}      // reply
{ "pending": { "pairingId": "uuid", "name": "Sam's phone", "kind": "phone|tablet|browser|computer|machine",
               "platform": "iOS 19", "login": "sam@example.com|null", "method": "tailscale|native",
               "expiresAt": "2026-11-17T…Z" } | null }
```

One request at a time is what the pairing state allows, so this is one object. `null` means nobody is waiting. A request of kind `machine` is shown, because the UI should say why it will not be honoured (see the refusal table) rather than hide it.

There is deliberately **no event** for this: a machine device is not sent `remote.pairing.requested` (it is `drop` in `tiers.json`). The screen polls `machine.pair.pending` while it is open — 2 s is what the linking machine itself polls at.

### 7. `remote.pair.confirm`

```jsonc
// params: approve:false        "pairingId", "approve", "tier": "viewer|chat|operator"
{ "pairingId": "uuid", "approve": true, "tier": "chat" }
// reply
{ "approved": true, "device": { …device row… } }   // approve
{ "approved": false }                              // reject
```

A tier is required when approving (`invalid_argument`), as on the desktop. `conflict` when there is no pending request, it was already decided, or it expired.

### 8. `remote.activity.list`

```jsonc
{ "limit": 100 }   // optional; 100 by default, 500 rows are kept at most
// reply
{ "entries": [ { "id": "1234", "at": "2026-11-17T…Z", "deviceId": "uuid|null",
                 "deviceName": "Sam's phone", "kind": "paired|revoked|connected|disconnected|denied|prompt|approval|handoff|manage…",
                 "summary": "Vex's laptop removed Sam's phone from another machine." } ] }
```

Newest first. **Every change a machine makes writes one line in that machine's own list, naming the machine that did it:**

| Action | Line (kind) |
|---|---|
| `remote.devices.revoke` | `<machine> removed <device> from another machine.` (`revoked`) |
| `remote.devices.update` | `<machine> changed <device>'s access from another machine.` (`manage`) |
| `remote.pair.confirm`, approve | `<machine> approved <device> from another machine.` (`paired`) |
| `remote.pair.confirm`, reject | `<machine> refused <device> from another machine.` (`manage`) |
| any refusal in the table above | `<machine> was refused <method>.` (`denied`) — the gateway's existing denial line |

The four reads write nothing. The desktop's own calls keep today's lines unchanged (`<device> was removed.`, `<device> was paired.`): a line naming the actor is written only when the call arrived from a machine.

### 9. The near side: seven pass-throughs (`machines.rs`)

Each takes `machineId` and forwards every other parameter unchanged. Each needs tier `operator` **on the link** (checked before a request is made, with the plain sentence) and every one is `never` remote: the `machine.*` family row already covers them, and a phone cannot reach another machine through its host.

| Near method | Far method | Tier on the link | Kind |
|---|---|---|---|
| `machine.remote.status` | `remote.status` | operator | read |
| `machine.devices.list` | `remote.devices.list` | operator | read |
| `machine.devices.update` | `remote.devices.update` | operator | write (refreshes the link's snapshot) |
| `machine.devices.revoke` | `remote.devices.revoke` | operator | write |
| `machine.pair.pending` | `remote.pair.pending` | operator | read |
| `machine.pair.confirm` | `remote.pair.confirm` | operator | write |
| `machine.activity` | `remote.activity.list` | operator | read |

Params: `{"machineId": "<link id>"}` plus the far side's own params (`deviceId`, `name`, `tier`, `pairingId`, `approve`, `limit`). The near side strips `machineId`; nothing else is added.

**Refusals the near side gives before any request** (the far gateway stays the authority and its own refusal is passed through unchanged):

| Case | Code | Sentence |
|---|---|---|
| no `machineId` | `invalid_argument` | `machineId is required` |
| unknown link | `not_found` | `That machine isn't linked.` |
| link not online | `provider_unavailable` | `<name> can't be reached right now.` |
| link revoked | `provider_unavailable` | `<name> removed this computer's pairing. Pair it again.` |
| link below operator | `tier_required` | `<action> on <name> needs Operator there, and this computer is linked as Chat. Change this computer's tier in <name>'s Remote settings.` |

Actions in that last sentence: `Reading Remote`, `Listing the paired devices`, `Changing a device`, `Removing a device`, `Reading the waiting request`, `Approving a device`, `Reading the activity list`.

### 10. Events

None new, and none changed. The far side's `remote.activity`, `remote.device.changed` and `remote.pairing.*` events are all `drop` for devices, so a machine learns of a change by reading again — which is what its screen does after each write. `machine.changed` still carries only `{machineId, state}`.

### 11. Bot tools

None. This piece adds no tool a bot can call.

---

## Running log

### 2026-11-17, start

Read the skill (`codebase-map.md`, `verification.md`, `lessons.md` for every 2026-11-17 entry), the 17 November amendment, and the four research files. Read `remote/mod.rs`, `remote/web.rs`, `remote/pairing.rs`, `remote/policy.rs`, `machines.rs`, `tiers.json`, the parity test and `machines_tests.rs`.

Facts that shape the implementation:

- `policy::authorize` sees the tier and the method, never the device kind. `handoff.*` is gated by a kind check in `web.rs` `rpc_post` (`web.rs:819`). The seven `manage` methods need the same kind check there, plus the three row-specific checks the class cannot express.
- A dispatch arm is needed for the new `remote.pair.pending` in `main.rs`, or the desktop cannot reach it and the parity test calls the row stale (wave 2's `task.checkBack`).
- `register_socket`/`close_socket` already give `online`, so `remote.devices.list` needs nothing new.
- The near side already has `call_through`/`write_through` with `require_online` and `require_tier`; the seven methods are one line each beside `machine.permission.reply`.

### 2026-11-17, far side written

- `remote/pairing.rs`: `Pending` carries `device_id` (the asking device, from its cookie; never in a reply); `submit` takes it; `waiting()` and `waiting_identity()` were added beside `pending()`.
- `remote/web.rs`: `pair_post` reads the caller's own device from its cookie, if any; `rpc_post` refuses a `manage` call from any kind but `machine` and applies the three row rules (`manage_refusal`, beside `audit_kind`), then stamps `actor {deviceId, name}` into the params for the handlers.
- `remote/mod.rs`: `remote.pair.pending` (`pair_pending`), `ACTOR_KEY`/`actor_of`, and the actor-aware activity lines in `pair_confirm`, `devices_update`, `devices_revoke`. `audit_kind` writes no line for `manage`, because each handler writes the better one.
- `remote/policy.rs`: `MethodClass::Manage`, parsed from `"manage"`, operator-tier, and the class-validity test accepts it.
- `tiers.json`: six `never` rows became `manage`, one row added (`remote.pair.pending`).

### 2026-11-17, near side written

`machines.rs` `rpc`: seven arms, `call_through` for the four reads and `write_through` for the three changes, each at `operator` on the link. `main.rs` `dispatch`: one arm for `remote.pair.pending` and seven for the new `machine.*` methods.

### 2026-11-17, checks so far

- `cargo check -p retinue-daemon --all-targets` (shared warm target `backend-work-from-the-owner-assigned\target`): clean. The daemon's build script was blocked once (os error 4551); deleting `target\debug\build\retinue-daemon-*` and retrying let it run, as `lessons.md` describes.
- `node scripts/check-format.mjs` and `node scripts/check-architecture.mjs`: pass.
- `npm run typecheck`: pass.
- `npx vitest run src/remote/remoteTiers.test.ts src/remote/remoteTiers.parity.test.ts`: 39 and 7 tests pass, so the new dispatch arms and the new rows agree, and the four new golden cases match in both languages.

### 2026-11-17, the tests, run

Every one of these ran on this machine against a binary linked after the last edit:

| Run | Result |
|---|---|
| `cargo test -p retinue-daemon --bin retinued -- remote::` | **148 passed, 0 failed** (24.7 s) |
| `cargo test -p retinue-daemon --bin retinued -- machines_tests::` | **70 passed, 0 failed** (741 s) |
| `cargo test -p retinue-daemon --bin retinued -- manage` | **33 passed, 0 failed**, including all four new far-side tests and the three new near-side ones |
| `... -- a_linked_machine_manages` alone | **1 passed** (1.6 s) |
| `... -- device_management_passes` alone | **1 passed** (0.2 s) |
| `... -- pairing::tests` | **14 passed, 0 failed**, including the new `the_waiting_request_is_read_without_its_identity` |
| `cargo clippy -p retinue-daemon -p retinue-storage --all-targets -- -D warnings` | **clean** |
| `node scripts/check-format.mjs`, `node scripts/check-architecture.mjs` | **pass** (1480 files, boundaries) |
| `npm run typecheck` | **pass** |
| `npx vitest run src/remote/remoteTiers.test.ts src/remote/remoteTiers.parity.test.ts` | **39 and 7 passed** |

Two things cost time here, both already in `lessons.md` and both handled:

- One test failed on the first run: `device_management_passes_through_to_the_far_gateway` linked at `operator`, but the fake far gateway answered `/api/me` with `"chat"`, and `refresh_one` lowers a link to the device's own tier after a write — so the second write was refused as `tier_required`. The fake now takes its tier (`far_gateway_at`), which is what the far side really answers. The behaviour was right; the fixture lied.
- `clippy -D warnings` caught `too_many_arguments` on `PairingState::submit` (8/7) after the extra parameter. It carries `#[allow(clippy::too_many_arguments)]` with a one-line reason, the same as ten other seam functions in this crate (`machine_client.rs:285`, `media.rs`, `team.rs`, `repository.rs`, `fanout.rs`); a request struct would have churned ten test call sites for no behaviour.
- After that edit Smart App Control refused the already-linked test binary (os error 4551, and the copy in `C:\tmp` was refused too — the block is by content). Touching `pairing.rs` and rerunning relinked a new binary, which ran; every run in the table above is from the binary after the last edit.
- The full-daemon run was not attempted: on this PC the shared target is contended by several lanes (one build waited 40 minutes for the artifact lock), so each gate was run on the crate this change touches (`retinue-daemon`, `retinue-storage`).

### 2026-11-17, what is finished

Everything in the brief, with these limits:

- **No event** for a waiting pairing request reaching a machine; the screen polls `machine.pair.pending`. Adding one means a new `allow` event type crossing to devices, which is a review decision of its own.
- **`docs/remote/PROTOCOL.md`'s tier table is not regenerated.** It already lagged `tiers.json` by about 200 rows before this change, and `node scripts/print-remote-tiers.mjs` needs a TS loader this worktree lacks. Regenerating it belongs in one commit of its own, or the diff hides this change among stale rows.
- **The near side does not tell the window which far device is the link's own row.** `machine.list`'s row carries `deviceId` in the stored link but `row_of` does not pass it out (it is a device id, deliberately kept on this side). A screen can find its own row by `kind: "machine"` only if the far machine has exactly one machine device; the clean fix is to add `ownDeviceId` to `machine.list` rows, which is piece 2's row shape, not this piece's.
- **This worktree has a `node_modules` junction** to the main checkout (as `backend-work-from-the-owner-assigned` does), needed for `npm run typecheck` and Vitest. It is git-ignored; remove it as a link before deleting the worktree.

## Files touched

| File | What changed |
|---|---|
| `crates/retinue-daemon/src/remote/pairing.rs` | `Pending.device_id`; `submit` takes the asking device; `waiting()`, `waiting_identity()`; two tests |
| `crates/retinue-daemon/src/remote/web.rs` | `pair_post` records the asking device; `rpc_post` gates `manage` (`manage_refusal`) and stamps `actor`; `audit_kind` writes no second line for `manage` |
| `crates/retinue-daemon/src/remote/mod.rs` | `remote.pair.pending`; `ACTOR_KEY`, `Actor`, `actor_of`; the four actor-aware activity lines |
| `crates/retinue-daemon/src/remote/policy.rs` | `MethodClass::Manage` (parsed, operator-tier) |
| `crates/retinue-daemon/src/machines.rs` | the seven pass-throughs |
| `crates/retinue-daemon/src/main.rs` | `remote.pair.pending` and the seven `machine.*` dispatch arms |
| `crates/retinue-daemon/src/remote/tests.rs` | `seed_device_of_kind`, the reworked tier-table test, four new gateway tests |
| `crates/retinue-daemon/src/machines_tests.rs` | `far_gateway_at`, three new tests |
| `apps/desktop/src/remote/tiers.json` | six rows `never` → `manage`, one row added |
| `apps/desktop/src/remote/remoteTiers.ts` | the `manage` class |
| `apps/desktop/src/remote/remoteTiers.test.ts` | the class rule |
| `apps/desktop/src/remote/fixtures/policy-goldens.json` | four golden cases |
| `docs/remote/THREAT_MODEL.md` | section 16 gains the manage table |
| `docs/reference/invariants.md` | one rule with its files and tests |
| `.claude/skills/retinue-development/references/lessons.md`, `codebase-map.md` | the lesson and the map entry |

No line endings were rewritten; `node scripts/check-format.mjs` confirms it. Nothing is committed.


