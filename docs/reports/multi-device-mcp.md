# The Retinue MCP on the linked machines: diagnose and test multi-device from outside the window

> **Lane:** Ada, backend. Worktree `first-step-before-reading-anything-your`, `main` fast-forwarded to `int/wave3` (`a0d3ffa`) before anything was read. Nothing is committed.
> **Sources:** the brief from the Director (18 November 2026); `docs/reports/multi-device-{live,linux,manage,phone,projects}.md`; `crates/retinue-daemon/src/external.rs` (the MCP), `machines.rs` (the `machine.*` RPCs), `machine_stream.rs` (the live socket), `remote/policy.rs` (redaction).
> **This file is the contract.** The Director builds nothing from it (no screen changes), but a shape here is what a bot sees through the MCP, so it changes only with a dated line under **Changes**.
>
> **Note for the Director:** `docs/plans/multi-device-plan.md` (section "Diagnose and test through the Retinue MCP") was named in the brief but is not in any branch of this repository (`git log --all -- docs/plans/multi-device-plan.md` is empty). This lane worked from the brief's list and the five reports; if the plan says something else, this is the file to correct.

## What this adds, in one paragraph

Seventeen tools in the Retinue MCP's catalog (`external.rs` `catalog()`), so an outside session (the Director, or any agent the owner points at Retinue) can list the linked machines, read their conversations, start and drive work there, answer an approval, use the shared project folder, read the far Remote/devices state, and ask one diagnostic report per link — all without the window. Every tool is a **thin wrapper over an existing `machine.*` RPC through `dispatch`**, so it can never reach more than the window can: the request goes out with the same link, the same token, the same tier checks, and the same far-gateway redaction.

## Rules this lane implements, each with a test

| Rule | Where | Pinned by |
|---|---|---|
| The MCP's own level applies on top of the link's tier: reads at Viewer, start/message/steer/stop at Chat, approvals and projects and devices at Operator. A tool above the level is absent from `tools/list` and refused if called. | `external.rs` `catalog()` / `tool_list()` / `call_tool()`; the link's tier is checked by `machines.rs` `require_tier` | `machine_mcp_tests.rs::the_machine_tools_appear_at_the_level_that_may_call_them` |
| `vault.*`, `owner_*` and `computer_*` are never reachable, and no token, cookie, fingerprint, password, URL or environment value appears in any reply. | the tools call only `machine.*` methods; the replies are allowlisted field by field (`list_machines`, `machine_remote_status`) | `machine_mcp_tests.rs::the_machine_tools_appear_at_the_level_that_may_call_them`, `list_machines_and_diagnose_read_a_live_link` |
| Every tool goes through `dispatch` with the method name the window uses. | `machine_mcp.rs` (`dispatch(st, "machine.…", …)`) | every test below; the tools are `machine.*` calls end to end |
| Message text never reaches a log line. | `external.rs` `record()` writes the client, the tool and a bot name only; the new tools pass no text to it | `machine_mcp_tests.rs::no_machine_reply_carries_a_token_an_address_or_a_fingerprint` (scans the activity ledger), and the same check for a sent message in `start_and_message_on_a_machine_and_wait_return_the_reply` |
| No probe may change anything on either machine. | `diagnose_machines` calls only `machine.probe`, `machine.stream.status`, `machine.project.browse`, `machine.devices.list` and `machine.permissions` | `machine_mcp_tests.rs::diagnose_machines_reports_an_offline_link_plainly`, and the live test above |

## Contract

Levels are the MCP's (`external.rs` `Tier`); each is also subject to the link's own tier, so a Viewer tool on a link at Viewer can still be refused by the far machine (its sentence is passed through unchanged). `…` is an opaque id.

### 1. `list_machines` — Viewer, no arguments

