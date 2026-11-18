# Remote mode: threat model

> **What this is:** The threat model for Remote mode. The release-gate checklists in sections 8 and 11 are still owed.
> **Status:** current
> **Last updated:** 2026-10-22

**Status: design.** Written 15 October 2026; amended 16 October 2026 for the owner's choice of
Tailscale. Scope: the gateway, Tailscale serve, paired devices and the pairing flow. Out of
scope: the coding agents' own sandboxing, provider accounts, and Tailscale's own security.

> **Superseded rows.** The 15 October model covered a relay, Noise sessions, X25519 device keys
> and a direct LAN listener. Those parts are gone. Rows that only made sense for them (relay
> reading or correlating traffic, browser key theft, recorded-frame replay, LAN handshakes,
> browser tickets) are marked superseded below. New rows T28–T33 cover Tailscale.

## 1. The core fact

A message sent from a remote device is a prompt to a coding agent on the host. Within the
host's approval mode, that prompt can read files, run commands and change the project.
**Remote access to prompts is remote code execution on the computer, bounded by approvals.**
Every control below exists because of that sentence, and the product says it in plain words.

## 2. Assets

| Asset | Why it matters |
|---|---|
| The host computer and the user's projects | A prompt can act on them. |
| Conversations and tool output | May contain code, secrets pasted by the user, internal paths. |
| Provider credentials (Claude, Codex, OpenCode auth files) | Read by the daemon locally; must never reach a device. |
| Model-provider API keys Retinue holds for Pi (16 November 2026) | Written by the owner at the desktop, kept in Credential Manager, given only to one Pi child; must never reach a device, a backup, a log, a prompt, a reply or any other child process. |
| The daemon capability token | Full authority over the loopback API. |
| Device cookies | Prove a paired device; whoever holds one acts at its tier. |
| The owner's Tailscale account | Decides which nodes can reach the host at all. |
| The host's `tailscale serve` configuration | May hold other people's handlers; must not be clobbered. |
| Approvals | The brake on unattended action. |

## 3. Actors

- **Owner** on the host and on paired devices.
- **Thief or finder** of a paired, unlocked device.
- **Other members of the tailnet**, or people a node is shared with.
- **Network attacker** outside the tailnet.
- **Other local user or process** on the host.
- **Malicious web page** the owner visits in the same browser as the PWA.
- **Attacker who takes over the owner's Tailscale account.**
- **Compromised provider output** (prompt injection in a repository or web page the agent reads).

## 4. Trust boundaries

1. Device <-> host: WireGuard inside the tailnet, managed by Tailscale. HTTPS with the tailnet
   certificate, terminated by `tailscaled` on the host.
2. `tailscaled` <-> gateway: plain HTTP on `127.0.0.1:<remote.port>`. Any local process can
   reach this port too, so nothing that arrives here is trusted for its source.
3. Gateway <-> daemon dispatch: in-process, trusted.
4. Daemon <-> coding agents: existing boundary, unchanged.
5. Host <-> other local processes: the daemon API stays capability-protected. The gateway is the
   one new loopback listener, and only while Remote is on.

## 5. Abuse cases and mitigations

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| T1 | Stolen unlocked phone sends prompts | Device removable from the host at once; removing the node from the tailnet also cuts it off; owner picks the tier at approval; quiet hours; idle expiry; activity list shows every prompt source | Until removed, the thief can send prompts that run within the approval mode. Document it; recommend "Ask first" mode. |
| T2 | Attacker approves a command from a stolen phone | Approvals from remote are **off by default**; the operator tier only acts while that switch is on | If the owner enables it and loses the phone, approvals are possible until removal. |
| T3 | *Superseded:* relay reads or alters traffic | There is no relay. WireGuard and HTTPS protect the link | — |
| T4 | Pairing code guessed or shoulder-surfed | 8 characters from a 32-symbol alphabet; 5-minute life; 5 wrong attempts burn it; only reachable from inside the tailnet; the owner must approve the device and its tier | A watcher on the tailnet who reads the code and submits first locks the offer; the owner sees an unexpected device and rejects it. |
| T5 | Remote client calls a dangerous method (settings, runtime profile, file revert, `remote.*`) | Host-side tier table in `tiers.json`; never-remote methods denied at every tier; parity test fails the build when a daemon method has no rule | A bug in the table; mitigated by the parity test and review. |
| T6 | Snapshot leaks secrets (env values, instructions, absolute paths) | `redactSnapshotForRemote` strips env values and instruction text, shortens paths, drops keys named like secrets | New fields added later; the redaction test uses an allowlist-minded fixture and must be extended with each new snapshot field. |
| T7 | Attachment used to read arbitrary files | Attachments are refused from remote in v1 | None while refused. |
| T8 | *Superseded:* device key theft from a browser | No device keys; the cookie replaces them (see T32) | — |
| T9 | Malicious web page drives the PWA | Cookie is `SameSite=Strict` and `HttpOnly`; a state-changing request and the WebSocket upgrade must carry an `Origin` equal to an accepted origin, and a safe `GET`/`HEAD` that carries one must match (a browser omits it same-origin); `Sec-Fetch-Site: cross-site`/`same-site` is refused; CSP `connect-src 'self'`, `frame-ancestors 'none'` | Browser vulnerabilities. |
| T10 | *Superseded:* replay of recorded Noise frames | TLS inside WireGuard; a recorded request is useless without the cookie | — |
| T11 | Denial of service on the host | Per-device rate limits; 256 KiB body cap; one event socket per device; reachable only from the tailnet | A paired device can still cost bandwidth until removed. |
| T12 | Prompt injection makes a bot exfiltrate through Remote | Remote adds no new tool or egress; the bot's capabilities and approval mode are unchanged | Existing agent risk, unchanged. |
| T13 | *Superseded:* another user connects to the direct LAN address | There is no LAN listener; see T28 for local processes | — |
| T14 | Audit log leaks content | Entries are one short templated sentence, never message text, paths or command lines | Template mistakes; covered by a unit test on every entry kind. |
| T15 | Host left serving while the owner thinks Remote is off | One master switch; status chip in the sidebar and tray; off stops the listener and removes our serve handler at once | None expected. |

## 6. Defaults that carry the model

1. Remote is off.
2. Approvals from remote are off globally.
3. The owner picks each device's tier at approval; the dialog preselects **Chat**.
4. Idle devices expire in 30 days.
5. Never Funnel, never `serve reset`, never a non-loopback bind.
6. No message text in logs, push payloads or the activity list.

## 7. Not mitigated by design

- A compromised host computer. Remote adds nothing here.
- A user who turns approvals from remote on, sets bypass mode and loses a phone. The product
  warns, and bypass mode is already a typed-confirmation setting.
- Data the user chooses to paste into a chat.
- Tailscale itself being broken. Retinue relies on WireGuard and the tailnet's access rules.

## 8. Review checklist before P1 ships

- [ ] Every daemon method has a tier rule (parity test green).
- [ ] The gateway calls internal dispatch only; the capability token never appears in a
      response, event, log or error.
- [ ] Snapshot and event redaction covered by tests with the real field set.
- [ ] Rate limits and size limits tested.
- [ ] Pairing: 5-minute expiry, 5 attempts then burned, offer locked by the first correct code,
      owner approval required; all tested.
