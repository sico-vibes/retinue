# Multi-device Retinue: what T3 Code does, where Retinue stands, and a way to the owner's bar

**What this is:** research for the owner, through the Director.
**Status:** research only. No code changed, nothing committed, nothing installed or run.
**Date:** 17 November 2026.
**Retinue:** `main` at `b7c3a32`. Read from the working tree on 17 November, which was `b7c3a32` plus the Director's uncommitted wave-2 edits; one file I cite, `apps/desktop/src/ui/MachineConversation.tsx`, differs from `HEAD` by a one-line loading placeholder, so its line numbers follow the working tree and are one higher than `HEAD`'s after line 16. No Retinue code was changed by this study.
**T3 Code:** MIT, shallow clone at `C:/Users/Sico Vibes/Desktop/DevCN/_research/t3code`, `main`, `b387e3a179d7495321019363c5ba11e847834cd0`, 17 November 2026. Read only; install and run nothing.

This study answers one question: **can the owner move their daily work from T3 Code to Retinue, on four devices at once, at the same ease and reliability?** It does not decide the product; it says what T3 does, what Retinue does, the exact gap, and one design fitted to our rules.

## The six requirements, as testable statements

Taken from the owner's words, turned into statements that can each be checked against code.

| # | Requirement | What "met" means |
|---|---|---|
| R1 | All four devices see the same sessions, live | A session started anywhere appears on every device within about a second, and each change (a message, a tool step, a stop, an approval request) shows without a refresh |
| R2 | Carry on any session from any device | Any device can type into any session, stop it and answer its approval, without first moving anything |
| R3 | Each conversation shows where it runs and where it came from | The row and the chat header name the machine; a session started by another machine or arrived by handoff is marked |
| R4 | Start new projects on ANOTHER device from the device in hand | From the phone or the laptop: clone a repository on the home PC or the VPS, or pick a folder on that disk, then start sessions there |
| R5 | Manage everything about the other devices from whichever device is in hand | Pair, approve, revoke, change tiers, see versions and connectivity, restart the other daemon, without walking to it |
| R6 | Easy, robust and reliable | Sleep/wake, offline, a stopped daemon and a changed address each recover without the owner thinking about it |

## Files

| File | What it holds |
|---|---|
| `01-t3-vs-retinue.md` (this file) | The six requirements; how T3 Code meets each (question 1); what changed since the October and November studies |
| `02-retinue-today.md` | Retinue today against the same six (question 2), and the gap table with effort and risk (question 3) |
| `03-design-and-order.md` | A design fitted to our rules, the rows it needs, the rule amendments it would need from the owner (question 4); what T3 code is worth adapting and the notice it needs (question 5); a build order (question 6); open questions (question 7) |
| `04-linux-headless.md` | The VPS: how T3 runs headless on Linux, what a headless `retinued` needs area by area with effort, how it is installed, updated, kept running and paired, and whether the Harbor container could host it. Also a check of the Director's own findings |

## How to read the evidence

- `file:line` for every claim about T3 or Retinue. A claim I could not confirm says so.
- **Confirmed** means I opened the file or the doc and read the line.
- **Assumed** means I reasoned from something adjacent; it is marked where it matters.
- T3's own docs are treated as evidence about T3, not proof that it works; nothing in either product was run.

---

# 1. How T3 Code does each requirement

## 1.1 The topology: one server per machine, no hub, every app a client

T3 has no hub. Each machine that runs work runs an **environment**: a server that owns the providers, the agent processes, terminals, Git, project files and durable state. Web, desktop and mobile are clients that speak RPC to it and may hold several environments at once (`docs/internals/overview.md:3-6`, `docs/internals/remote.md:3-7`).

