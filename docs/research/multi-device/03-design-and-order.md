# A design for many-machine Retinue, what to reuse, the build order, and open questions

Part of `01-t3-vs-retinue.md`. Date 17 November 2026. Retinue `main` at `b7c3a32`, read from the working tree (see the note on line numbers in `01`, header). Research only: nothing here is implemented, and no rule below is changed by this file. Where a piece needs the owner to amend a rule, the rule is quoted.

Sections: 1 the shape (three options), 2 the pieces, 3 the rules each piece needs amended, 4 what T3 code is worth adapting, 5 the build order, 6 open questions, 7 what I could not confirm, 8 a note for the Director.

---

# 1. The shape: three options, one recommendation

The gap table (`02-retinue-today.md`, section 3) has three roots: no live view of another machine, nothing can be created there, and its Remote settings are untouchable from here. Two of those are plumbing. The third, and the phone's single-host limit, are **shape** questions, so decide the shape first.

| Option | What it means | Cost | Risk | Fits our rules? |
|---|---|---|---|---|
| **A. T3's shape: every client holds a catalogue of machines** | The phone and the window each pair with each machine and show them side by side. | High on the phone: the client is *served by the host* and bound to `location.host` (`apps/desktop/src/remote/remoteTransport.ts:244-245`), so three machines means three origins, three cookies and three installed apps; a shared catalogue would need cross-origin storage or `SameSite=None`, which the cookie rules refuse | High: three trust chains per device, three revocations, and a stolen phone holds three cookies | No, on the phone; yes on the desktop |
| **B. The daemon that serves the phone is the aggregator** | The phone pairs with **one** machine. That daemon already holds links to the others, so it presents their sessions, sends prompts through them and reads their events. Effective tier is the lower of the phone's tier and the link's tier | Medium: a socket in the machine client, a read-through surface for devices, and a phone UI with a machine picker | Medium: one phone holds reach into N machines for as long as it is paired; mitigated by one revocation point and a two-tier ceiling | Yes, with the amendments in section 3 |
| **C. One designated hub machine** | All links point at a hub; everything goes through it | High: the hub is a single point of failure and a single point of trust, and two machines on the same desk still route through it | High | Yes, but worse than B |

**Recommendation: B.** Reasons.

1. It keeps the rule that the daemon owns sessions and state, and the window never talks to another machine (`crates/retinue-daemon/src/machines.rs:1-11`). The phone stays a thin client of one host that it already trusts.
2. One pairing, one cookie, one revocation. A stolen phone loses every machine the moment it is revoked at its host, which is *better* than three separate revocations.
3. The ceiling is expressible: the far gateway's tier stays the authority (`machines.rs:521-536`), and the phone's own tier caps it, as T3 does with links ("The access you pick is the most agents from this machine may do there. An agent also never gets more there than its own mode here", `_research/t3code/docs/user/remote-access.md:383-384`).
4. It needs no third-party relay: the path is the same Tailscale (or Native TLS) hop that exists today.

**Its honest cost.** When the phone's host is asleep, the phone sees nothing, even if the home PC is awake. T3 does not have that failure because each connection is independent. Mitigations, in order of cheapness: (a) the phone may pair with a second host as a fallback, as a separate installed app; (b) when its host is unreachable the phone says plainly "Waiting for <host machine>" rather than showing the other machines as offline; (c) later, a hub election. I would ship (a) and (b) and not attempt (c).

---

# 2. The pieces

Each piece says: daemon, RPCs and events, UI, and the threat rows it needs. The rule amendments each needs are in section 3, with the exact wording quoted.

## Piece 1 — A live link, so another machine's sessions move as they happen (R1)

**Now.** Three polls: 20 s for machines (`machines.rs:54`), 3 s for the open conversation (`apps/desktop/src/ui/MachineConversation.tsx:19`), 15 s for the list (`:216`). `machine_client.rs` has no socket (`machine_client.rs:286-430`).

**Daemon.**

