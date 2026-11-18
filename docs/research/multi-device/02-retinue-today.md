# Retinue today, and the gap to the owner's bar

Part of `01-t3-vs-retinue.md`. Date 17 November 2026. Retinue `main` at `b7c3a32`, read from the working tree (see the note on line numbers in `01`, header). Read only.

This file answers question 2 (Retinue against R1 to R6, exactly) and question 3 (the gap table with effort and risk). The design that closes the gaps is `03-design-and-order.md`.

---

# 2. Retinue today

## 2.1 What exists

| Piece | Where | What it does today |
|---|---|---|
| Machine identity | `crates/retinue-daemon/src/machines.rs:35-36` | `machine.id` (a UUID in settings) and `machine.name` |
| Machine link | `machines.rs:1-11` | This daemon acts as a **paired device of another machine's Remote gateway**. No listener, no relay; the window never talks to the other machine |
| Pairing | `machines.rs:590-740` | Probe `GET /api/ping`, then a one-time code over web PKI, or the other machine's password over a certificate the owner compares; the other owner approves with a tier |
| Device token | `machine_client.rs:341-377` | A user-only file under `<data>/machines/<id>.token`, read for one request, never in the database, a backup or a log |
| Transport | `machine_client.rs:286-330`, `:384-430` | HTTPS only, redirects off, no proxy, 32 MiB cap, 10 s default; web PKI for Tailscale, SHA-256 pin for Native |
| Reading a machine | `machines.rs:324-386` | Every 20 s: `machine.info`, `agent.list`, `session.list` and the device's tier in parallel; state becomes `online`, `offline` or `revoked` (`interval` at `:54`) |
| Bots of another machine | `machines.rs:277-315` | Name, colour, art, role, provider, model, busy flag, active conversation, count |
| Conversations | `machines.rs:912-921` | `machine.conversations` and `machine.conversation` pass `agent.conversations` and `session.get` through |
| Start and steer there | `machines.rs:922-950` | `machine.start` → `agent.conversation.create`; `machine.prompt`, `machine.steer`, `machine.cancel`; `machine.permission.reply` → `permission.reply` at `operator`; `machine.projects` → `project.list` (added 13 November, commits `ef0244b` and `375a40a`) |
| Tier check, near side | `machines.rs:486-555` | `require_tier` refuses before the request with the far side's own class; the far side's gateway stays the authority |
| Take over (handoff) | `handoff.rs:1-20` | `handoff.preview/prepare/commit/abort` on the giving machine, `machine.takeOver` here: commit and push the conversation's own `retinue/` branch, fetch and import, then flip ownership at `commit` |
| Gateway | `crates/retinue-daemon/src/remote/` (`mod.rs`, `web.rs`, `pairing.rs`, `policy.rs`) | Tailscale and Native methods, device cookies, per-method tiers, an events socket with replay |
| Phone client | `apps/desktop/src/remote/` (`RemoteApp.tsx`, `remoteTransport.ts`, `tiers.json`) | A PWA served by the host; snapshot plus an events socket with a persisted cursor (`remoteTransport.ts:154-164`, `:445`, `:610-616`) |
| Sidebar | `apps/desktop/src/ui/MachineBots.tsx:31-44` | Other machines appear as groups with a state dot and their bots as rows |

The 16 November amendment added the vault, channels and Pi; none of them changes this picture.

## 2.2 R1: all four devices see the same sessions, live

**This machine.** Live. The window listens to daemon events (`apps/desktop/src/ui/machines.ts:140-148` is the pattern; the main chat uses the same channel), so a turn's messages and steps arrive as they happen.

**A phone paired to one machine.** Live, and it catches up after sleep: the client opens an events socket, sends `afterSequence: cursor`, persists the cursor in storage (`remoteTransport.ts:164`, `:445`, `:610-616`) and drops any sequence it has already applied (`:514`). The gateway replays retained events (`crates/retinue-daemon/src/main.rs:1466-1478`; tier `read`, `viewer`, `apps/desktop/src/remote/tiers.json:19-22`).

**Another machine seen from this one.** Not live. Three polls and no socket:

- the daemon re-reads each machine every **20 s** (`machines.rs:54`, `:412-432`);
- the open conversation is re-read every **3 s** (`apps/desktop/src/ui/MachineConversation.tsx:19`, `:220-231`);
- a bot's conversation list every **15 s** (`MachineConversation.tsx:206-218`);
- `machine.changed` is emitted with only the state, because "nothing about the other machine's bots goes into an event, because events are stored" (`machines.rs:380-384`);
- the machine client has no event socket at all: `machine_client.rs` exposes `send`, `rpc`, `rpc_direct`, `rpc_with_timeout` and nothing streaming (`machine_client.rs:286-430`).

So a new message on the home PC appears on the laptop up to 3 s later, and only while that conversation is open. A session that starts on the home PC does not appear on the laptop at all until the 20 s read, and then only as a count on a bot, or a row once its dialog is open.

**What "what each session is doing" means here.** Two limits sit on top of the polling, and both are policy, not plumbing:

- the transcript keeps only `user`, `assistant` and `notice` rows (`MachineConversation.tsx:239`), so a tool step is not drawn at all;
- even a read that includes tool rows carries no detail: the gateway replaces a tool's real title with `Tool` and drops its `detail` unless `includeToolOutput` is on (`crates/retinue-daemon/src/remote/policy.rs:428-431`, `:469-477`, `:142-147`). The laptop is a device like any other, so it gets the same redaction.

T3's client is the owner's own computer and sees the full work log; Retinue's laptop sees another machine's work at phone strength. That is a deliberate privacy rule, and it is the one place where matching T3's *feel* needs an owner decision rather than code (open question 9 in `03-design-and-order.md`).

**A phone cannot see another machine, ever.** `machine.*` is class `never` for paired devices (`tiers.json:1769-1815`), and the phone client is bound to the origin that served it: it reads `location.host` (`remoteTransport.ts:244-245`) and shows one "Connected to <host>" (`RemoteApp.tsx:543`). There is no catalogue of hosts, which is the opposite of T3 (`01-t3-vs-retinue.md`, 1.3).

**Verdict R1: not met.** Two of four devices see live; the laptop sees other machines by polling, and only their text; the phone sees one machine only.

## 2.3 R2: carry on any session from any device

**This machine and its phone.** Met, within the tier rules: the phone sends, steers, stops and answers approvals at `operator` when the host allows approvals from remote (`tiers.json:593-621`, `:635-639`, `:712-715`).

**Another machine, from the laptop.** Partly built:

- a composer exists on another machine's conversation and steers while the bot is busy (`MachineConversation.tsx:151-178`);
- stop exists (`:165-168` → `machine.cancel`);
- "New conversation" on that bot exists at tier chat (`:193-204` → `machine.start`);
- **approvals cannot be answered**: the transcript says "waiting for an approval. Answer it on <machine>" (`:278`), although `machine.permission.reply` is implemented at `operator` (`machines.rs:949`) and no UI calls it (no match for `machine.permission.reply` in `apps/desktop/src` outside `tiers.json`);
- the only way to take a session over is the move: `machine.takeOver`, which needs `operator`, a copy or a remote, and pushes a branch (`handoff.rs:1-20`, `MachineConversation.tsx:39-121`). Moving is not "continue here"; it is a transfer with a push and a lease.

**A phone driving a linked machine.** Refused by design (`tiers.json:1769-1815`).

**Verdict R2: half met.** Text and stop from the laptop; approvals not; the phone reaches only its own host.

## 2.4 R3: where it runs and where it came from

- Another machine's bots are grouped under the machine's name with an online/offline dot (`MachineBots.tsx:16-44`).
- The conversation dialog header says "On <machine>" plus last seen (`MachineConversation.tsx:246-252`).
- In the main conversation list, a conversation that moved away gets a badge with the machine's name (`apps/desktop/src/ui/ConversationsPanel.tsx:181`, `MachineBadge` at `MachineBots.tsx:16-18`).
- The companion shows which bots are working on which machine (`MachineConversation.tsx:138-147`).

What is missing: a conversation that *started* on another machine, or arrived by handoff, is marked only by `session.link.movedTo`; there is no "came from" mark for a session that was started elsewhere and is still running there. And a conversation on another machine never appears in the main list, so "one list of everything, with machine badges" does not exist; the machine's conversations live inside its bot dialog.

**Verdict R3: partly met.** Name of the machine: yes, in the dialog and on moved rows. One unified list with badges: no.