- The desktop app bundles a server and launches it as its "primary" backend, alongside optional SSH and WSL backends; the pool is keyed by instance id and the primary instance is special (`apps/desktop/src/backend/DesktopBackendPool.ts:45-84`, `PRIMARY_INSTANCE_ID` at `:47`).
- The desktop renderer is **not served by the backend**: the `t3code://` scheme serves the bundled client from disk and API traffic goes to each environment's own URL (`docs/internals/remote.md:188-192`).
- A desktop can turn its own server off and become a pure client (`localEnvironmentEnabled`, `docs/internals/remote.md:182-192`, `docs/user/remote-access.md:496-508`).
- An environment's identity is stable across restarts and address changes; a project and its threads belong to exactly one environment (`docs/internals/remote.md:9-19`).
- A "logical project" can exist on several machines: the client groups checkouts by repository identity (following `upstream` when there is a fork), and labels them by `origin` (`docs/internals/remote.md:9-19`). This is what makes one sidebar show one project with several machine members (`apps/web/src/components/ProjectEnvironmentBadge.tsx:7-44`).

**Consequence for R1 and R2.** "The same sessions on every device" is not a synchronisation feature in T3. Each session lives on exactly one machine, and every device is a window onto that machine. Nothing is copied to be kept in step, so nothing can drift. This is the single design choice that makes the rest of T3's behaviour fall out.

**The phone is a client only.** Phones and browsers do not count against the machine cap (`docs/user/remote-access.md:79-83`) and there is no mobile server.

## 1.2 How devices find and trust each other

Four routes, one server identity behind all of them (`docs/internals/remote.md:3-7`, `docs/user/remote-access.md:134-176`):

| Route | How it is set up | Evidence |
|---|---|---|
| T3 Connect | Sign in with one cloud account (Clerk) on the host and the device; a Cloudflare tunnel per environment | `docs/internals/t3-connect.md:3-12`; the host runs `cloudflared` pinned in `packages/shared/src/relayClient.ts` (`:175-198`) |
| LAN / private network | Enable network access, then a one-time pairing link or QR code | `docs/user/remote-access.md:91-133` |
| Tailscale HTTPS | `tailscale serve` on the host, turned on in Settings or with `t3 serve --tailscale-serve` | `docs/user/remote-access.md:196-229` |
| SSH (desktop-managed) | Enter `user@host`; the desktop spawns SSH, downloads a server to `~/.t3/runtime`, forwards the port | `docs/user/remote-access.md:241-270`, `docs/internals/remote.md:168-174` |

Authorisation is per device and is a set of scopes, granted at pairing and never widened afterwards (`docs/internals/environment-auth.md:9-28`). Cookies, bearer tokens and DPoP tokens all adapt the same scoped session (`:25-28`). Scopes include `orchestration:read|operate`, `terminal:operate`, `filesystem:read|write`, `source-control:write` and separately access management (`docs/user/remote-access.md:402-457`).

**T3 Connect is not a proxy after bootstrap.** The relay mints a one-time bootstrap credential bound to the client's DPoP key, the client exchanges it directly with the environment, and the relay never sees the session token (`docs/internals/t3-connect.md:44-65`; "the relay Worker does not proxy their HTTP or WebSocket sessions", `:3-5`). Device approval is available and off by default; when on, the environment trusts device keys, not the relay, and a forged mint only ever produces a request that waits for approval (`:67-83`).

**Routes are replaceable; identity is not.** A saved environment holds an ordered list of routes; the driver connects over the first that works, and the supervisor preflights earlier routes while connected over a later one (`docs/internals/remote.md:32-56`). Any route that answers as this environment is valid; the client learns the LAN and tailnet addresses the server reports and drops those it stops reporting (`:47-56`). Peers must never fall back to loopback, because loopback means a different machine on another device (`:27-30`).

## 1.3 How sessions and their live events reach every device (R1)

One RPC session per environment per client, over HTTP and WebSocket, with subscriptions. The contract is the boundary between independently versioned clients and servers (`docs/internals/overview.md:16-19`).