- [ ] Activity entries tested for content-free templates.
- [ ] Independent review of the cookie, `Origin` and Tailscale-login checks and of the serve and
      Funnel handling.

## 9. Additions from the independent review (15 October 2026)

Source: [REVIEW_NOTES.md](REVIEW_NOTES.md). These rows continue the table in section 5.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| T16 | Two devices race for one pairing code | The first correct code locks the offer; later submissions are refused; the owner approves name, platform and Tailscale login before a record exists | A watcher who wins the race is caught by the owner's approval |
| T17 | *Superseded:* relay correlates devices and hosts | No relay. Tailscale's coordination server sees node metadata, as it does for every tailnet | Covered by T33 |
| T18 | Cloned device credential | One live event socket per device; a second closes the first and writes an activity line; Tailscale-login check when the header is present (see T32) | A clone used while the original is offline |
| T19 | Push counts reveal activity | Push payload is "check in" only, no counts; counts stay inside the tailnet | Existence of a push is itself a signal |
| T20 | Staged attachment ids guessed or reused, decompression bombs | Attachments are refused in v1 | Returns when staging is designed |
| T21 | Event payloads carry secrets (message bodies, tool output) | Event allowlist and per-event scrubbing; tool detail off by default; message text is the conversation and is shown | A secret pasted into a chat is visible to the owner's own devices by design |
| T22 | Snapshot fields outside the redactor (paths, args, attachments) | The redactor's field policy in `tiers.json` and `policy.rs`; results of other reads go through `redactResultForRemote`; golden fixtures run in both Rust and Vitest | New fields added later must be added to the policy and its test |
| T23 | *Superseded:* browser WebSocket cannot carry headers | The same-origin cookie travels on the upgrade; `Origin` is checked | — |
| T24 | CSRF or framing of the PWA | `SameSite=Strict` cookie, `Origin` check, `frame-ancestors 'none'`, `connect-src 'self'`, approvals need a user gesture inside the PWA | Browser bugs |
| T25 | Quiet hours bypassed through cancel, archive or queue send | Quiet hours are a hard read-only override applied before dispatch | None expected |
| T26 | Revocation leaves a session open | Revoking marks the device revoked and closes the event socket on the host; the next request fails with `device_revoked` | None expected |
| T27 | Viewer forces provider polling or reads host config | `runtime.models refresh` refused remotely; executable paths and arguments redacted | None expected |

### Corrections to earlier rows

- **T5:** the tier table is an executable specification and a parity test of method **names**.
  Enforcement does not exist until the gateway applies it. The risk stays open until then.
- **T9:** the content security policy is a requirement, not a present fact.
- **T14:** a unit test over every activity-entry template is a requirement for P1; none exists.
- **Idle expiry:** the "Never" option removes the only automatic containment for a forgotten
  device; the UI keeps 30 days as the default and warns on "Never".
- **Redaction is not defence in depth.** It is the only layer between the host and a device for
  fields the daemon keeps.

## 10. Tailscale threats (16 October 2026)

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| T28 | A local process spoofs `tailscaled`: it connects straight to `127.0.0.1:<remote.port>` and forges `Tailscale-User-Login` or `Origin` | The cookie is required on every `/api/*` request; the login header and `Origin` are extra checks, never proof on their own; pairing still needs the live code and the owner's approval | A local process that can read the browser's cookie store already owns the user's account |
| T29 | Other tailnet members, or people the node is shared with, reach the serve address | They need a pairing code and the owner's approval; the approval shows their Tailscale login; the Remote page says that everyone on the tailnet can reach the address and links to Tailscale's access rules | A member who shoulder-surfs a code still has to get past the owner's approval |
| T30 | Accidental Funnel exposes the gateway to the internet | Retinue never runs `tailscale funnel`; it checks Funnel is off for its port at start and on each status refresh, and stops serving if Funnel appears | The owner turns Funnel on by hand between checks; the cookie and `Origin` checks still apply |
| T31 | Retinue clobbers the user's own `tailscale serve` setup | Never `serve reset`; add and remove only our own handler; fall back from 443 to 8443 when 443 is taken; refuse rather than overwrite | Tailscale CLI changes; covered by tests that assert the exact commands issued |
| T32 | Cookie theft (malware, a shared browser profile, a backup of browser data) | `HttpOnly`, `Secure`, `SameSite=Strict`, `__Host-` prefix; only its SHA-256 is stored; login check when the header is present; one live socket per device; idle expiry; one-click revoke | A stolen cookie used from another node with no login header, until revoked |
| T33 | The owner's Tailscale account is taken over | An attacker can add nodes, but still needs a pairing code and the owner's approval on the desktop; existing cookies stay bound to their recorded login; the Remote page recommends two-factor sign-in for Tailscale | An attacker who also controls a paired device acts at its tier |

## 11. Checklist additions for P1

- [ ] The gateway applies `authorizeRemoteCall`, `redactResultForRemote` and the event allowlist
      (Rust ports, same `tiers.json`); a test proves no code path returns raw daemon output to a
      device.
- [ ] Pairing offer locked by the first correct code; later submissions refused; owner approval
      implemented.
- [ ] Cookie attributes, the `Origin`/`Sec-Fetch-Site` check and the Tailscale-login check tested,
      including a browser-like `GET` with no `Origin`, a cross-site `GET`/`POST` refused, and a
      request with a forged login header and no cookie.
- [ ] Tests prove the gateway never issues `tailscale serve reset` or `tailscale funnel`, and
      that it refuses to start when Funnel is on.
- [ ] The listener binds `127.0.0.1` only (test).
- [ ] Static server rejects paths outside its manifest, including traversal.
- [ ] PWA content security policy written and tested.
- [ ] One live event socket per device enforced.
- [ ] Activity entry templates covered by a test.
- [ ] Tailscale status JSON never logged.

## 12. Native (Direct) threats: a port on the public internet (17 October 2026)