- A new module `crates/retinue-daemon/src/machine_stream.rs`: one WebSocket per online link to the far gateway's existing events socket, opened with the same pinned trust and the same device cookie the RPC path uses (`machine_client.rs:384-430`). The gateway already serves it: `crates/retinue-daemon/src/remote/web.rs:912` subscribes a connected device to the daemon's events.
- On connect, send `afterSequence` with the last sequence applied for that machine, and drop any replayed sequence at or below it — the same rule the phone client implements (`apps/desktop/src/remote/remoteTransport.ts:445`, `:514`, `:610-616`). The cursor is a number per machine, in memory only.
- The event allowlist already decides what a machine link may see, because a machine link is a `machine`-kind device at a tier (`apps/desktop/src/remote/tiers.json:1965-2022`). Nothing new is exposed: the laptop receives exactly what a phone at the same tier would.
- On a relevant event (`session.changed`, a message, an approval request, `session.deleted`), the daemon emits a **local, content-free** event `machine.session.changed` with `{machineId, sessionId, revision}` and nothing else. That keeps the existing intent that "nothing about the other machine's bots goes into an event, because events are stored" (`machines.rs:380-384`); the window then re-reads `machine.conversation`, which is redacted by the far gateway as today.
- Keep the 20 s poll as the floor and as the source of bot counts. Add a backoff and a wake on window focus, which the link has neither of today (`machines.rs:412-432`).

**RPCs.** No new read RPC. One new diagnostic: `machine.stream.status` `{machineId}` → `{connected, lastSequence}` at `viewer`, so the window can say "live" or "polling" honestly.

**Events.** New local event `machine.session.changed`; extend `machine.changed` with `{live: bool}`.

**What the socket gives, and what it does not.** `tool.changed` is `drop` for devices (`tiers.json:2023`), so a step may not produce an event of its own; while a turn is running, the daemon re-reads the open conversation on the existing 3 s timer (`MachineConversation.tsx:19`, which the daemon can carry instead), so a step appears within 3 s at most and immediately when `session.changed` fires. A tool's title and detail still arrive as `Tool` and nothing, because the gateway redacts them (`remote/policy.rs:428-431`, `:469-477`); showing the real work log on the laptop needs the redaction exception in open question 9, not a socket.

**UI.** The conversation dialog stops polling and re-reads on `machine.session.changed`; the transcript footnote stops saying nothing about freshness. The companion's machine badge already reacts to `machine.changed` (`apps/desktop/src/ui/machines.ts:140-148`) and needs no change.

**Threat rows.** One: *a machine link receives the host's event stream*. Mitigation: it is the same socket and the same allowlist a paired device gets at that tier; the cursor is a number; no bot or message content is stored here.

## Piece 2 — One list of every conversation, with the machine on it (R1, R3)

**Now.** Another machine's conversations exist only inside its bot dialog. `refresh_one` already reads `session.list` every 20 s (`machines.rs:342`) but keeps only bot counts (`:277-315`).