- **Intent is durable before work starts.** The orchestrator decides events and `EventSink` commits events, projections, the accepted command receipt and outbox effects in one transaction; subscribers get events after that commit (`docs/internals/overview.md:45-73`).
- **The log keeps about a day of history** for reconnect replay and command-id retry, and `ProjectionMaintenance` prunes older events (`docs/internals/overview.md:61-73`).
- **Catch-up after sleep is replay, or one snapshot when replay would be too big.** `decideThreadResume` replays at most 128 events or 1 MiB, otherwise returns a bounded current snapshot (`apps/server/src/orchestration-v2/ThreadStream.ts:18-25`, `:91-114`). A client whose cursor is below the pruned sequence resumes from a snapshot; an idle thread resumes with no snapshot (`docs/internals/overview.md:66-73`).
- **The retry policy has one owner**: jittered exponential backoff capped at five minutes, reset only after a connection stays up (`docs/internals/connection-runtime.md:9-17`). Offline reports and auth failures wait for a wakeup instead of burning attempts (`:17-19`).
- **Stale sockets are proved dead, not guessed.** A 500 ms probe, then a fresh public-descriptor request each second; the deadline is 3 s on loopback and LAN and 15 s elsewhere, because a tailnet over DERP can be slow (`docs/internals/connection-runtime.md:19-35`).
- **HTTP and socket health are separate.** RPC sessions authenticate at socket upgrade; HTTP requests take current credentials from the authorisation service, so a token renewal does not interrupt a conversation; T3 Connect access tokens last an hour and are renewed in the last minute (`docs/internals/connection-runtime.md:43-62`).
- **Cached data stays readable offline** and must not overwrite newer live data during a reconnect (`docs/internals/connection-runtime.md:73-76`).
- **Running threads are kept mounted on desktop** so opening one needs no replay; web and mobile do not do this (`docs/internals/connection-runtime.md:138-151`).
- **Thread detail separates subscription lifetime from cache lifetime**: a registry-local cache keeps state and replay cursor for five idle minutes (`:138-143`).

**Ordering.** The server's own sequence numbers order a stream; a snapshot carries `snapshotSequence` and per-stream versions (`ThreadStream.ts:68-81`; `docs/internals/overview.md:61-73`). Ordering is decided by the server, never by a client's clock.

## 1.4 How "where it runs" and "where it came from" are modelled (R3)

- **A machine kind and a label** travel with each environment: `EnvironmentMachineKind` is `server | cloud | linux | desktop | laptop | mac-mini | mac-studio`, with display labels (`packages/contracts/src/environment.ts:43-50`, `apps/web/src/components/EnvironmentMachineIcon.tsx:41-59`).
- **A thread row shows the machine** when the row is not the current environment: the row computes `isRemote = thread.environmentId !== currentEnvironmentId` and renders a machine icon and the environment label (`apps/web/src/components/sidebar/SidebarThreadRow.tsx:840-848`, `:1168-1215`).
- **A project header shows where else it exists**: a tooltip "Also on …" listing the other machines that hold the same repository (`apps/web/src/components/ProjectEnvironmentBadge.tsx:7-44`).
- **The composer names its host** and offers "Add to machine" (`apps/web/src/components/BranchToolbarEnvironmentSelector.tsx:33-60`).
- **Provenance is explicit for work that came from elsewhere.** Threads that a peer link starts are marked **From**, with the machine name, and carry an immutable `linkOrigin` that follows subagents, forks and threads they create (`docs/user/remote-access.md:385`, `docs/internals/remote.md:113-146`).

## 1.5 Starting a project or a session on a remote device (R4)

Three entry points, all of which run on the target machine with the target machine's credentials, CLIs and files ("Projects, provider credentials, and agent work stay on the remote machine", `docs/user/remote-access.md:246`).