Native is the second Remote method ([AGENTS.md](../../AGENTS.md), amendment of 17 October 2026,
later). While it is on, the gateway's own listener binds the host's interfaces on
`remote.nativePort` (default 48621), serves HTTPS only with a certificate Retinue generates, and
may be reachable from the whole internet once the owner forwards the port (by hand or through
UPnP with consent). Everything in §4–§11 still applies; this section covers what is new. Facts
below come from the lane reports ([FIX35_C1](../history/fix-35/c1-report.md), [FIX35_C2](../history/fix-35/c2-report.md))
and the Director's review; none of it has been tested against a live router, a real phone on
mobile data or an outside attacker.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| N1 | Password guessing from the internet | Argon2id hash only; a wrong username still runs one verify against a dummy hash (no timing oracle); 5 failures per source in 15 min → that source gets `429` for 15 min; 30 failures across all sources in 1 h → every sign-in pauses for 30 min and the desktop shows it; a content-free activity line when a limit starts (not one per guess, so the 500-row log cannot be flooded) | A slow, distributed guesser below the global limit against a weak password. The UI requires 12+ characters, not the username |
| N2 | A correct password from a stranger's device | A new device still needs the owner's approval on the desktop, with the tier picked there (Chat preselected); approvals from remote stay off by default | An owner who approves without reading the request |
| N3 | Flooding unauthenticated routes (`/api/login`, `/api/pair`, `/api/ping`, static files) | Per-source token bucket, 30 requests / 10 s with burst 60, before any work | A botnet spread across many addresses; the daemon is not a hardened public web server |
| N4 | Man in the middle on first contact | TLS is mandatory (plain HTTP refused); the browser warns once because the certificate is self-made, and Settings › Remote shows the SHA-256 fingerprint to compare | An owner who clicks through the warning without comparing the fingerprint on a hostile network |
| N5 | Theft of the TLS key | The key lives in the data folder with user-only access (`icacls /inheritance:r /grant:r <user>:F` on Windows, mode 0600 elsewhere); "New certificate" replaces it and changes the fingerprint | Malware running as the user can read it, and the cookie store too |
| N6 | Cross-site use of a device cookie, or a hostile name pointed at the host (DNS rebinding) | `Origin` (when sent; a safe `GET` may omit it) and `Host` must name the certificate's own addresses; a Tailscale origin is refused under Native and the reverse; cookies keep `Secure`, `HttpOnly`, `SameSite=Strict` | None known beyond §9 |
| N7 | UPnP opens more than Retinue asked for, or leaves a mapping behind | Only with the owner's consent; maps only `remote.nativePort` over TCP; removed on disable and at daemon shutdown; the lease is renewed while on | A crash between mapping and removal leaves the forward until the lease ends; a router with a broken UPnP stack |
| N8 | Firewall changes without the owner knowing | One inbound rule, `Retinue-Remote`, for the Native port only, added and removed only through an elevated prompt the owner accepts; no console window | The owner widens the rule by hand |
| N9 | Retinue contacting third parties (IP echo, relays, DNS) | None: the external address comes from the router (UPnP) or the owner; carrier-NAT and private addresses are detected locally; the self-test is the owner opening `/api/ping` from a phone | — |
| N10 | `/api/ping` as a probe | Answers only a fixed, content-free marker, `{"ok":true,"app":"retinue","role":"remote-gateway"}` (FIX39 P3; nothing about the host, owner or account); records "reached from" as a public address or "this network"; rate-limited like other unauthenticated routes | Anyone who can reach the port learns that something answers there, which the sign-in page shows anyway |
| N11 | A signed-out device learns which method is on | `/api/me` refusals name `native` or `tailscale` and nothing else (no host name, no device data) | The method is visible from the address anyway |
| N12 | Test runs open firewall prompts or bind the internet | Under `#[cfg(test)]` the listener binds `127.0.0.1` only; no test runs real `netsh`, PowerShell or UPnP discovery | — |

## 13. Checklist additions for Native (owed before a non-beta release)

Implementer-tested means the lane's own unit tests, re-run by the Director on `main`; nothing
here is independently reviewed yet.