**Daemon.** New RPC `machine.sessions` at `viewer` returning the rows the 20 s loop already holds (id, title, agent id, project name, state, updated at, archived) for one machine or all links. No new outbound call. Add `kind` (the far machine's OS family) to what `machine.info` reports and to the link row, so a row can show a laptop or server icon rather than only a name.

**UI.** The conversations panel merges this machine's rows with `machine.sessions` rows and puts a machine badge on each row that is not this machine's (the badge exists: `apps/desktop/src/ui/MachineBots.tsx:16-18`). The bot groups stay, so nothing is lost. A filter "This machine / All machines" answers the owner who wants T3's view without giving up the current one.

**Threat rows.** None new: this is a read the gateway already permits at `viewer` (`tiers.json:40-46`), and it stays in memory.

## Piece 3 — "Where it came from" (R3)

**Now.** Only a moved conversation is marked (`apps/desktop/src/ui/ConversationsPanel.tsx:181`).

**Daemon.** When a call arrives over the gateway, the gateway knows the calling device (`remote/web.rs:828-857`). Stamp the created session with `link.startedFrom = {deviceId, deviceKind, machineName}` when the method is `agent.conversation.create` or `session.prompt` on a new conversation. That is the same shape as `link.movedTo` (`machines.rs:571-577`) and is content-free. Show it as "Started from <machine>" in the header and as a small mark on the row.

**UI.** One line in the chat header and one in the transcript's first notice.

**Threat rows.** None new; the value is a machine name, which the threat model already allows to travel.

## Piece 4 — Answer an approval from the machine in hand (R2)

**Now.** The transcript says "Answer it on <machine>" (`MachineConversation.tsx:278`) although `machine.permission.reply` is implemented at `operator` (`machines.rs:949`) with no UI caller.

**Daemon.** Nothing. The path exists and the far side enforces "only when the host allows approvals from remote" (`tiers.json:712-715`).

**UI.** The transcript's pending-approval block becomes a card with the tool, the command as the far gateway redacted it, and Allow/Deny, calling `machine.permission.reply`. If the far side refuses, show its sentence unchanged (`machines.rs:443-471` already maps it).

**Threat rows.** One: *an approval is answered from a third machine*. Mitigation: the card names the machine and the bot; the far gateway's own rules apply; approvals-from-remote must be on there; the answer is one-time and bound to the request id.

## Piece 5 — The phone sees and drives the machines its host is linked to (R1, R2)

This is the piece that needs the owner's decision. Quoted in section 3, amendment **A1**.

**Daemon.** A new device-facing surface on the gateway, class `linked`, granted to `phone`, `tablet` and `browser` devices, and never to `machine` devices (a link calling another link is how a mesh forms by accident):

| Method | Far-side class | Rule |
|---|---|---|
| `machine.list` | — | names and states only |
| `machine.sessions` | `read` | as piece 2 |
| `machine.conversation` | `read` | as today |
| `machine.start` | `chat` | refused when the link's tier is `viewer` |
| `machine.prompt`, `machine.steer`, `machine.cancel` | `chat` | same |
| `machine.permission.reply` | `approve` | only if the phone's tier is `operator` **and** the far side allows approvals from remote |

Never, on this surface: `machine.pair.*`, `machine.remove`, `machine.rename`, `machine.takeOver`, `handoff.*`, `machine.projects`, the new project-creation methods, and the device-management methods of piece 7. That keeps the phone's reach to *work*, not to *links*.

**Ceiling.** Effective = min(phone tier, link tier), checked in one helper next to `require_tier` (`machines.rs:521-536`), and the far gateway still decides. The phone's client shows the machine's name on every row, on the chat header and in every refusal.

**UI (phone).** A machine picker at the top of the roster, reading `machine.list` from its host; the roster and chat then work as today, scoped to the picked machine. "Connected to <host>" becomes "Through <host>", with the picked machine named.

**Threat rows.** Three: *a phone prompts an agent on a third machine*; *a stolen phone reaches every machine its host is linked to*; *a phone learns the names of machines and their bots*. Mitigations: the two-tier ceiling; revocation at the host ends all of it at once; names are already visible at the desktop; no link management from a phone.

## Piece 6 — Start a project on another machine (R4)

This is the largest piece and needs amendments **A2** and **A3**.

**Daemon (far side).**

- `project.browse` — a **scoped** directory listing: root fixed to the host's configured base directory for new projects, refusing any path that resolves outside it (symlinks included), names and types only, no file contents, no sizes. This answers the objection written into `project.discover` today: "It lists the folders of the host's disk" (`tiers.json:1342-1345`).
- `project.createFromFolder` — register an existing folder under that same root as a project.
- `project.clone` — allowed, but only for a URL that matches the repository identity of a project the calling machine already has, so "clone onto the other machine" cannot become "make the host fetch any URL with its git sign-in". The far side records the clone as an ordinary project clone (`projects.clone` already exists for the desktop) and shows it in the ledger.

**Daemon (near side).** `machine.projects` (exists, `viewer`), `machine.project.browse`, `machine.project.create`, `machine.project.clone`, `machine.clone.status` — each a thin pass-through of the shape at `machines.rs:549-566`, at the new class.

**RPCs and events.** Reuse the clone snapshot contract the desktop already has (`project.clone` + the clone list); surface it through `machine.session.changed`-style content-free events so a link's clone progress can be shown without copying git's output across machines.

**UI.** A "New project on <machine>" entry beside the machine's bots in the sidebar, and in the phone's machine picker. Its dialog is T3's shape adapted to our design language: source (this machine's project, with its repository identity), an arrow, target machine and folder chosen with the scoped browser, a protocol toggle (SSH/HTTPS), and a list of git-ignored files to carry. Progress shows the stages the far daemon reports.