1. **A new project on a chosen machine.** The command palette's add-project flow picks an environment, then browses **that environment's** filesystem: `browseEnvironmentId` and an environment-scoped `filesystemEnvironment.browse` (`apps/web/src/components/CommandPalette.tsx:638-641`, `:684`; `packages/client-runtime/src/state/filesystem.ts:200-201`; the RPC shape is `packages/contracts/src/filesystem.ts:29-52`). Project creation is `projects.createNew` (`packages/contracts/src/rpc.ts:450`).
2. **A clone onto a chosen machine.** `projectClone.start` / `.cancel` / `.retry` with a subscription to the clone list (`packages/contracts/src/rpc.ts:653-656`, `:1475-1499`), and the clone snapshot contract gives staged progress and failures (`packages/contracts/src/projectClone.ts:13-41`). The clone is a durable server-side operation with a cancel (`:69-77`); `sourceControl.cloneRepository` is the one-shot variant (`rpc.ts:645`).
3. **"Add to another machine": clone an existing project onto a second machine** (`apps/web/src/components/AddProjectToMachineDialog.tsx:72-73`). This is the owner's typical case in one dialog:
   - the source machine, the target machine and the target folder are all on one screen; the default folder comes from the target's own `addProjectBaseDirectory` setting (`:113-125`, `:347-358`);
   - the clone URL is offered as SSH or HTTPS, derived from the source project's identity (`:106-114`, `:330-345`);
   - the target server does the clone, so the target's git sign-in is what matters, and the app suggests granting the source-control app access on the target (`apps/web/src/lib/addProjectToMachine.ts:130-160`);
   - **git-ignored files travel client-side**, because "the two servers never talk": the client reads `.env`-like files from the source and writes them into the clone on the target after it finishes (`apps/web/src/lib/addProjectToMachine.ts:102-107`, `:163-203`; the selection UI is `AddProjectToMachineDialog.tsx:360-427`);
   - the client lands the owner on the new machine's copy immediately, with the clone running under the composer and sends held until it finishes (`AddProjectToMachineDialog.tsx:186-209`, `:211-239`).

**Which machine does the work.** Always the target. Its provider CLIs, its sign-ins, its disk, its ports. The client is only an orchestrator of requests. Nothing about the source machine's credentials travels (`docs/internals/overview.md:3-6`).

**Auto balance** can choose the machine for a new thread in a project that exists on several connected machines, with per-machine preferences, and the composer keeps that choice stable once made (`docs/user/remote-access.md:178-194`). Picking a branch or worktree pins the draft to that machine (`:190-194`). It is off by default (`:180`).

## 1.6 "Continue on another device" (R2)

There is no "move this session to my phone" in T3, because there is nothing to move: the session is on the machine, and the phone is another window (`docs/internals/remote.md:3-7`). Typing from any device is `thread` send/steer over the same RPC surface, and every device sees the same projection.

The things T3 *does* call handoff are about context, not devices:

- **Portable handoff** carries a textual, budgeted selection of app history when a provider cannot resume or when the owner switches provider or forks; it never claims native provider context (`docs/internals/context-handoffs.md:1-19`, `docs/user/portable-handoffs.md:1-47`).
- **Peer links** let one environment's agents work in another: launch, message, wait on and stop its threads, manage queues, projects and scheduled tasks; a delegated task runs there as an ordinary thread and the parent wakes with the result (`docs/user/remote-access.md:356-400`, `docs/internals/remote.md:70-146`).

So "carry on from the phone" is a *connection* problem, not a *transfer* problem. That distinction is the one that matters most for Retinue's design (see `03-design-and-order.md`, section 1).

## 1.7 What happens when a device or machine is offline

- **Machine asleep or stopped**: the saved environment keeps its routes and shows offline; the supervisor backs off to five minutes, and probing resumes on foreground, network change or a wakeup (`docs/internals/connection-runtime.md:9-35`). Cached projections stay readable (`:73-76`).
- **Headless machines keep working**: a background service runs the server with no app open, listens on all interfaces and refuses any peer that is not loopback, private or tailnet IPv4, which keeps a VPS's public address closed (`docs/internals/remote.md:58-65`, `docs/user/background-service.md:1-58`).
- **T3 Connect tunnels are reclaimed and recreated**: an idle tunnel is removed after several minutes down, the hostname reservation survives, and the host recreates it on wake without re-pairing (`docs/internals/t3-connect.md:119-158`, `docs/user/remote-access.md:472-481`).
- **Peer links survive a restart**: an already-running remote task finishes and its result arrives when links come back on (`docs/user/remote-access.md:362-366`); open remote tasks are followed again at startup (`docs/internals/remote.md:133-140`).
- **Client restart forgets only local credentials**: desktop restarts forget the previous local bearer token and its reusable bootstrap grant replaces prior sessions (`docs/internals/environment-auth.md:107-112`).