- [x] Native refuses plain HTTP; a non-TLS client gets nothing usable (implementer-tested).
- [x] Argon2id verify with a dummy hash for unknown usernames; uniform `login_invalid` (implementer-tested).
- [x] Per-source lockout and global pause, with one activity line each (implementer-tested).
- [x] `Origin`/`Host` checks per method (implementer-tested).
- [x] UPnP mapping removed on disable and shutdown, against a fake gateway (implementer-tested).
- [x] Firewall add/remove argv asserted against fakes; one rule name, one prompt (implementer-tested).
- [x] Tests bind loopback only (implementer-tested; the owner's firewall prompts stopping is not yet observed).
- [ ] An independent review of the TLS listener, sign-in, lockouts and `Origin`/`Host` handling.
- [ ] A live test from a phone on mobile data: port forward, certificate warning and fingerprint check, sign-in, approval.
- [ ] A live UPnP test on a real router, including that the mapping is gone after Remote turns off.
- [ ] A check that the key file's permissions are applied on a real Windows install.

## 14. FIX39 changes (18 October 2026)

Implementer-tested means the package's own unit, mounted and fake-process tests. Nothing in this section is independently reviewed, and nothing was exercised on a real tailnet, router or phone.

### 14.1 Remote

- **Origin: no change.** The FIX39 packages did not alter the `Origin` rule in [PROTOCOL.md](PROTOCOL.md) §4.1. The rule that safe reads may omit `Origin` comes from FIX38 and still stands.
- **Tool steps (P2).** Over Remote, a tool step's `title` is replaced by a generic "Tool" unless the device is allowed tool output. `label`, `pattern`, `url`, `path` and `command` are withheld by default. Titles such as "Ran cargo test C:/…" can carry paths and command text. The shared policy goldens prove this in both Rust and TypeScript.
- **Ping marker (P3).** `GET /api/ping` answers a fixed marker that names the app and its role and nothing else. It lets one Retinue copy recognise another on the same port.
- **Remote discard of a task branch (P10, finding 6, not yet reviewed).** `proposal.decide` with `discard: true` on a `merge` proposal removes a task's copy and branch. The operator tier may call `proposal.decide` when approvals from remote are on, so a paired operator device can discard a task branch. The discard is bounded to that repository's own `<repo>-retinue` copy and `retinue/*` branch, and it is refused while the task is still working. Whether a phone should be able to discard at all is an owner decision.
- **One copy only (P3).** A second copy of Retinue that already serves Remote is refused, not shared. The daemon exits when its app exits (a parent watchdog), so a stale process cannot keep a port. Single-instance focus was not exercised live.
- **Wide layout (P3).** The wide browser layout is a presentation change. It does not change what any device may call.

### 14.2 Local diagnostics (P12)

The export is local. It is written only after the owner picks a path in the native save dialog. Nothing is uploaded. No paired device can call `diagnostics.preview` or `diagnostics.export`; both are `never` remote.

**The logs never contain:** message, prompt, reply, streamed or thinking text; recap or summary text; command text or output; file contents or diffs; credentials, tokens, passwords, cookies (including Remote cookies), capability values or provider sign-in data; environment values; or Tailscale status JSON.

**The logs do contain:** IDs, error classes, provider names, versions, timings and exit codes. The daemon and the shell both write through a central redacting writer. A test scans every log call site and fails if one logs a field named text, prompt, content, message, command, secret, token, cookie, password or credential. Logs are daily files, 14 kept, at most about 20 MB in total.

**The export zip contains:**
- `README.txt`, saying what is inside;
- `environment.json`: app and protocol versions, OS, architecture and OS version, coding-agent names and versions. No executable paths;
- `settings.json`: secrets removed; `profile.about` and every `internal.*` key omitted;
- `events.json`: the last 200 app events, through an allowlist of identifiers, states, error classes, providers, versions, timings, counters and flags. Questions, titles, inputs, content and paths are dropped;
- the recent log files, redacted again on the way in.

**Known limits.** Redaction is pattern-based, so a secret in an unusual shape could pass it. `remote.native.account.username` is kept, as a label that says which account is configured. The zip is not encrypted, and sending it anywhere is the owner's decision.

## 15. Task board and bot deletion (22 October 2026)

The task board (`task.create`, `task.list`, `task.get`, `task.update`, `task.dispatch`, `task.unassign`, `task.reorder`, `task.cancel`, `task.stop`, `task.review`, `task.runs`) and permanent bot deletion (`agent.purge`) are `never` for a paired device, at every tier (`apps/desktop/src/remote/tiers.json`). Dispatching a task starts provider work on the host, and stopping or reviewing a task changes a run the owner is watching, so both stay at the desktop. Task events (`task.changed`, `task.run.changed`, `task.run.finished`) are dropped for remote devices and carry no task text in any case. This section adds no new threat rows: the threats above cover the surface, and a remote task list is not built.

## 16. Machine links and handoff (31 October 2026)

A machine link is one Retinue daemon acting as a paired device of another, through that machine's gateway. It adds no listener and no relay, and it uses the same pairing, owner approval and tiers as a phone. These are the new things it can do, and what stops each going wrong.

| Risk | What stops it |
|---|---|
| A stolen device token on disk | It sits in a user-only file under `<data>/machines/`, never in the database, a backup or a diagnostics bundle, and is read for one request. Removing the link on either side ends it. |
| Someone between the two machines (Native) | The link pins the SHA-256 of the certificate. The owner compares the fingerprint on both screens before a password is sent, and the certificate is checked again before sending. A later certificate change refuses the connection. |
| A redirect or system proxy seeing the device cookie | Redirects are off and the client uses no proxy. |
| An address that later answers as a different machine | Every read checks `machine.info`. A different machine id marks the link offline, and its answers aren't used. |
| A phone at the top tier moving a conversation | `handoff.*` needs tier `operator` **and** device kind `machine`. The kind comes from the pairing request, and the owner sees it when approving. |
| A handoff granting Full access | A created bot's mode is capped. No conversation mode travels, and neither do environment values, tokens, or host paths. |
| Both machines running one conversation | Only one can: a lease on the giving machine blocks new turns during the move, and the taking machine can't run its copy until the giving machine has confirmed. |
| Lost work in a move | A push is never forced and never to the base branch. A rejected push ends the move and nothing changes hands. Uncommitted files are only committed with the owner's consent. A fetch only fast-forwards, and refuses when it would lose local commits. |
| A branch name from another machine reaching git | It must start `retinue/`, pass `git check-ref-format --branch`, and never start with `-`. |

**`.retinue.json`.** A repository is someone else's input, and a setup script is code. The file does nothing until the owner trusts its exact bytes, a change stops it counting, it can never turn on Full access, and it's read from the project's own folder, never a conversation's copy. Only `project.sharedSettings.trust` and `.write` can record that trust, and it never travels in a backup or to a device.

**What this doesn't cover.** A link at `operator` can pull any eligible conversation's text from that machine, and push its task branch to that machine's `origin`. That's what the owner approves when choosing that tier. Reading another machine's conversations is the same trust as reading them from a phone: paired machines are the owner's own. The review owed before a non-beta release (sections 8, 11 and 13) now covers this section too.

### A headless Linux host (17 November 2026, multi-device piece 8)

The owner's VPS runs `retinued` under systemd with no window, with the gateway on and a console instead of Settings (`AGENTS.md`, 17 November, "Headless Linux daemon for the VPS"; `docs/research/multi-device/04-linux-headless.md`; `docs/operations/linux-host.md`).

| Risk | What stops it |
|---|---|
| A Linux host's bot secrets, vault values, webhook secrets, a Harbor token or a Pi key sitting in the database, a backup, a log or a diagnostics bundle | Off Windows they live in owner-only files under `<data>/secrets`: folder `0700`, file `0600`, written through a private temp file and a rename, so no reader sees a half-written value. The store's names are hex-encoded, so no name can leave the folder. Nothing here reaches the database, an event, a backup or a log; `secret.list` returns names and timestamps only. Tests: `secret_files` in `crates/retinue-daemon/src/secrets.rs`. |
| A local process running as the owner using the console file (`<data>/console.json`) to approve a device or turn Remote on | The file is user-only and exists only on a host started without a desktop shell; its token opens the `console.*` methods and **nothing else** — never `dispatch`, so never a conversation, a bot, a setting or a secret. It is not the daemon capability, which stays in memory. A device that steals it can do what someone already logged in as the owner can do from the host's terminal, which is the trust the host already carries. Tests: `console_tests.rs`. |
| A pairing decision made with no window — the wrong device approved, or a code seen by someone else | The code is printed to the host's terminal only: never the log, the database, `console.json` or an event, and it expires as on the desktop. Approval still goes through `remote.pair.confirm`, so it keeps the tier choice, the one-use token, the `paired` line in that host's activity list, and the refusal sentences. `retinued pair list` shows name, kind, platform and sign-in method before the decision, and an expired or already-decided request is not offered. |
| A Linux bot being offered the owner's-computer tools, which would fail inside a PowerShell script that is not there | `owner_computer::mcp_server_for` returns `None` off Windows, so the tools never appear, and a call that arrives anyway answers one sentence and lists nothing. |
| A pairing request saying `platform: "windows"` for a Linux host, misleading the owner's own approval dialog | The field is `std::env::consts::OS` of the asking device. |

**What this doesn't cover.** The daemon runs as the owner's own user, so it sees that user's git configuration, SSH keys and CLI sign-ins (open question 10 in `03-design-and-order.md`). A dedicated service account is the alternative and is not built. The Secret Service keyring path is not wired; on a Linux desktop there is no user for it yet.
### A machine manages another machine's devices (17 November 2026)

The owner opened seven `remote.*` methods to a linked machine at operator, plus one new read (`remote.pair.pending`). They are `manage` in `tiers.json`; the gateway additionally refuses any device whose kind is not `machine`, whatever its tier. This is a deliberate exception to "every `remote.*` method" and to "approvals from remote are off by default", so each risk is recorded here. Piece 7 of `docs/research/multi-device/03-design-and-order.md`; the contract is `docs/reports/multi-device-manage.md`.

| Risk | What stops it |
|---|---|
| A phone or browser at operator manages a machine's devices | The class alone never lets one through: `rpc_post` refuses every kind but `machine` with one fixed sentence, and writes a `denied` line in the host's list. Proven by `remote/tests.rs::a_phone_at_operator_cannot_reach_the_manage_subset` (phone and browser, every one of the seven) and `the_manage_subset_still_needs_operator`. |
| A machine silently raises its own tier, or removes its own pairing before anyone notices | `remote.devices.update` and `remote.devices.revoke` are refused when the id is the caller's own, checked in the gateway against the device it authenticated, not against a body field. |
| A machine approves another machine, or approves itself, and so makes a mesh or a duplicate identity | `remote.pair.confirm` refuses a waiting request whose kind is `machine`, and one whose asking device is the caller. The asking device is recorded with the request from its own cookie at `POST /api/pair` and never returned; nothing in the body can claim it. |
| One machine quietly changes another's security posture | Every change writes one line in the changed machine's own activity list naming the machine that did it (`<machine> removed <device> from another machine.`), after the same change the desktop would make; the four reads write nothing. An owner reading either computer sees who did it. |
| A machine reads more of another machine than the owner approved | `remote.status` is the same object the desktop's window gets, `remote.devices.list` the same rows, `remote.activity.list` the same content-free lines. No token, no credential, no host path, no message text, and never another machine's event stream. The device list carries the owner's own account label (`login`), which the owner already sees on both computers. |
| Quiet hours used as a shield: a stolen phone waits until night to be revoked | `manage` is not refused during quiet hours on purpose. It starts no work, and cutting off a lost device must not wait for the morning. |
| A machine's own window talks to the other machine directly | Unchanged: the window calls this daemon only. The seven near-side methods are `machine.*`, `never` remote, each needing operator on the link, and each a pass-through of the far side's own call with the far gateway still the authority. |
### 16.1 The phone through its host (17 November 2026)

The owner's decision of 17 November 2026: a phone pairs with **one** host and may watch, start and steer work on the machines that host is linked to. It may never pair, link, rename or remove machines, move a conversation, or manage another machine's devices. The `linked` class in `apps/desktop/src/remote/tiers.json` grants the eight methods; everything else in `machine.*` stays `never`.

| Risk | What stops it |
|---|---|
| A phone reaching every machine its host is linked to | It reaches only what the host's own link reaches, and only at the lower of the phone's tier and the link's tier, checked by one helper in `machines.rs` before any request. The far gateway still decides, in its own words. |
| A stolen phone turning one host into reach into four machines | One pairing, one cookie, one revocation: revoking the phone at its host ends all of it at once, and removing a link on the host ends that machine. The phone can never add, rename or remove a link. |
| One link driving another, forming a mesh by accident | The kind check is the daemon's: a `machine`-kind or `computer`-kind device calling a `linked` method is refused `forbidden_remote`, the mirror of `handoff.*` refusing a phone. |
| A phone lifting its own ceiling | The tier is stamped into the call by the gateway and always overwritten, so a client's own `remoteTier` parameter is discarded. A test proves it in both directions. |
| An approval answered two hops away without the far machine agreeing | `machine.permission.reply` needs `operator` on the phone **and** on the link, and the far machine's own approvals-from-remote switch. It refuses `approvals_disabled` in its own words, and this host cannot know that switch, so it never guesses. |
| A phone prompting a machine during the host's quiet hours | Quiet hours refuse the `linked` methods that drive work (`machine.start`, `.prompt`, `.steer`, `.cancel`, `.permission.reply`), in both languages' policy; reading the machines and their conversations still passes, as a read does. |
| A phone learning a machine's address or certificate fingerprint | `machine.list` for a paired device drops `url`, `fingerprint`, `auth` and `pairedAt`. The desktop still reads them. Nothing names a host path, and the far gateway's redaction applies to every read. |
| An event relay carrying content, or reaching another machine | `machine.session.changed` holds `{machineId, sessionId, revision}` and nothing else. Its policy is `linked`: the host sends it to a phone, tablet or browser socket and to nothing else, so a link's daemon never receives another link's changes. |
| A relayed event being acted on by the wrong device | It carries no title, text, tool name or path, so the only use is a re-read, which the far gateway redacts as for any device. |
| A prompt through a link leaving no trace at the host | Every phone call that starts, drives or stops work on another machine writes one content-free line in the host's activity list: the device's name, the method, and "prompt" or "approval". |

**What this doesn't cover.** The phone holds reach into every machine its host has linked for as long as it is paired. The host must be awake: when it is asleep, the phone sees nothing of the other machines, and says so rather than showing them as offline. A machine's name and its bots' names are visible to the phone at `viewer`, as they already are at the desktop.


## 17. Phone push (31 October 2026)

Push goes through the phone maker's push service (FCM, Mozilla, WNS, Apple), so a third party carries the notification envelope. It is off by default, and turning it on is the owner's confirmed choice at the computer (`remote.push.set` with `confirm: true`).

| Risk | What stops it |
|---|---|
| Message text, a tool name, a path or a project name reaching the push service | The payload is exactly `{title, sessionId}`. The title is "<bot> needs you" or "<bot>'s turn failed". Only that JSON is encrypted and sent; a test asserts that a permission card full of private text yields a body containing none of it. |
| The push service reading the notification | The body is RFC 8291 `aes128gcm`, encrypted to the device's own `p256dh` key with its `auth` secret, so only that browser can read it. |
| A forged sender to the push service | Every request carries an RFC 8292 VAPID token signed with the install's P-256 key, scoped to the push service's own origin. |
| A device stealing or redirecting pushes | A device can only change its own subscription: the gateway puts the calling device's id in the call, and the phone cannot supply it. One row per device; revoking the device deletes it. |
| A malicious endpoint (SSRF, exfiltration, redirects) | The endpoint must be `https`, with no user or password, on FCM, `*.push.services.mozilla.com`, `*.notify.windows.com` or `web.push.apple.com`. Anything else is `invalid_argument`. Sends use no proxy, follow no redirect and stop at ten seconds and 512 bytes of answer. |
| Spamming the owner's phone | At most one push per conversation per minute, no sends in quiet hours, and a send only for a permission card or a failed turn. |
| A dead subscription kept forever | A 404 or 410 from the push service deletes the row; any other failure is counted, recorded as the first `failed_at`, and logged only as fixed text. |
| The VAPID private key leaking | It is generated once into `<data>/remote-push-key.pem` with `apply_key_permissions`, never returned by any method, never in a backup (every `remote.*` key is refused) and never logged. `Default`/`Debug` print the public point only. |
| A phone subscribing while signed out | `push.*` are gateway-only calls with `tiers.json` rows, so a request without a paired device cookie is refused before it reaches the daemon. |

**What this doesn't cover.** The push service learns that this install sends a notification, when, and to which subscription, and it can drop or delay it. Retinue cannot know a notification was read. A bot's own name appears in the title, because the owner chose it.

## 18. Webhook triggers (31 October 2026)

A webhook starts an automation from a signed HTTP request (`POST /api/hook/<id>`). It exists only while Remote is on, and only the gateway serves it; the daemon API never sees it. **On Native the gateway is on the public internet, so the address is not a secret: assume anyone who can reach the port can reach this route.** Everything here assumes an untrusted caller who may know an automation's id (it is a UUID, but ids leak through screenshots, backups and logs of the owner's own doing) and does not know its signing secret.