**Threat rows.** Three: *a linked machine writes a folder on the host*; *a linked machine makes the host clone an arbitrary URL*; *a linked machine browses the host's disk*. Mitigations: root-confined browse; same-identity clone only; one ledger line per action; no file contents in any listing; and the host's owner can turn the whole surface off with one switch.

## Piece 7 — Manage the other machines from here (R5)

Needs amendment **A4**.

**Far side.** The daemon already implements the methods piece 7 needs; they are simply `never` for devices: `remote.status`, `remote.devices.list`, `remote.devices.update`, `remote.devices.revoke`, `remote.activity.list` and `remote.pair.confirm` (`remote/mod.rs:1011-1041`, handlers at `:1094-1245`). Add a device-facing subset, class `manage`, granted **only** to device kind `machine` at `operator`:

| Method | Why |
|---|---|
| `remote.status` | see whether Remote is on, on which method, at which address |
| `remote.devices.list` | see what is paired to that machine (ids, names, kinds, tiers, last seen) |
| `remote.devices.revoke` | cut a lost phone off at that machine |
| `remote.devices.update` | rename, or change a tier, **but never the caller's own row** |
| `remote.pair.pending` | **new read**: the waiting pairing request's name, kind, platform and login, which today only the host's own window sees (`remote/pairing.rs:281-295`) |
| `remote.pair.confirm` | approve that request with a tier, refusing any request whose kind is `machine` and any request from the caller itself |
| `remote.activity.list` | the content-free ledger |

Never: `remote.enable/disable`, `remote.pair.begin`, `remote.method.set`, every `remote.native.*`, `settings.set`, `agent.create`. Approving is allowed for **machines** only, never for phones.

That last point is a real exception and must be written as one: today "approvals from remote are off by default" (`AGENTS.md`, 17 October amendment), and here the laptop becomes a remote approver for the PC. It is defensible because a machine link is another desktop the owner physically controls, paired with a pinned certificate and an owner-chosen tier; but the ruling has to say *machine, at operator, never a phone*, and every approval must appear in the PC's own activity list.

**Console approval for a headless host.** A headless host needs two console paths, not one, and the code path and the password path differ:

- On **Tailscale** the certificate is web PKI, so the probe offers `auth: "code"` (`machines.rs:596-621`) and the linking machine posts to `/api/pair` with a code and `kind: "machine"` (`:726`). The code comes from `remote.pair.begin`, which refuses under Native (`remote/mod.rs:1067-1073`) and is a window-only RPC (`tiers.json:1440-1443`). So a headless host needs a console command that prints a code and never logs it.
- On **Native** the linking machine signs in with the host's password over a certificate the owner compares, then waits for the host's owner to approve the pending request with a tier (`machines.rs:728-760`; the pending request carries name, kind, platform and login at `remote/pairing.rs:281-295`). That approval is `remote.pair.confirm` (`remote/mod.rs:1094-1136`), also window-only. So a headless host needs a console confirm too.

One small bug to fix on the way: the pairing request hard-codes `"platform": "windows"` (`machines.rs:726`, `:738`), which would be wrong for a Linux daemon.

**UI.** The Machines page grows a "Manage <machine>" row per link: version, Remote state, devices, activity. Every destructive action confirms and names the machine.

**Threat rows.** Two: *one machine changes another's security posture*; *a machine approves a device on another machine*. Mitigations: `machine` kind only, `operator` only, never the caller's own row, ledger lines, and the far side's owner sees the change in its own activity list.

## Piece 8 — A daemon on the VPS (R6)

**The full assessment is `04-linux-headless.md`**: what T3 does (install, `t3 up`, a systemd *user* unit with `Restart=always` and OOM settings, a pinned runtime, updates by `t3 update`), what a headless `retinued` needs area by area with effort, how it is installed, updated, kept running and paired, and why the daemon belongs on the VPS host beside Harbor's container rather than inside it.