## 1.8 T3's own costs and weak points, stated plainly

- **A paid third party sits in the pairing path** for T3 Connect: Clerk for identity and Cloudflare for tunnels, with 3 machines free and 10 or 100 with a paid T3 Chat plan (`docs/user/remote-access.md:79-89`). Traffic does not go through the relay after bootstrap (`docs/internals/t3-connect.md:3-12`), but identity and reachability do.
- **Peer links expire after 30 days** and need a new pairing code (`docs/user/remote-access.md:393`).
- **Cloning to another machine is not a folder picker.** The destination is a text field pre-filled from the target's base-directory setting (`AddProjectToMachineDialog.tsx:347-358`); the browsable picker is in the add-project flow (`CommandPalette.tsx:638-641`), not in "add to another machine".
- **Ignored-file copying is client-relayed** and capped at 1 MiB per file and 200 files (`apps/web/src/lib/addProjectToMachine.ts:21-25`). Folders git ignores whole are left out (`AddProjectToMachineDialog.tsx:422-425`).
- **Everything is documented from T3's side; nothing was run here.** Mobile behaviour is known from `docs/user/remote-access.md` and `connection-runtime.md` (mobile list behaviour), not from the app.

## 1.9 What changed since the two earlier studies

- **`docs/history/fix-40/t3-research.md` (18 October, T3 `3ff5b3c`)** studied how T3 runs agents: durable intent, resume and fallback, provider capabilities, subscriptions, usage limits. It is about orchestration, not devices. Its findings on the event log, snapshot+cursor and the single retry owner match what I read at `b387e3a` (`docs/internals/overview.md:45-73`, `docs/internals/connection-runtime.md:9-35`, `ThreadStream.ts:91-114`), so nothing there is stale.
- **`docs/research/many-machines-2026-11.md` (13 November)** covered T3's multi-machine shape from T3's docs: one environment per machine, four routes, peer links, the machine cap, offline behaviour.
- **New in this study**, read from the code rather than the docs: the "Add to another machine" clone flow and its client-side ignored-file relay (`AddProjectToMachineDialog.tsx`, `lib/addProjectToMachine.ts`); the per-environment folder browser (`CommandPalette.tsx:638-641`); the machine-kind icon set and the "Also on …" project badge; auto balance; the bounded replay-versus-snapshot decision in code (`ThreadStream.ts:91-114`); and the fact that the desktop renderer is not served by the backend (`docs/internals/remote.md:182-192`).
- **Unverified:** whether any of those UI flows existed at `3ff5b3c`. The clone is shallow and I did not compare trees.

## 1.10 T3 scored against the six requirements

| # | T3 | Seen in |
|---|---|---|
| R1 | Met, by design: sessions live on one machine; every device is a window; event replay or snapshot after sleep | `remote.md:3-7`, `connection-runtime.md:9-35,64-107`, `ThreadStream.ts:91-114` |
| R2 | Met for desktop and web; a phone reaches every environment it has been paired with, one connection each | `connection-runtime.md:88-107`, `remote-access.md:167-176` |
| R3 | Met: machine kind and label per row, project "Also on", "From" for link-started work | `SidebarThreadRow.tsx:840-848,1168-1215`, `ProjectEnvironmentBadge.tsx:7-44`, `remote-access.md:385` |
| R4 | Met: add project on a chosen machine (browse that disk), clone onto a chosen machine, auto balance | `CommandPalette.tsx:638-641`, `AddProjectToMachineDialog.tsx:72-239`, `remote-access.md:178-194` |
| R5 | Partial: each environment's own Settings → Sharing & access is reachable from any connected device, including its device list, routes and revocation; there is no single console for all machines at once | `remote-access.md:402-457`, `overview.md:36-43` |
| R6 | Strong: one retry owner, proved-dead sockets, durable intent, background service, tunnel recovery, cached reads offline | `connection-runtime.md:9-35`, `overview.md:45-73`, `t3-connect.md:119-158` |

Continue to `02-retinue-today.md` for Retinue against the same six, and the gap table.