```jsonc
{ "machines": [ {
  "id": "…", "name": "Home PC",
  "state": "online",              // "online" | "offline" | "revoked"
  "detail": null,                 // the plain sentence when the link is away
  "kind": "windows",              // "windows" | "linux" | "macos" | null (an older far daemon)
  "live": true,                   // the live socket
  "version": "0.1.0-beta.60",     // the far daemon's version, null before the first read
  "tier": "operator",             // this computer's tier on that machine
  "stream": { "connected": true, "lastSequence": 1234 },
  "bots": [ { "id": "…", "name": "Ada" } ]
} ] }
```

- Built from `machine.list` plus one `machine.stream.status` per link. `url`, `fingerprint`, `auth` and `pairedAt` are **not** in the reply.
- Bots are reduced to an id and a name.
- Errors: none of its own.

### 2. `machine_conversations` — Viewer, `{ machineId }`

```jsonc
{ "machineId": "…", "machineName": "Home PC",
  "state": "offline", "stale": true,        // stale is true whenever state is not "online"
  "conversations": [ {
    "id": "…", "title": "Invoice parser", "agentId": "…", "agentName": "Ada",
    "projectName": "Retinue", "state": "working",
    "updatedAt": "2026-11-17T09:14:02.114Z", "archived": false,
    "startedFrom": { "deviceId": "…", "deviceKind": "machine", "machineName": "Laptop" },  // only when present
    "machineId": "…", "machineName": "Home PC", "kind": "windows"                          // the live lane's stamp
  } ] }
```

- The rows are `machine.sessions` unchanged (the last read this computer holds). An offline machine keeps its last rows, and `stale: true` says so.
- `startedFrom` is new on the row: `docs/reports/multi-device-live.md` §7 puts `link.startedFrom` on the far `session.list` row, and `machines.rs` `sessions_from` now copies it out (see **Changes**).
- Errors: `not_found` ("That machine isn't linked.") for an unknown `machineId`.

### 3. `read_machine_conversation` — Viewer, `{ machineId, sessionId, beforeMessageId?, limit? }`

One far conversation as a page, exactly `read_conversation`'s shape plus the machine:

```jsonc
{ "machineId": "…", "sessionId": "…", "state": "idle", "title": "Invoice parser",
  "messages": [ {
    "id": "…", "role": "user" | "assistant" | "tool" | "error", "text": "…",
    "createdAt": "…", "kind": "message" | "tool" | "error",
    "command": "npm test",     // a tool step, when the link is a machine and the far daemon sent the work log
    "output": "ok 3 tests",    // the same
    "title": "Ran npm test"    // a tool step's own title
  } ],
  "count": 50, "totalCount": 214, "truncated": true,
  "firstId": "…", "lastId": "…", "bytes": 41231, "capBytes": 131072 }
```

- The page is `external.rs`'s own `conversation_page`: newest `limit` items (default 50, at most 200), or the ones before `beforeMessageId`; never both cursors; cut to 128 KiB with at least one item.
- A tool step is one item. A `machine` link gets the far gateway's work log (title, `detail.command`, `detail.output`), because `remote/policy.rs` keeps those for device kind `machine`; a phone-kind link never does, and then the item has only `text`.
- `state` is the far conversation's own state word.
- Errors: the far side's own code and sentence (`not_found`, `provider_unavailable`).

### 4. Chat: `start_on_machine`, `message_on_machine`, `steer_on_machine`, `stop_on_machine`

```jsonc
// start_on_machine { machineId, botId, text, projectId? }
{ "machineId": "…", "sessionId": "…", "started": true,
  "message": { "turnId": "…", "accepted": true, "state": "working" } }

// message_on_machine { machineId, sessionId, text }
{ "machineId": "…", "sessionId": "…", "result": { …the far session.prompt answer… } }

// steer_on_machine { machineId, sessionId, text }
{ "machineId": "…", "sessionId": "…", "result": { …the far session.steer answer… } }

// stop_on_machine { machineId, sessionId }
{ "machineId": "…", "sessionId": "…", "result": { …the far session.cancel answer… } }
```