In short: data folder defaults to `%LOCALAPPDATA%` then temp (`main.rs:6247-6254`); secrets return an error off Windows (`secrets.rs:155-165`); Tailscale is found by `tailscale.exe` on `PATH` then under `Program Files` (`remote/tailscale.rs:78-90`); CI is `windows-latest` only (`.github/workflows/ci.yml:31`, `:98`). ACLs, process groups, runtime discovery and the hook name are already Unix-aware (`sandbox.rs:53-57`, `retinue-process/src/lib.rs:13-31`, `retinue-runtime/src/lib.rs:66-70`, `outside.rs:77`). Needs: an XDG data default, a `PATH` lookup without the `.exe`, a Linux CI job, a systemd unit, the `platform` field no longer hard-coded to `windows` (`machines.rs:726`, `:738`), the console pairing paths above, and the secret decision in section 6. **About two to three weeks of one lane, of which the first two days decide whether it is three weeks or six** (`04`, section 6).

---

# 3. The rules each piece needs the owner to amend

Quoted exactly, with the file and line. Nothing here is assumed; if a piece is not listed, it needs no amendment.

**A1. A phone may work through its host's links.** Today, three rows refuse it:

> `"why": "Machine links are the owner's, set up at the computer. A paired device must not link, rename or remove machines, or read another machine through this one."` — `apps/desktop/src/remote/tiers.json:1772`
> `"why": "Starting a conversation on a linked machine is the desktop's own link, made over its gateway; a paired phone must not drive it."` — `tiers.json:1779`
> `"why": "Answering an approval through a linked machine's gateway would let a phone approve on a third machine; only the desktop may."` — `tiers.json:1807`

And the working rule in `AGENTS.md` (17 October 2026, Remote):

> "**Never remote:** `settings.set`, every `remote.*` method, agent create/update/delete, runtime profile changes, runtime refresh, file revert, automation writes, memory writes, daemon shutdown, attachments (in v1) and every native Tauri command."

A1 amends the first three rows into a new `linked` class for `phone`/`tablet`/`browser`, and adds a line to that AGENTS.md sentence. It does **not** touch the rest of that list.

**A2. A linked machine may create a project on the host.**

> `"why": "A remote device must not create projects."` — `tiers.json:827`
> `"why": "Cloning writes a folder on the host and may use the host's git sign-in."` — `tiers.json:1324`
> `"why": "It lists the folders of the host's disk."` — `tiers.json:1345`

A2 replaces these for device kind `machine` at a new `workspace` class, with the root-confined browse and same-identity clone above, and a switch on the host that is off by default.

**A3. The 31 October amendment's sentence about another machine being read-only is already out of date.**

> "**Only one machine runs a conversation.** Another machine's conversations are read-only here. Take over is the only action. Sending a message to another machine's bot isn't part of this." — `AGENTS.md`, 31 October 2026 amendment

The code has shipped past it: `machine.start`, `machine.prompt`, `machine.steer` and `machine.cancel` exist (`machines.rs:922-948`) and the window has a composer (`MachineConversation.tsx:151-178`). The amendment should be rewritten to match, before the next reader trusts it.

**A4. A machine may manage another machine's Remote settings.**

> "**Never remote:** `settings.set`, every `remote.*` method, agent create/update/delete, runtime profile changes, runtime refresh, file revert, automation writes, memory writes, daemon shutdown, attachments (in v1) and every native Tauri command." — `AGENTS.md`, 17 October 2026 amendment
> `"why": "Approving a device with a tier is the owner's decision at the computer."` — `tiers.json:1457`

A4 replaces the words "every `remote.*` method" with "every `remote.*` method except the device-management subset for a linked machine", and carves out the seven methods in piece 7 for device kind `machine` at `operator`. Every other `remote.*` refusal stays, and it must also state the approvals exception in its own words: a **machine** at `operator` may approve a device on another machine, never a phone. It also needs this sentence in the threat model to change: "Reading another machine's conversations is the same trust as reading them from a phone: paired machines are the owner's own." (`docs/remote/THREAT_MODEL.md:264`) gains the device-management case beside it.

**Rules that no piece needs to change, and why that matters.**