## 2.5 R4: start new projects on another device

**Nothing exists in the UI.** `machine.projects` (→ `project.list`) is implemented (`machines.rs:950`) with no caller in `apps/desktop/src`.

**And the gateway refuses the three methods a clone or a folder pick would need, for every remote device:**

| Method | Class | The rule as written |
|---|---|---|
| `project.create` | `never` | "A remote device must not create projects" (`tiers.json:824-827`) |
| `project.clone` | `never` | "Cloning writes a folder on the host and may use the host's git sign-in" (`tiers.json:1321-1324`) |
| `project.discover` | `never` | "It lists the folders of the host's disk" (`tiers.json:1342-1345`) |
| `project.scaffold*`, `project.update`, `project.delete` | `never` | (`tiers.json:1251-1338`) |

So R4 is blocked twice: no UI, and a deliberate refusal on the far side. This is the single largest gap for the owner's typical case (start work on the home PC from the office).

**Verdict R4: not met**, and not for lack of plumbing alone.

## 2.6 R5: manage everything about the other devices from the device in hand

From the laptop, `Settings > Machines` can: pair a new machine, rename it, remove it, refresh it, and read its tier (`MachinesPage.tsx:172-201`; the tier is read with the rest at `machines.rs:343`, `:374-376`).

Everything else is refused on the far side, for every device:

- `remote.*` — all 22 rows are `never`, including `remote.devices.list`, `remote.devices.update`, `remote.devices.revoke`, `remote.pair.confirm`, `remote.status` (`tiers.json:1419-1593`);
- `settings.set` — `never` (`tiers.json:726-729`);
- `agent.create` — `never` (`tiers.json:733-736`).

So approving this laptop as a device on the home PC, changing its tier there, revoking a phone, turning Remote on or off, or reading its security options all need a walk to that machine. On the VPS there is nothing to walk to (2.7).

**Verdict R5: not met.**

## 2.7 R6: easy, robust and reliable

What is good: pairing is two screens and an approval; the token is a user-only file; a revoked link says so and stops asking (`machines.rs:332-336`); a wrong machine at the address marks the link offline (`:360-363`); a failed call has one error mapping (`:443-471`); the phone has a real retry with backoff and a cursor (`remoteTransport.ts:168-176`, `:610-616`).

What is missing:

- **No live connection to another machine, so no reconnect to speak of.** The 20 s loop is the whole retry policy: constant, no jitter, no separate "the socket died" signal.
- **No Linux daemon for the VPS.** Assessed in full in `04-linux-headless.md`: the data folder still defaults to `%LOCALAPPDATA%` and otherwise the temp dir (`main.rs:6247-6254`); the secret store returns an error for every call off Windows (`secrets.rs:155-165`); Tailscale discovery looks for `tailscale.exe` on `PATH`, then under `Program Files` (`remote/tailscale.rs:78-90`); CI is `windows-latest` only (`.github/workflows/ci.yml:31`, `:98`). ACLs, process groups, runtime discovery and the hook name are already Unix-aware, so the unknown is smaller than the Windows-only line count suggests.
- **No headless approval.** `remote.pair.confirm` is a window RPC (`tiers.json:1454-1457`) and all `remote.*` are `never`, so a headless host cannot approve a device; the daemon's capability token reaches only the shell that launched it (`main.rs` prints `retinued.ready` to stdout; see `docs/research/many-machines-2026-11.md:283-293`).
- **One address per link.** A link stores a single `url` (`machines.ts:27`, `machines.rs:658-670`); there is no route list, so a machine reachable by tailnet name and LAN is reached the one way it was paired.
- **Approvals from another machine are off** unless the far side allows approvals from remote (`tiers.json:712-715`).

**Verdict R6: not met for the VPS, and thin for the laptop-to-PC path.**

## 2.8 The gap in one paragraph

Retinue has the hard, security-heavy half: a paired machine link with a pinned certificate, a device token, tiers enforced on the far side, a gateway with an events socket and replay, a phone client, and a working move. What it does not have is the **shape** T3 has: T3's client is a catalogue of machines, each a live window onto its own sessions; Retinue's window is one machine's, with other machines bolted on as polled summaries and a dialog. Three consequences follow, and they explain almost every line of the gap table: no live view of another machine, no way to create anything there, and no way to manage its Remote settings from here.