The route sits outside the `Origin` check, because a sender is a machine with no browser origin, and it adds no cookie, so a malicious page gains nothing from it (it cannot set the signature header cross-origin without a preflight the gateway never allows). It is inside the per-source token bucket that covers every unauthenticated route.

| Risk | What stops it |
|---|---|
| Learning whether an id exists, or is enabled | Every failure answers exactly what an unknown id answers: the same `404` with the same body (`not_found`). A missing, malformed or wrong signature, an old timestamp, a replay, an unknown id and a disabled webhook are indistinguishable in status and body. |
| A timing oracle distinguishing "unknown id" from "wrong signature" | The HMAC is computed on every request that reaches the handler, with the automation's real secret when there is one and a fixed placeholder key otherwise, so the work is one HMAC either way. Two small differences remain (an enabled webhook costs one credential-store read, and a too-large body is refused before the HMAC); both are documented residual risk. |
| Forging a signature | HMAC-SHA-256 over `"<timestamp>." + body`, compared in constant time (`verify_slice`), with the 32-byte secret held in the Windows credential store through section 5. The secret is shown once when the webhook is enabled, and no RPC can return it again. |
| Replaying a captured request | `X-Retinue-Timestamp` must be within 300 seconds of the host's clock, and each accepted signature is remembered for 600 seconds; a second use is a failure. The cache is memory only, so a restart forgets it, and the 300-second window bounds what that forgets. |
| A stranger flooding the route to burn CPU or bury the activity log | The per-source token bucket refuses the flood before any work. A failed signature counts toward that source's own lockout (5 failures in 60 seconds → that source is shelved for 60 seconds). While shelved it costs one HMAC and no database write, no audit line and no failure accounting. It is **per source and never global**, so a stranger cannot pause anyone else, and a request whose signature is **valid is never refused by the webhook lockout**, so the real sender always runs. The worst case for a sender that shares a NAT with the attacker is 60 seconds of suppressed failure lines — not a refused request. |
| The run escaping an automation's fences | A webhook run goes through the existing automation path (`start_run`): the automation's own `plan`/`auto` mode (never `bypass`), its `maxRuntimeSecs`, single-flight, the global pause, and the concurrency limit. A paused, disabled or already-running automation answers `202` and does nothing but one content-free activity line, so a valid signature cannot tell "ran" from "skipped". |
| The request body injecting instructions | `includeBody` is off by default. When on, the body reaches the prompt only inside a delimited block as inert quoted data (`&`, `<` and `>` escaped, exactly as `knowledge.rs` `inert` does for a recap), preceded by one sentence saying it is data. The body is at most 64 KiB and is never stored, logged, put in an event or echoed in a response. |
| The secret leaking | It lives only in the credential store, under `Retinue/webhook-<automation id>`, and is shown once in the reply to `automation.webhook.enable`. It is absent from the automation DTO, every event, a backup (only the enabled flag and `includeBody` travel, if at all) and a diagnostics bundle. |
| A webhook enabled by accident | `automation.webhook.enable` needs `confirm: true`, and every `automation.webhook.*` method is `never` for a paired device, so only the desktop can turn one on or rotate its secret. |