- *"Content stays private. No message text appears in logs, push payloads or the activity list."* (`AGENTS.md`, 17 October) — every event in pieces 1 to 3 carries ids only; content is re-read through the far gateway, which redacts. This is the reason piece 1 relays ids rather than payloads.
- *"**A machine link is a paired device. … The window never talks to the other machine. No new listener and no relay.**"* (`AGENTS.md`, 31 October) — pieces 1 to 7 add no listener and no relay; the phone still talks to one daemon.
- *"**A handoff never grants Full access.**"* and the lease that stops two machines running one conversation (`handoff.rs:1-20`) — untouched. Piece 1 must respect `refuse_if_moved` (`machines.rs:568-577`) so a moved conversation does not appear live on the machine that gave it away.

**Also owed to the threat model.** Section 16 has a row for a phone moving a conversation and one for a handoff granting Full access, but **no row for a machine starting or steering a conversation**, which shipped on 13 November (commits `ef0244b`, `375a40a`; `machines.rs:922-948`). That gap should close whether or not this design is built.

---

# 4. What T3 code is worth adapting directly

T3 is MIT: `_research/t3code/LICENSE:1-21`, "Copyright (c) 2026 T3 Tools Inc." The obligation is that "the above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software" (`LICENSE:12-13`). Retinue already records the convention at `docs/legal/third-party-notices.md:97-99`: a T3 entry naming the pin and saying whether any file was copied, with the MIT text below it. If any of the following is copied or closely translated, update that entry to pin `b387e3a`, name the files, and keep the MIT text.

| T3 file | What is worth taking | How to take it |
|---|---|---|
| `apps/web/src/components/AddProjectToMachineDialog.tsx` | The dialog's information design: source on the left, a `git clone` arrow in the middle, target on the right; the protocol toggle; the ignored-files list with sizes and a default selection of `.env`-like files (`:106-125`, `:330-345`, `:360-427`, `:458-467`) | Rebuild in our React and design language; take the structure and the copy ideas, not the classes |
| `apps/web/src/lib/addProjectToMachine.ts` | The algorithm for carrying git-ignored files when the two servers never talk: read on the source first, start the clone, wait for its completion, write each file into the clone, collect per-file failures and offer a retry (`:102-107`, `:130-160`, `:163-203`). The caps (1 MiB per file, 200 files, `:21-25`) are sensible | Translate to TypeScript in our client (the daemon side differs: our clone runs in `retinue-daemon`) |
| `packages/contracts/src/projectClone.ts` | The clone snapshot shape: `phase`, `stage` (`connecting|counting|receiving|resolving|checkout`), `percent`, `detail`, `stalled`, `error`, `sequence`, and the two display helpers (`:13-41`, `:79-111`) | Adapt the field names to our contracts; the "stalled means the detail is a lie" rule is worth keeping |
| `packages/client-runtime/src/connection/supervisor.ts` + `docs/internals/connection-runtime.md:19-35` | The proved-dead-socket probe: 500 ms wait, then a fresh descriptor request each second, deadline 3 s on LAN and 15 s elsewhere | Ideas only; our transport is Rust, in `machine_client.rs` |
| `apps/server/src/orchestration-v2/ThreadStream.ts` | `decideThreadResume`: replay at most 128 events or 1 MiB, otherwise one snapshot (`:18-25`, `:99-114`) | Ideas only; our gateway has replay (`main.rs:1466-1478`) and no threshold yet |
| `apps/web/src/components/EnvironmentMachineIcon.tsx` | The set of machine kinds (laptop, desktop, server, cloud VM, mini PC, workstation) and their labels (`:41-59`) | Do not copy: the icons are lucide, which Retinue already uses; reuse the *kind list* |

**Not worth taking.** T3's connection catalogue (`packages/client-runtime/src/connection/*` as a whole): it is built for a client that holds many environments, which is the shape we are deliberately not adopting (section 1). Its relay and Cloudflare code (`infra/relay`, `packages/shared/src/relayClient.ts`) is out by rule.

---

# 5. The build order

Small, separately shippable steps. Each one leaves the owner with something usable, and none depends on a later one to be worth shipping.