- `start_on_machine` is `machine.start` (the far `agent.conversation.create`) **then** `machine.prompt` with `text`. It answers as soon as the far side accepted the message; the reply comes from `wait_on_machine`. If the conversation was made but the message failed, the error says so and names no text.
- `text` is 1 to 100,000 characters, as `send_message`.
- Errors: `invalid_argument`, `not_found`, `provider_unavailable`, `tier_required`, `conflict` (a conversation that moved), each in the far side's or `machines.rs`'s own words.

### 5. `wait_on_machine` — Viewer, `{ machineId, sessionId, timeoutSecs? }`

```jsonc
{ "machineId": "…", "sessionId": "…", "done": true, "state": "idle", "reply": "…",
  "error": { "message": "…", "detail": "…" } }          // only when the turn failed
{ "done": false, "state": "waiting_permission", "approvalPending": true, "approvals": [ …as machine_approvals… ] }
{ "done": false, "state": "working", "timedOut": true }
```

- Waits at most `timeoutSecs` (default 120, 600 at most).
- It re-reads the far conversation (`machine.conversation`) when a `machine.session.changed` event for that conversation arrives, and otherwise no more often than every 2 seconds; never twice within 500 ms even on a burst of events.
- `reply` is the newest assistant message's text, capped at 60,000 characters. A failed far turn reports its plain message and Details line, as `wait_for_turn` does.
- `state` is the far conversation's own word when the wait ended: `completed` after a turn that finished, `idle` for one that never started, `error` when the turn failed. Only `starting`, `working`, `waiting_permission` and `cancelling` are busy, so any other word ends the wait.
- It returns early when the conversation is `waiting_permission` and a card is waiting, so a caller can answer it instead of waiting for a timeout.

### 6. `machine_approvals` — Viewer, `{ machineId, sessionId? }`; `answer_machine_approval` — Operator, `{ machineId, permissionId, choice }`

```jsonc
// machine_approvals
{ "machineId": "…", "permissions": [ {
  "id": "…", "sessionId": "…", "kind": "shell", "title": "Run the tests",
  "detail": "npm test", "choices": ["allow", "deny"]
} ] }

// answer_machine_approval { machineId, permissionId, choice: "allow_once" | "deny" }
{ "machineId": "…", "permissionId": "…", "choice": "allow_once",
  "answeredWith": "allow",            // the card's own word, when it has one of the accepted set
  "result": { …the far permission.reply answer… } }
```

- `machine_approvals` is `machine.permissions` with each field allowlisted and `title` capped at 300 characters, `detail` at 2,000. No provider payload, no rule key.
- `answer_machine_approval` maps `allow_once` to the first of `allow_once, allow, accept, yes` the card offers and `deny` to the first of `deny, reject, decline, no, deny_once, deny_always`, as `answer_approval` does for this machine's own cards. The far side must also have **approvals from remote** on; its refusal sentence is passed through.

### 7. `machine_folders` — Viewer, `{ machineId, path? }`; `machine_add_project` — Operator, `{ machineId, path, name? }`; `machine_clone` — Operator, `{ machineId, url, folder?, name? }`; `machine_clone_status` — Viewer, `{ machineId, jobId }`

Each passes through, unchanged, to the `machine.*` method the window uses:

| Tool | Far call | Reply |
|---|---|---|
| `machine_folders` | `machine.project.browse {machineId, path?}` | `{base, path, parent, entries: [{name, kind}], truncated}` — all inside the far shared folder |
| `machine_add_project` | `machine.project.create {machineId, path, name?}` | `{project: {id, name, path}}` |
| `machine_clone` | `machine.project.clone {machineId, url, parent, name?, confirm: true}` | `{job: {id, url, target, state, detail, project}}` |
| `machine_clone_status` | `machine.clone.status {machineId, jobId}` | the far `{job}` |