**What this doesn't cover.** A sender that keeps the secret can start the automation whenever it likes, within the automation's own caps; revoking is `automation.webhook.rotate` or `disable`, and the owner's only signal is the run history. The gateway is not a hardened public web server: the token bucket and the lockout are memory-only, so a restart forgets them, and a distributed flood across many addresses is bounded only by the daemon's capacity (row N3). A webhook does not authenticate *who* sent it, only that someone held the secret. And the HMAC proves knowledge of the secret, not confidentiality: the body arrives in clear inside TLS, so anyone who breaks the certificate trust sees it.

## 19. Browsers and computer control (14 November 2026)

Owner amendment of 14 November 2026 ("browsers and computer control", `AGENTS.md`). Bots may drive the linked cloud computer (phase A), browse in Retinue's own profile on it (phase B) and, once the owner turns it on, control the owner's own Windows computer through a supervised `open-computer-use` child (phase C). Designs: `docs/research/owner-requests-2026-11-14.md` section 4 and `docs/research/owner-computer-driver-2026-11-14.md`.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| C1 | **The model receives a picture of the owner's screen** (a mail, a bank page, a password manager in view) | Phase C ships with no screenshot tool: the owner's screen is read as an accessibility tree only, password fields masked fail-closed, capped in nodes and characters. When screenshots come, each frame is shown to the owner first and only an approved frame is sent; never on a timer, a loop or every step. A deny-list (password managers, banking, mail, terminals) refuses reading those windows. Cloud computer and browser screenshots are media attachments, never inline bytes or log lines | The text tree itself can carry private content from an allowed window; the owner chose to allow that window for that conversation |
| C2 | **A bot acts as the owner on a signed-in site** (posts, pays, changes a setting, approves a sign-in) | Bots browse in Retinue's own Chromium profile on the cloud computer with no imported cookies; the owner's signed-in browser is a per-conversation opt-in. Every sign-in form, 2FA prompt, payment field or post/submit control pauses for the owner (`needs_owner`). On the owner's computer every grant needs the owner's approval card in that conversation, lasts at most 5 minutes (15 per turn, 60 s idle), is shown by a bar and the companion pill with Stop, and ends on lock, sleep or loss of foreground | Within a live grant, a click the bot is allowed to make is made; the brake is the visible indicator and Stop |
| C3 | Prompt injection from a web page or window text steers the bot to act | Same pauses as C2; the activity rows show each action class to the owner; grants never cover Full access (`bypass` refuses `owner_*`) | Injection that stays inside allowed actions on allowed windows |
| C4 | Unattended or remote use | Every `computer_*` input, `browser_*` and `owner_*` method is `never` in `tiers.json` and their events `drop`; refused in automations, unattended turns and teammate-woken turns | None expected |
| C5 | A compromised or swapped driver binary | `open-computer-use` is pinned by version and SHA-256, staged by a script that refuses a wrong hash, run as Retinue's supervised child with real-pointer movement off and Retinue's own limits forced around it | A malicious release that matches the pinned hash cannot happen; a flaw in the pinned version can |
| C6 | Typed secrets or screens end up in logs, backups, diagnostics or push | Activity rows are content-free (app name, action class, character counts); screenshots and accessibility trees are never persisted outside the turn's media | Template mistakes; covered by tests in each module |