| Step | What ships | Why here | Size | Blocks on |
|---|---|---|---|---|
| **1** | **Piece 1 + Piece 2 + Piece 4**: a live link, one conversation list with machine badges, and approvals answered from the machine in hand | The owner's laptop and home PC already link both ways; this is what they *see* every hour of the day. It needs **no rule change**, so no waiting on a decision, and it builds the socket the phone step reuses | M | Nothing |
| **2** | **Piece 8 + console pairing**: a Linux daemon, and a console way to print a code and confirm a request, so the VPS can exist as a place to work | The only hard "cannot" in the six requirements. Its first two days — a Linux CI job that proves `cargo check` passes — can start today, in parallel with step 1, and they decide the size of everything after | M to L | The secret decision (section 6, Q4); the compile probe decides the rest |
| **3** | **Piece 3**: "Started from <machine>", and the header naming the machine everywhere | Small, and it is what makes a merged list legible | S | Step 1 |
| **4** | **Piece 5**: the phone through its host, with the machine picker and the two-tier ceiling | The phone is one of the four devices and today sees one machine. This is the first amendment the owner must sign | M | Amendment A1 |
| **5** | **Piece 7**: device management from a linked machine | Makes the laptop the console the owner asked for; also what stops "I must walk to the PC to approve my phone" | M | Amendment A4, and step 2 for the VPS |
| **6** | **Piece 6**: new project and clone on another machine, with the root-confined browse | The largest and the riskiest; it should land when the rest is proven | L | Amendments A2, A3; steps 1 and 2 |

**Why step 1 first.** It is the step with no blocking question. Every other step waits on an owner decision (2, 4, 5, 6) or is cosmetic without it (3). Step 1 also converts the machine dialog from polling to events, which is the difference the owner will notice first: a new message or turn on the home PC appearing on the laptop in about a second rather than up to three.

**The one thing that should start before step 1 is written.** A Linux CI job running `cargo check -p retinue-daemon -p retinue-process -p retinue-runtime` costs a day and answers whether the VPS is three weeks of work or six (`04-linux-headless.md`, section 3.2). It needs no owner decision and no design, so it belongs in the same week as step 1.

**What "done" means for step 1, so it can be judged.** On the laptop, with the home PC linked: start a conversation on the PC from the laptop; a message typed there appears on the PC's window and back within 1 s; a new step row appears on the laptop within 3 s (as `Tool`, until question 9 is answered); an approval raised on the PC can be answered from the laptop; the conversation appears in the laptop's main list with the PC's badge; unplug the PC's network and the list says so within 20 s, then recovers on its own when it returns. That is a checklist the Director can run with the owner, not a unit test.

---

# 6. Open questions for the owner