---

# 3. The gap table

Effort is sized in backend-lane units of the kind the Director has been issuing (one file plus tests, or one UI surface). **S** is a few days, **M** a week or two, **L** a phase.

| # | Requirement | T3's way | Retinue's way | The gap | Effort | Risk |
|---|---|---|---|---|---|---|
| R1 | Live on every device | Sessions live on one server; every device a live window; replay or snapshot after sleep (`01`, 1.3) | Live only to the host; other machines polled at 3 s, 15 s and 20 s (`machines.rs:54`, `MachineConversation.tsx:19`, `:216`); a step reaches the laptop only as `Tool`, with no detail (`policy.rs:469-477`), and tool rows are filtered out of the transcript (`MachineConversation.tsx:239`) | A machine event socket, or a relay of the far gateway's events into this daemon's bus, with a cursor per machine | M | Medium: an event relay must carry no content beyond what the gateway already allows, and must not replay into stored events (`machines.rs:380-384` relies on that) |
| R1b | Phone sees all machines | Phone holds a catalogue of environments (`connection-runtime.md:88-107`) | One origin, one host (`remoteTransport.ts:244-245`, `RemoteApp.tsx:543`) | A host catalogue in the phone client, with a cursor and a status per host | M | Medium: three device cookies in one browser origin store; must not share a cursor across hosts |
| R2 | Type into any session anywhere | Same RPC surface everywhere | Composer, steer and stop from the laptop; approvals not; phone cannot drive a link | Wire `machine.permission.reply` into the transcript's approval card; allow the phone to reach machine links at all (rule change) | S for the laptop; **L** for the phone (rule change) | High for the phone: it multiplies the prompt surface across machines and needs its own threat rows |
| R3 | Where it runs, where it came from | Machine kind and label per row, "Also on", "From" | Machine group per bot; header "On <machine>"; badge on moved rows | One list of all conversations with a machine badge; a "came from" mark; machine kind and label as data on the link | S to M | Low |
| R4 | Start a project on another machine | Add project on a chosen machine (browse that disk), clone onto a chosen machine, client-relayed ignored files (`01`, 1.5) | Nothing; and `project.create`, `project.clone`, `project.discover` are `never` (`tiers.json:824-1345`) | A clone/project flow on a linked machine + a scoped folder browse + a new tier class | **L** | **High**: this is the rule the product is built on ("a remote device must not create projects"), and a folder browser is a read of the host's disk |
| R5 | Manage other devices from here | Each environment's Settings → Sharing & access is reachable from any connected device (`remote-access.md:402-457`) | Machines page can pair, rename, remove, refresh; the far side refuses everything else | `machine.remote.*` management pass-throughs at `operator` for `machine` devices only, plus a console approval on a headless host | **L** | **High**: it lets one device change another's security posture; needs by-method rules, not a class |
| R6a | Reliable reconnects to another machine | One retry owner, proved-dead sockets (`connection-runtime.md:9-35`) | Constant 20 s poll | Keep the poll as the floor; add backoff and a wake on foreground when a socket lands | S | Low |
| R6b | A daemon on the VPS | T3's server runs headless as a systemd user service (`background-service.md:11-43`, `bootService.ts:100-145`) | Windows-only data default, secret store, Tailscale lookup and CI (`main.rs:6247-6254`, `secrets.rs:155-165`, `remote/tailscale.rs:78-90`, `ci.yml:31`) | Linux build, XDG default, `PATH` lookup, a secret decision, CI job, unit file, tarball release; full assessment in `04-linux-headless.md` | M to L (2–3 weeks of one lane) | Medium: the secret decision is the owner's; whether it compiles is unproven |
| R6c | Headless approval | `t3 auth connect-device approve <code>` (`remote-access.md:53-56`) | `remote.pair.begin` and `remote.pair.confirm` are window RPCs (`tiers.json:1440-1457`) | Two console paths: print a code on a Tailscale host, and confirm a pending request with a tier (`machines.rs:596-760`) | S to M | Medium: the capability must not become a logged secret |

**Order of size.** R4 and R5 are the two large ones, and both are rule questions before they are code questions. R1 and R1b are the ones the owner will *feel* first, and they need no rule change: nothing new leaves the host that the gateway does not already allow.

Continue to `03-design-and-order.md`.