**Release gate.** A release that changes these defaults (the switch's default, the screenshot rule, the grant limits, the pause list) needs the owner's live check before it ships.

## 20. Pi, with keys held by Retinue (16 November 2026)

Owner amendment of 16 November 2026 in `AGENTS.md` ("Pi, with keys held by Retinue"), plan `docs/plans/vault-channels-pi.md` section C, code `crates/retinue-daemon/src/pi.rs` and `crates/retinue-adapter-pi/`. This is the first time Retinue itself stores a model-provider API key. The exception is Pi only: Retinue is the thing that holds the key, and the only place a key goes is the environment of one supervised Pi child.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| P1 | **Retinue now holds the owner's provider keys** (a new asset on the host) | `Retinue/pi/<provider>` in Windows Credential Manager, written only by `pi.key.set` at the desktop, read only to build one child's environment; held in a `Zeroizing` string; no RPC returns a value and `pi.providers` answers `hasKey` only | An attacker who can already run as the owner can read the store, exactly as they could read any other credential on the machine |
| P2 | A key reaching a child that is not the Pi conversation it belongs to | `pi::add_keys_to_environment` is called from `exec_options_for_session` only when the conversation's provider is `pi`; Claude Code, Codex and OpenCode children are built without touching it | A bot's own shell tool can print its environment: the key protects at rest and from other providers, not from the bot the owner gave it to (the same limit `secrets.rs` documents) |
| P3 | A key in a backup, a diagnostics bundle, a log, an event, a prompt or a reply | Keys are not settings, not agent fields and not in SQLite, so `backup.*` and a diagnostics bundle never see one; nothing logs an environment value; Pi's own error text is bounded and a key-shaped word is replaced with `[hidden]` before it can become a turn's error | A provider that echoes a key in its own error would have it hidden, not verified byte for byte |
| P4 | Two providers sharing one key variable (OpenCode Zen and Go, Moonshot global and China, the Qwen pairs) silently overriding each other | `pi.key.set` refuses a second key while another provider with the same `keyEnv` has one, naming both | None expected |
| P5 | `pi.key.test` spending the owner's money on a request | The test runs Pi's own `auth check` (credential resolution, no model request) with a 20 s bound; the owner starts it at the desktop and `pi.key.test` is `never` for a device | Pi's `auth check` may refresh an OAuth token, which is a provider call |
| P6 | A key crossing to a paired device | `pi.providers`, `pi.key.set`, `pi.key.delete` and `pi.key.test` are `tiers.json` class `never`; no `pi.*` event exists | None expected |
| P7 | A project folder loading its own Pi extensions or settings into a bot | Pi is started with `--no-approve`, because a non-interactive Pi cannot ask whether to trust a folder; `AGENTS.md` and `CLAUDE.md` still load as context | Context files are still untrusted input, as Pi's own security page says |
| P8 | A Pi child acting beyond what the owner expects | Pi has no per-tool approval prompt, so Retinue does not claim any: the runtime row says `permissions: false` and no card is shown. Pi runs with the owner's OS permissions, like the other CLIs | The approval brake that exists for Claude Code, Codex and ACP agents does not exist for a Pi bot |

## 20. Chat channels to the CEO: Telegram and WhatsApp (16 November 2026)

Owner amendment of 16 November 2026 (`AGENTS.md`), contract in `docs/plans/vault-channels-pi.md` section B. A channel is a new **egress**: the owner's prompts and the CEO's replies pass through Telegram's or Meta's servers. Only one sender is ever paired, and only the CEO's channel conversation is reachable.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| H1 | **Prompts and replies leave the host** through Telegram or Meta, where they may be retained, logged or read by the platform | The channel is off until the owner connects it and approves one sender on the desktop. Only the CEO's channel conversation is routed. The reply is the CEO's final text only (no thinking, no tool output, no paths, no loopback or `file:` links, credential-looking words hidden), and long replies are split. | What the owner types and what the CEO answers is visible to the chat provider; that is the owner's choice of channel |
| H2 | A stranger talks to the bot, in a group or in a private chat | Structural refusals, in this order: `chat.type` must be `private`, `from.id` must equal `chat.id`, bots are refused, nothing at all is read until the owner has approved the one paired sender, and the sender id is checked again before the message is used. Groups, channels, other senders and status posts are dropped without an answer | None expected; a stolen phone is a signed-in account (H3) |
| H3 | A stolen phone sends prompts that run on the host | The paired sender is the only identity, and the CEO conversation is the same as any other: tool calls still pass approvals, and Full access is never granted by a channel message (the conversation's own mode applies). `channel.disconnect` in Settings removes the sender and the token at once | Anything the CEO is allowed to do unattended in that conversation can be asked for from the phone |
| H4 | Someone guesses the pairing code and takes the channel | The code is eight symbols from a 32-symbol alphabet, stored only as SHA-256, valid ten minutes, five wrong tries and the window closes; a candidate is recorded but nothing is accepted until the owner approves it on the desktop. `channel.pairing.answer` is `never` for a remote device | A guess creates a candidate card and nothing else |
| H5 | A bot token or an app secret leaks | Tokens and the app secret are in the Windows credential store (`Retinue/channel/<kind>/<name>`), never in SQLite, an event, a log, a backup or a diagnostics bundle. No RPC returns one; `channel.list` shows the account and a masked number only. Telegram errors are stripped of the token and of credential-looking words before they reach the page | The store is per user; a hostile process running as the owner can read it, as with every credential |
| H6 | A reply from the CEO carries something private into the chat (a path, a local URL, a key) | `chat_text` strips Windows and UNC paths, `http(s)://127.0.0.1|localhost|[::1]` and `file:` links, then hides credential-looking words and any word a `Bearer`/`token=`-style label precedes, before splitting | A secret the model invents in a form the scrubber does not recognise |
| H7 | An approval is answered in the chat by someone else, or guessed | Buttons carry an opaque one-use id held in memory, bound to one card, and a press is refused unless it comes from the paired sender in a private chat. The answer goes through the same `permission.reply` the desktop uses, so the audit line and the conversation state follow. Vault uses, `owner_*` and `computer_*` grants, questions, proposals, Full access and settings never reach the chat at all. Turning the buttons off is one switch (`channel.update { approvals: false }`) | The chat shows the card's title and detail, so the owner loses the diff view when answering from the phone |
| H8 | A provider redelivers a message and the CEO runs the same request twice | The inbound ledger is keyed by the provider's own message id, claimed once; the prompt also carries a deterministic `clientMessageId` derived from that id. A retry is dropped before any work | A message sent twice by the owner on purpose is two turns, as it should be |
| H9 | The daemon listens for Telegram or WhatsApp traffic | Telegram is polled only (`getUpdates`): no listener, no webhook, no public address. The gateway is untouched. `whatsappWeb.start` refuses in this build; the Cloud API is outbound only and its inbound design (a public webhook) is not built | The Cloud API's inbound half needs a new public route before it can pair a sender |
| H10 | A channel is used from a phone or another machine | Every `channel.*` method is `never` in `tiers.json`, and `channel.updated`, `channel.qr` and `channel.pairing.candidate` are dropped for remote devices | None expected |

**What this doesn't cover.** The Telegram or Meta account itself (a compromised bot token is H4/H5, a compromised phone is H3), the platform's own retention, and message content the CEO chooses to repeat. WhatsApp Web is not built: no client here reaches WhatsApp Web, and the note in `docs/operations/` records what a future one would have to be.
## 21. Learned skills and first-run suggestions (17 November 2026)

Plan `docs/plans/nod-adoption-plan.md` wave 3 (report 08, ideas 1 and 5), code `crates/retinue-daemon/src/skills.rs` and `suggestions.rs`, contract `docs/reports/wave3-skills.md`. This is the first time a bot can write text another bot will later follow.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| L1 | **A bot writes an instruction other bots follow** (its own notes as a prompt-injection path) | Only a turn the owner started may save or patch (the turn's origin is recorded where the turn is registered); the draft passes the memory module's secret and injection-phrase rails; a saved skill is `pending` and visible only to the bot that learned it until the owner approves it in Settings, Skills. Skill text grants no permission: the index says it is guidance, and the tools are unchanged | An owner who approves text they did not read; the approval card is the brake, and the text is theirs from then on |
| L2 | A saved procedure carries the owner's data: an address, a card number, a key, a sign-in link | Secrets, runs of 13+ digits and sign-in-looking links are refused outright; e-mails, links, home folders and runs of 6+ digits become `<email>`, `<link>`, `<user>` and `<number>` before anything is stored. The body never reaches a phone (`skills.learned.changed` is `drop`) | The model can describe a person or a value in prose the mask does not recognise |
| L3 | A stale or wrong skill confidently leads a bot wrong | 40 learned skills per bot, least recently used evicted first; `usedAt` is written on every load; the row shows when it was learned and last used; once the owner edits one the bot cannot patch it again | Nothing verifies the procedure is still right; that is the review's job |
| L4 | An unattended run teaches the whole team something | `save_skill` and `patch_skill` are absent from an automation or task run's tool list and refused by the dispatch; a teammate-woken turn is not owner-started, so it is refused too | None expected |
| L5 | A paired device approves, edits or forgets a skill | `skills.learned.list`, `.approve`, `.edit` and `.forget` are class `never`; `skills.learned.changed` is `drop`. `skills.list` and `skills.get` keep the `viewer` read they already had, and a body crosses exactly as it did for an imported skill | A paired device can read an approved skill's body, as before |
| L6 | Onboarding reads the owner's disk beyond the project | `project.suggestions` reads a fixed list of file names inside the project's canonical folder only, each capped at 64 KiB, with links and paths outside the folder refused; no model call, no event, nothing sent anywhere; the reply names the host folder, so the method is `never` remote | A project folder that points at a wider tree is the owner's own choice |
## 21. MCP connectors and their OAuth sign-in (17 November 2026)

Plan `docs/plans/nod-adoption-plan.md` section D (wave 1) and report `docs/research/n8n-nod/03-connectors.md` idea 2 (wave 3). A connector is a third-party MCP server the owner adds; a bot's tool call reaches it under the owner's own token, so X1 is the row wave 1 owed. The sign-in adds two things to this model: a consent page in the owner's own browser, and a listener on the host's loopback.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| X1 | **A bot's tool call reaches a third-party service under the owner's own token** | Every call is a card titled `<connector>: <tool>` unless the owner set a standing `allow`; `deny` refuses without a card. An unattended run may call only a tool the server marks read-only, and only while the connector's `unattended` switch is `readOnly`; Full access is never a bypass. The token is in Credential Manager, never in SQLite, an event, a reply, a log or a backup, and every result and error is masked for it in plain, URL-encoded and base64 forms | Anything the owner allowed once (or allowed for good) runs without another ask, and the card shows the arguments as text, not the service's own diff |
| X2 | **The callback listener is new reach into the host** | Bound to `127.0.0.1` on an ephemeral port, never a public address; one flow task serves it; one use; five minutes; the `state` is compared before a code is exchanged; a wrong or missing `state` answers 400 and grants nothing; the listener closes when the flow ends, is cancelled, or expires (`connector.oauth.cancel` waits for the task to drop) | A program already running as the owner can connect to the port during those five minutes; that program could already read Credential Manager |
| X3 | The consent page, or the URL that carries it, leaks a secret | The URL carries the random `state` and the PKCE challenge only, never a token or a code. The daemon opens it in the owner's own browser (the default handler), not in a bot's profile. It is returned once, in the `connector.oauth.start` reply, and nothing logs a URL that carries a query | The browser's own history keeps the URL; it holds no secret |
| X4 | Tokens at rest leak or are returned | `Retinue/connector/<id>` in Windows Credential Manager, split into numbered parts over the 2,048-byte cap and joined on read; read into `Zeroizing` strings; no RPC returns one; `secret.list` does not list them (a `/` is not a secret name); not in a backup or a diagnostics bundle | The store is per user: a process running as the owner can read it, as with every credential |
| X5 | A hostile or compromised discovery document points the daemon somewhere it should not go | Every discovered endpoint must pass the connector URL checks: HTTPS only, or plain http only for a loopback host; DNS pinned to the validated address; a cross-origin redirect is not followed; answers are capped; PKCE must be `S256`; dynamic registration is the only client-registration path (no copied client secrets) | A server the owner deliberately added can still name its own endpoints; that is its consent screen |
| X6 | A stale token is used, or a refresh races another | An access token is refreshed within 60 seconds of expiry, behind one gate, re-reading the bundle after the lock so two callers do not spend the same refresh token; a failed refresh says "connect again" and marks the sign-in `error` | A refresh that succeeds at the service but fails to save leaves the old bundle; the next call renews again |
| X7 | A sign-in is started from a phone or another machine | `connector.oauth.start`, `.status` and `.cancel` are `tiers.json` class `never`; `connector.oauth.changed` is `drop` | None expected |
| X8 | Removal leaves a usable grant behind | `connector.remove` (and clearing the token on an `oauth` connector) sends an RFC 7009 revoke to the service first, then deletes every credential part; a failed revoke does not stop the deletion | A service that does not implement revocation keeps the grant until it expires |

**What this doesn't cover.** The provider's own account security and consent screen, a grant the owner approves at the service and then revokes there while a call is in flight, and a service that silently ignores the revoke. WhatsApp-style third-party libraries are not involved here: the daemon speaks OAuth itself.

## 21. Projects on another machine, from the owner's devices (17 November 2026)

Owner amendment of 17 November 2026 (`AGENTS.md`): a paired device may list folders under one base folder the host's owner sets, register a folder there as a project, and clone a URL into it. Contract: `docs/reports/multi-device-projects.md`. Code: `crates/retinue-daemon/src/project_share.rs`, `project_clone.rs`, `machines.rs`, `remote/web.rs`, rows in `apps/desktop/src/remote/tiers.json` (class `workspace`).

The surface is **off by default** (`projects.devices.enabled` absent) and, when on, is confined to one folder (`projects.devices.folder`). Names and kinds only; no file contents, no sizes, no timestamps.

| # | Abuse | Mitigation | Residual risk |
|---|---|---|---|
| W1 | **A paired device browses the host's disk** beyond what the owner offered | Every call is refused unless the owner's switch is on here and a base folder is set. A path must be absolute, hold no `..` segment, and resolve (`canonicalize`) inside the base folder; `C:\work two` never passes as a child of `C:\work` because both sides are folded and compared with a separator, case-insensitively on Windows. A listing returns the immediate children only, at most 500, each by name and kind | A device at `operator` learns the base folder's own path and the names of everything under it. That is what the owner chooses by setting it |
| W2 | A symbolic link or junction inside the base folder reaching outside it | Each entry is `canonicalize`d and dropped when it resolves outside the base folder, so a screen cannot offer it; a browse or a create that names one is refused with the same sentence as any other outside path. `..` is refused before the disk is asked | A link whose target changes after the listing is judged again on the next call, so the answer is per call |
| W3 | **A device makes the host clone a URL** and uses the host's git sign-in | `project.clone` is `workspace`/`operator` and, for a device's call only, the parent must be inside the base folder (the gateway stamps `deviceCall: true` and a device cannot clear it, so the desktop's own clone still reaches any folder). Same URL checks as the desktop, the same in-memory job list, and `project.clone.cancel`/`.list` stay `never` for a device | The host's own git credential does the fetch. That is the host owner's choice when they turn the switch on; the clone writes only inside the base folder |
| W4 | A device claims to be the desktop so it can escape the folder | The gateway sets `deviceCall` on every `project.*` call after the request arrives, replacing anything the device sent under that name, and the tier check runs before the handler | None expected; the flag is never read from a device's body |
| W5 | One device reads another device's clone, or a job's details, by guessing an id | A job id is a random UUID, `project.clone.status` answers `not_found` for an unknown one, and the reply carries the folder the device asked for, a state word, one plain line and the project's id and name only | A device at `operator` that learns an id sees that job's target path and one-line error |
| W6 | A project row carries a setup script or shared-settings state to a device | The far side reduces the row to `id`, `name` and `path` before it leaves; `project.changed` stays `drop` for devices, so the window learns of it by reading `machine.projects` | None expected; a test asserts the object has exactly three fields |
| W7 | A folder path or the switch travelling in a backup or a diagnostics bundle | `backup.export` refuses every `projects.devices.*` key (`backup.rs` `exportable_setting`), and neither is in `SETTINGS_ALLOWLIST`, so `settings.get` for a device never carries them | None expected |
| W8 | A device writes to the host during quiet hours, or below the tier the owner chose | Class `workspace` is denied in quiet hours like `chat`, needs `operator`, and writes one content-free activity line on success ("*<device> sent project.browse from the remote app.*"). A refusal by the switch or by confinement writes no line | The activity line names the method, never a folder or a URL |
| W9 | The window on one machine driving a third machine through it | `machine.project.browse`, `machine.project.create`, `machine.project.clone` and `machine.clone.status` are `never` for a distance device; only the desktop's own link may call them, and each checks the link's tier (`operator`) before the request | None expected; the phone has no screen for this in this lane |