1. **The phone's home.** Which machine should the phone pair with as its home (the one that aggregates)? If that machine is asleep, the phone sees nothing. Is that acceptable, with a second host as a fallback app?
2. **Phone reach.** Do you want the phone to be able to *start and steer* work on the machines its host is linked to (amendment A1), or only to *watch* them at first?
3. **Approvals across machines.** Should an approval raised on the home PC be answerable from the laptop (it already can be, in code, but the UI does not offer it), and from the phone two hops away?
4. **Secrets on Linux.** On the VPS there are three choices, and only the owner can pick: a `0600` file under the data folder (simplest, a written exception to "secrets live in the OS store"); `systemd-creds` / `LoadCredentialEncrypted` where systemd supports it (encrypted at rest, bound to the host, more moving parts); or bot secrets declared unsupported there (a daemon that runs conversations but cannot hold webhook, Harbor or vault secrets). Which?
5. **Project creation from another machine.** Allow it only for a linked *machine* (laptop → PC), or for the phone too? And for a clone, only the same repository as a project already on the calling machine, or any URL?
6. **Device management from a machine.** Should the laptop be able to approve a new phone on the home PC, or should approvals stay at every machine's own desktop? (My recommendation: machines only, never phones.)
7. **The list.** One merged list of every conversation with machine badges, or keep the current machine groups (with the merged list as an optional filter)?
8. **Stale data on the phone.** When a machine is offline, show its last known conversations greyed out (T3's way), or show nothing?
9. **How much of another machine's work log the laptop may see.** Today a tool step reaches another device as the word `Tool` with no command and no output (`remote/policy.rs:428-431`, `:469-477`). T3's client sees the full log because it *is* the machine's own app. Options: keep phone-strength redaction on a machine link (safest, but the laptop's view of the home PC is shallower than T3's); or let device kind `machine` carry tool titles and detail, since it is another owned desktop with a pinned certificate, and keep phones as they are. This is a content rule, so it is the owner's call.
10. **Whose account the VPS daemon runs as.** Your own user (a systemd *user* unit with `loginctl enable-linger`; it sees your git config, your SSH keys and your CLI sign-ins) or a dedicated service account (separate `~/.claude` and `~/.codex` sign-ins, a separate git identity, cleaner blast radius). It changes what "the VPS's credentials" means for every session started there.

---

# 7. What I could not confirm

- **T3's phone app at runtime.** I read `docs/user/remote-access.md`, `docs/internals/connection-runtime.md` and `docs/internals/remote.md`; I did not read `apps/mobile/src` in depth, so "the phone holds a catalogue of environments" rests on the docs (`connection-runtime.md:88-107`) and on the settings page described at `remote-access.md:167-176`. **Assumed**, not confirmed in code.
- **Retinue's UI at runtime.** Nothing was launched. The polling intervals and the missing approval UI are read from source (`MachineConversation.tsx:19`, `:216`, `:278`).
- **Whether `machine.start` on the far side records any provenance today.** I found the gateway's `device` in scope at dispatch (`remote/web.rs:828-857`) but only a `push.*` use of `deviceId` (`:840-846`); I did not trace `agent.conversation.create` into storage. Piece 3 assumes the stamp must be added.
- **The Linux build of `retinued`.** Not attempted; CI is Windows-only (`.github/workflows/ci.yml:31`, `:98`), so nothing proves it compiles.
- **Cost of T3 Connect** beyond the machine caps; not relevant to Retinue's design but relevant to the owner's comparison.
- **Whether the gateway's event socket accepts a machine-kind device** at every tier, or has an extra check. `remote/web.rs:912` subscribes the socket to daemon events; I did not read the surrounding 100 lines to confirm the tier check applies identically to a machine link.

# 8. A note for the Director

**Against `00-director-map.md`.** The map's reading of T3 and of Retinue matches what I found; three places differ, each with evidence:

- **Order.** The map puts the VPS last; I put it second (in parallel with the first step), because it is the only hard "cannot" among the six requirements (`main.rs:6247-6254`, `ci.yml:31`) and nothing else in the build makes the third machine exist. It is also the step that needs one owner decision (secrets on Linux), which is why it cannot be first.
- **The phone.** The map offers two ways: through the host, or pairing with each host. I recommend through the host only (option B, section 1), because the phone client is served by the host and bound to its origin (`remoteTransport.ts:244-245`), so "pairing with each host" means three separate installed apps; and because the two-tier ceiling is expressible and one revocation covers everything.
- **Tool steps.** The map says "`tool.changed` is `drop` for devices, so a phone sees messages but not what a session is doing" (`00-director-map.md:31`). The event is dropped (`tiers.json:2023`), but a *read* does include step rows, redacted to the word `Tool` with no detail (`policy.rs:428-431`, `:469-477`); and the laptop's transcript filters tool rows out entirely (`MachineConversation.tsx:239`). So the gap is both an event and a redaction, and the redaction half is an owner decision (question 9).

Two documentation debts found while reading, worth fixing whether or not this design is approved:

1. `AGENTS.md` (31 October) still says another machine's conversations are "read-only here" and "Take over is the only action", which the 13 November code contradicts (`machines.rs:922-948`, `MachineConversation.tsx:151-204`).
2. `docs/remote/THREAT_MODEL.md` section 16 has no row for starting or steering a conversation on a linked machine, which shipped in the same commit.

Proposed skill update (this sandbox may be read-only for the skill; the Director applies it): add to `.claude/skills/retinue-development/references/lessons.md`, dated 2026-11-17:

> **The machine-link surface is asymmetric: the far gateway allows more than the near daemon exposes, and the near daemon exposes more than the UI calls.** `machine.permission.reply` and `machine.projects` exist in `machines.rs:949-950` with no caller in `apps/desktop/src`. The far side still refuses project creation, folder listing and all `remote.*` for any device (`tiers.json:824-1345`, `:1419-1595`). Before promising a multi-device feature, check three places in this order: `tiers.json` class, `machines.rs` RPC table, then the UI.