- `machine_folders` is a Viewer tool because it only reads, but the far side requires **operator on the link** (class `workspace`) and its project-sharing switch on. With either missing the refusal is the far sentence, unchanged: "Projects from other machines are off on this computer. Turn them on in Settings › Machines."
- `machine_clone`'s `folder` is the parent folder; when it is omitted the tool reads the far base folder with `machine.project.browse` and clones there. It sends `confirm: true` because the Operator level is the owner's own grant for this tool; the far side still confines the parent to its shared folder (the gateway stamps the call as a device's).
- Errors: the far side's own code and sentence.

### 8. `machine_devices` — Operator, `{ machineId }`; `machine_remote_status` — Operator, `{ machineId }`

```jsonc
// machine_devices
{ "machineId": "…", "devices": [ { "id", "name", "kind", "tier", "platform", "online", "pairedAt", "lastSeenAt", … } ],
  "pending": { "pairingId", "name", "kind", "platform", "askedAt" } | null }

// machine_remote_status
{ "machineId": "…", "enabled": true, "method": "native", "port": 48621, "deviceCount": 3, "connectedCount": 1, "hostName": "HOME-PC" }
```

- `machine_devices` is `machine.devices.list` plus `machine.pair.pending`. Device rows never carry a token hash (storage never returns it).
- `machine_remote_status` is `machine.remote.status` **reduced** to the allowlisted fields above: the far certificate fingerprint, its URLs and its `native` block stay there.
- **Not part of this lane:** approving, renaming or revoking a device from the MCP. Those stay in the app (and in the paired machine's own Remote screen); the MCP reads them only.

### 9. `diagnose_machines` — Viewer, no arguments

```jsonc
{ "machines": [ {
  "id": "…", "name": "Home PC", "state": "online", "detail": null,
  "kind": "windows", "version": "0.1.0-beta.60", "tier": "operator",
  "reachable": true, "certificateTrusted": false, "pinnedCertificate": true, "isRetinue": true,
  "stream": { "connected": true, "lastSequence": 1234 },
  "probes": [
    { "what": "Browsing its folders", "ok": false, "code": "forbidden_remote",
      "message": "Projects from other machines are off on this computer. Turn them on in Settings › Machines." },
    { "what": "Listing the paired devices", "ok": true, "count": 2 },
    { "what": "Reading the pending approvals", "ok": true, "count": 0 }
  ],
  "summary": "Home PC is linked and live."
} ] }
```

- `reachable` is a `machine.probe` (`GET /api/ping`, no credential, no `Origin`) of the link's address; `certificateTrusted` is whether web PKI accepted the certificate, and `pinnedCertificate` says whether this link holds a fingerprint the owner compared. Neither the address nor the fingerprint is in the reply.
- The three probes are reads only, run when the link is `online`: `machine.project.browse` (the far shared folder), `machine.devices.list` (a `manage` read, so its refusal shows a link below Operator) and `machine.permissions` (a plain read). Each reports either a count or the error code and the exact sentence the window would show.
- `summary` is one of: "`<name>` is linked and live.", "`<name>` answers but its live socket is not open.", "`<name>` can't be reached right now.", "`<name>` removed this computer's pairing. Pair it again."
- A revoked link is not pinged at all.

## Changes

- **2026-11-18 — `machine.sessions` rows gain `startedFrom`** (the far `link.startedFrom`, only when it is present). One additive field, so `machine_conversations` can say where a conversation was started without a far read per conversation. `docs/reports/multi-device-live.md` §2's row shape gains it; nothing that reads the row breaks.
- **2026-11-18 — `machine_remote_status` is an allowlisted subset** of `remote.status`, not the whole object: the far fingerprint, URLs and `native` block do not cross.
- **2026-11-18 — `machine_clone` sends `confirm: true`** itself; the Operator level is the grant.

## Running log

### 2026-11-18 — report written, then built

(see below)
