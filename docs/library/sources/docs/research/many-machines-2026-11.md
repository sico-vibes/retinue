# Many machines in one Retinue

Research for the owner, through the Director. Read-only: no code was changed, nothing was committed.

Date: 13 November 2026. Worktree branch: `retinue/research-for-the-owner-through-the`.
Base commit at the time of writing: `4a59fea` (`docs: record the v0.2.0-beta.1 release`).

The question: the owner has a Windows laptop, a Windows home PC and a Linux VPS. In T3 Code all three
are connected and work can be started on any of them from one window. In Herdr all three are connected
over SSH and Tailscale. The owner wants the same in Retinue: link many machines and let employees work
on the VPS and the home PC, over Tailscale (already on the laptop and the PC, not yet on the VPS).

Sections follow the owner's questions in order. Every claim about Retinue code carries a `path:line`.
Every claim about another product carries a link. Where I could not confirm something, it says so.

---

## 1. How T3 Code does it

Sources: the repository is public, [pingdotgg/t3code](https://github.com/pingdotgg/t3code) (MIT). The two
documents that matter are
[docs/user/remote-access.md](https://github.com/pingdotgg/t3code/blob/main/docs/user/remote-access.md)
and the internals [docs/internals/remote.md](https://github.com/pingdotgg/t3code/blob/main/docs/internals/remote.md)
and [docs/internals/t3-connect.md](https://github.com/pingdotgg/t3code/blob/main/docs/internals/t3-connect.md).

**What runs on each machine.** T3 Code is a client plus a *server*, called an *environment*. The
environment owns the coding-agent processes, terminals, Git, project files and the durable state; the
desktop app, web app and phone apps are clients that speak authenticated RPC to it
(`docs/internals/overview.md`: "T3 Code keeps execution in the environment that owns the workspace").
A single machine can run the server as a desktop app, as a background service (`t3 up`), or from a
terminal (`t3 serve`). The desktop app bundles a server by default; **Settings → Environments → Local
environment** off turns that machine into a client only.

**How the window reaches it.** Four routes, and the same server is behind all of them:

| Route | What it is | Evidence |
|---|---|---|
| T3 Connect | Their own hosted relay plus **Cloudflare tunnels** | `docs/internals/t3-connect.md`: the host runs `cloudflared` pinned by `CLOUDFLARED_VERSION` in `packages/shared/src/relayClient.ts`; "Cloudflare bills a tunnel whether or not a connector is attached" |
| LAN or private network | Pairing code over the host's network address; also covers a tailnet address | `docs/user/remote-access.md`, "Pair over a LAN or private network" |
| Tailscale HTTPS | `tailscale serve` on the host, `t3 serve --tailscale-serve` | same doc, "Tailscale HTTPS" |
| Desktop-managed SSH | Desktop spawns `ssh`, installs the server to `~/.t3/runtime`, opens a port forward | same doc, "Desktop-managed SSH"; `packages/ssh/src/tunnel.ts` |

So the owner's impression is right: **T3 Connect is Cloudflare tunnels.** The relay is a Worker that
issues a bootstrap credential; after that the client talks to the environment's tunnel hostname
directly, so the relay does not proxy the traffic (`docs/internals/t3-connect.md`). Free accounts link
3 machines; a paid T3 Chat plan raises it to 10 or 100 (`docs/user/remote-access.md`, "More machines with
T3 Chat").

**How a machine is added.** Two ways. T3 Connect: sign in on the host and on the other device with the
same account, then choose the environment; on a headless host `t3 up` prints a browser link and a code
to approve from any device. Direct pairing: a one-time pairing link or QR code (`t3 pair`), opened in
"Add environment" on the receiving app, which then holds a token for that machine. Each machine can
carry several routes and the client connects over the first that answers, preferring LAN, then
tailnet, then T3 Connect, and moving back when a faster route returns.

**What a user can do with a remote machine.** Everything the client can do locally: start a thread,
send messages, answer permission questions, stop a run, use terminals, browse and edit files (with the
`filesystem:*` scopes), Git and pull requests, and **browser tabs that belong to the environment**, so
the owner and the agent see the same tabs, with `localhost` on the host reachable
(`docs/user/remote-access.md`, "Browser on a remote environment"). Permissions are scopes
(`orchestration:read/operate`, `terminal:operate`, `filesystem:read/write`, `source-control:write`,
`relay:read`, and separately access management), chosen per pairing.

**Starting work on another machine from one window.** This is their *Linked environments* beta, and it
is the closest analogue to the owner's ask:

- Link two machines in **Settings → Environments → Linked environments**; a pasted pairing code is
  needed for an address the app does not already know.
- "agents on one can work on the other: launch, message, wait on and stop its threads, answer their
  questions, and manage its queues, projects and scheduled tasks."
- Access is capped: "The access you pick is the most agents from this machine may do there. An agent
  also never gets more there than its own mode here." Threads a link starts are marked **From**.
- Below full access, linked agents may change only threads the link started, never that machine's own
  threads, projects or settings, and they skip its project setup scripts.
- An agent hands work over with a `delegate_task` tool: the task runs there as an ordinary thread in the
  same repository, and the parent thread wakes with the result.
- The link is one more OAuth client on the receiving machine, listed in its Sharing & access, revocable
  there. Links last 30 days.
- Internally the linking environment signs in to the other's `/mcp` as an outside MCP client and keeps
  that token; every forwarded call carries the caller's mode limit in a `T3-Mode-Limit` header
  (`docs/internals/remote.md`, "Peer links"; `apps/server/src/peer/PeerLinks.ts`).

**When a machine is offline.** The saved connection keeps its routes and shows as offline; the client
tries the remaining routes, then retries. T3 Connect reclaims an idle tunnel after five minutes down
and recreates it on the host's next start, at the same hostname, so no re-pairing is needed
(`docs/internals/t3-connect.md`, "Idle tunnels are reclaimed and recovered"). A task already running on
a linked machine still finishes if that machine has links off, and its result arrives when the link is
back on.

**Not confirmed.** I did not read the phone app or run any of it, so "works" here means "the public docs
say so". I did not verify that the Cloudflare tunnel is reachable from a network that blocks Cloudflare,
and I did not check what T3 Connect costs beyond the machine caps.

---

## 2. How Herdr does it

Sources: [herdrdev/herdr](https://github.com/herdrdev/herdr) (Apache-2.0, Rust, one binary) and its docs
[Connecting machines](https://herdr.dev/docs/connecting-machines/) and
[Persistence and remote access](https://herdr.dev/docs/persistence-remote/).

**What runs on each machine.** Herdr is a terminal multiplexer for coding agents: a *server* owns the
panes and the agent processes, a *client* draws the UI. "Each machine keeps its own Herdr server,
sessions, and running processes." The server keeps panes running after the client detaches, ignores
`SIGHUP`, and restores the layout after a restart (it can resume supported agent sessions; the original
processes do not survive).

**How the window reaches it.** **SSH only.** There is no relay, no tunnel service and no Tailscale
integration in the product; Tailscale appears only as a way to reach the SSH target. Two shapes:

- `herdr machine add workbox` saves an SSH target (a host alias from `ssh_config`, or
  `ssh://you@server:2222`). The saved profile holds "only an opaque ID, label, SSH target, explicit
  remote session, and enabled state"; **Herdr stores no passwords, keys or agent tickets, and
  authentication stays with OpenSSH.**
- `herdr --remote workbox` attaches to a server on the host and streams its terminal content over SSH.

Setup discovers a running server on the host, and offers to install or update the remote package when
the session is interactive; background reconnects only discover what is installed and never install,
update or restart anything. It requests SSH compression, and on Linux and macOS clients it shares one
OpenSSH connection (`ControlPersist 600`).

**How a machine is added.** One command on the client, `herdr machine add <ssh-target>`, after plain
`ssh` already works. No pairing code, no account, no token: the trust is the SSH host key and the SSH
login. Renaming, disabling and removing are local (`herdr machine list|rename|disable|remove`); removing
only disconnects the client and leaves the remote sessions running.

**What a user can do with a remote machine.** Switch to it in the sidebar, run workspaces, tabs, panes
and agents there, see every pane marked working, blocked or idle, and use the CLI against it
(`herdr --machine <label> agent list`). Input and terminal size go to the selected machine; other
connected machines keep updating their agent states and notifications without streaming screens.
Panes are the unit of work, so "files, terminals, previews" means a terminal in a pane, not an editor
or a browser tab.

**When a machine is offline.** The last workspace and agent state stays visible but dimmed and input is
disabled until a fresh connection; reconnecting never takes the selection away from the machine in use.
Retries back off to two minutes. A machine needing attention shows `! auth` or `! error` and the rest
stay usable. Bridge idle cleanup closes a quiet remote bridge after a minute and leaves the remote
server, sessions and panes running.

**The model in one line.** Herdr has no notion of "start this agent's task over there from here"; it has
*saved SSH terminals*, and the agents inside them are the remote machine's own. That is enough for the
owner's three machines because the owner is willing to keep a Herdr server on each and to reach each
over SSH.

---

## 3. Others worth comparing

Four models appear across the products, and the differences that matter for Retinue are *where the
agent process runs* and *what carries the bytes*.

| Product | Where the agent/work runs | Transport | How a machine is added | Model |
|---|---|---|---|---|
| T3 Code | A server ("environment") on each machine; the client is a renderer | T3 Connect (Cloudflare tunnel + relay), LAN pairing, Tailscale Serve, SSH port-forward | Sign-in to their account, or a one-time pairing code | Agent on the remote machine, reached directly |
| Herdr | A Herdr server with panes on each machine | SSH only | `herdr machine add <ssh-target>` after plain `ssh` works | Terminal multiplexer on the remote, reached over SSH |
| [Zed remote development](https://zed.dev/docs/remote-development) | A headless `remote_server` on the host; the UI, Tree-sitter and model calls stay local | SSH (one ControlMaster per project) | An SSH command in the "Remote Projects" dialog | Split: UI local, language servers/terminals/tasks remote |
| [VS Code Remote-SSH](https://code.visualstudio.com/docs/remote/ssh) | "VS Code Server" installed on the host; extensions run there | SSH tunnel | Pick or add an SSH host | Server on the remote, thin client |
| [Coder](https://coder.com/docs/user-guides/workspace-access) | Workspaces (containers/VMs) with a Coder agent, managed by a self-hosted control plane | The Coder server (its own tunnel to the agent), plus `coder ssh`, `coder config-ssh`, Coder Desktop VPN | Sign in to the Coder deployment; workspaces are created from templates | Agent on the remote, control plane in the middle |
| [Tailscale SSH](https://tailscale.com/kb/1193/tailscale-ssh) | Nothing extra: the host's own shell | Tailscale's own SSH server on the tailnet, authenticated by tailnet identity | `tailscale set --ssh` on the host plus a policy rule | No agent; just authenticated SSH |

Two lessons from this table:

1. **Every product that starts work on another machine puts something of its own on that machine.** T3
   Code installs a server, Herdr installs its server, Zed installs `remote_server`, VS Code installs
   VS Code Server, Coder installs an agent. None of them drives a coding agent purely over SSH from the
   laptop. That is the pattern, and it is the reason Retinue's existing machine link is the right base.
2. **The transport is replaceable; the identity is not.** T3 Code lets the same environment be reached
   by four routes and keeps one identity, so the route can be chosen by speed. That is worth copying in
   spirit: Tailscale now, something else later, without re-pairing.

---

## 4. Retinue today against the owner's ask

### What is built

A machine link is **this daemon acting as a paired device of the other machine's Remote gateway**. There
is no new listener, no relay and no second protocol (`crates/retinue-daemon/src/machines.rs:1-11`,
`docs/remote/THREAT_MODEL.md` section 16). The pieces:

| Piece | Where | What it does |
|---|---|---|
| Identity | `machines.rs:147-180` | `machine.id` (a UUID kept in settings) and `machine.name` (owner's choice or host name); `machine.info` reports both plus the handoff version |
| Pairing | `machines.rs:553-736` | Probe the address (`GET /api/ping`), then either a one-time code over web PKI or the other machine's password over a pinned certificate; the other owner approves with a tier; the device token is written to a user-only file |
| Token | `machine_client.rs:341-377` | `<data>/machines/<machine id>.token`, user-only permissions, read for one request, never in the database, a backup or a log |
| TLS | `machine_client.rs:231-256` | `https` only, no redirects, no proxy, 32 MiB cap, 10 s default; web PKI for Tailscale, SHA-256 pin for Native |
| Reading | `machines.rs:328-400` | A 20 s loop reads `machine.info`, `agent.list` and `session.list` per machine, in parallel; state becomes `online`, `offline` or `revoked` |
| Bots of another machine | `machines.rs:277-315` | Bots with name, colour, art, role, provider, model, busy flag, active conversation, count |
| Conversations | `machines.rs:811-820` | `machine.conversations` and `machine.conversation` pass `agent.conversations` and `session.get` through the gateway |
| Take over | `handoff.rs:1-20` and `handoff.rs:186-192` | `handoff.preview/prepare/commit/abort` on the giving machine, `machine.takeOver` on the taking machine: commit and push the conversation's own `retinue/` branch, fetch and import here, flip ownership at `commit` |
| UI | `apps/desktop/src/ui/MachinesPage.tsx`, `MachineBots.tsx`, `MachineConversation.tsx` | Machines page, a bot row per machine, a read-only conversation view with "Take over" |

### What works for "employees work on another machine"

- **Watching works.** The laptop lists the home PC's bots and their conversations, shows which is busy,
  and shows a busy badge in the companion (`MachineConversation.tsx:142`).
- **Moving a conversation works, in one direction and with a push.** Take over requires the link to be
  at tier `operator` and online (`apps/desktop/src/ui/machines.ts:118`), the conversation must have its
  own copy (a worktree) or a remote, and the move commits and pushes its `retinue/` branch
  (`handoff.rs:1-20`). Pictures and tool steps stay behind (`MachineConversation.tsx:94`).
- **Starting work on the other machine does not work today.** The whole `machine.*` surface is
  `machines.rs:794-822` plus `handoff.rs:186-192`; there is no `machine.prompt`, `machine.start`, no
  project or file read, and no terminal. The UI has no composer on a machine's conversation: it renders
  messages read-only and offers Take over (`MachineConversation.tsx:222-228`).

### The gap, stated exactly

The gateway on the other machine **already allows** a paired `chat` device to start a conversation and
send a message: `agent.conversation.create`, `session.prompt`, `session.steer`, `session.cancel`,
`permission.reply` and the queue methods are all rows in `apps/desktop/src/remote/tiers.json` (classes
`chat` and `approve`). What is missing is not permission on the far side but **the local half**: no
`machine.*` method passes those calls through, and no UI offers them. Everything a phone can already do
to a host, a linked machine could do, if the laptop's daemon forwarded the call and the window had a
composer.

Also missing for a real "work there" experience:

- **Project and file visibility.** `project.list` is `read/viewer` on the gateway, but no `machine.*`
  method exposes it, so the laptop cannot show a project list, a diff or a file.
- **A bot that does not exist yet.** Starting a conversation needs an agent on that machine, or
  `agent.create` (which is `never` for remote devices, and should stay so).
- **Two small inconsistencies found while reading.** The take-over note tells the owner to "set this
  machine to Chat" (`MachineConversation.tsx:34`) while the rule it is explaining requires `operator`
  (`machines.ts:118`, `tiers.json` `handoff.*`). And `machine.*` is `never` for paired devices
  (`tiers.json`), so a phone cannot drive a machine link: that is deliberate, and worth keeping.

**What would be missing for the laptop to send work to the home PC or the VPS:** a Linux daemon on the
VPS (section 5), a way to start a conversation on a linked machine and steer it (section 6, option A),
and a route to the VPS over Tailscale (section 7).

---

## 5. Linux: can `retinued` run headless today?

### Short answer

**Not today, and not because of the window.** The daemon is a separate binary (`retinued`,
`crates/retinue-daemon/Cargo.toml`) that the desktop shell spawns with `RETINUE_PARENT_PID`
(`apps/desktop/src-tauri/src/lib.rs:391`); it runs without a parent when that variable is absent
(`main.rs:5916`: "Keep the sender alive when launched without a shell"). It has its own HTTP API, its own
Remote gateway and its own storage, so the Tauri shell is not required. What blocks it is a set of
Windows-only pieces and one missing default.

### What is Windows-only, with evidence

| Area | Evidence | Effect on Linux |
|---|---|---|
| Data folder default | `main.rs:5918-5925`: `RETINUE_DB_PATH`, else `%LOCALAPPDATA%/Retinue/retinue.db`, else the temp dir | Without the variable it writes to the temp dir. Needs an XDG default |
| Secret store | `secrets.rs:1-14` and `secrets.rs:174`: Windows Credential Manager only; the non-Windows backend returns an error for every call | Bot secrets, webhook secrets, Harbor token and the one-use secret flow all fail on Linux. Needs a decision (a 0600 file, or libsecret) |
| Tailscale CLI discovery | `remote/tailscale.rs:79-89`: looks for `tailscale.exe` under Program Files | The Tailscale Remote method cannot start on Linux as written; needs a `PATH` lookup |
| Native firewall rule | `remote/native.rs:382`: Windows prompt path | Not needed if the VPS uses the Tailscale method |
| Port discovery (preview) | `preview.rs:364` and `preview.rs:399`: `netstat` and the process table are stubs off Windows | Previews lose the "which port is this" list; nothing breaks |
| Process trees | `retinue-process/src/lib.rs:125-205`: a real Unix branch (process group, `kill(-pgid, 9)`) | Works |
| File permissions on token/key files | `remote/native.rs:382` area has a Unix branch (`chmod 0600`) | Works |
| Dictation (Whistle) | `retinue-speech/src/needle.rs:1-6`: Cactus's prebuilt `needle.exe` | Dictation stays off on Linux; acceptable on a VPS |
| Hook bridge | `tools/retinue-hook/src/main.rs`; installed as `retinue-hook.exe` (`main.rs:14229`) | Needs a Linux build and Unix quoting in the hook command |
| WSL support | crate `retinue-wsl` in the workspace | Windows-only feature; leave it out |
| CI | `.github/workflows/ci.yml:26`: `windows-latest` only | Nothing proves a Linux build. A Linux job is part of the work |

### How much a Linux build of only the daemon needs

Sized for one backend lane, in this order:

1. **Make it build and start.** A `cfg(unix)` data default (XDG data dir), a Linux CI job that runs
   `cargo build -p retinue-daemon` and the daemon's tests, and a `systemd` unit example. Small.
2. **Tailscale CLI discovery by `PATH`.** `remote/tailscale.rs` only. Small, and it unlocks the whole
   Remote gateway on the VPS.
3. **A secret backend.** This is the one real design decision: the existing rule is that secrets live in
   the OS store and never in a file. On Linux there is no equivalent store without a session bus, so
   either (a) a 0600 file under `<data>/secrets/` with the same "never in the database, a backup or a
   log" rules, or (b) declare bot secrets unsupported on a headless host. Either way it must be written
   down in `AGENTS.md` as an exception or a limitation, not slipped in.
4. **Hook bridge for Linux.** Build `retinue-hook` for Linux and quote the command for `sh`. Small.
5. **Optional, later:** preview port discovery via `/proc`, dictation, and a Linux release artefact.

Steps 1, 2 and 4 are mechanical. Step 3 is the only part that needs the owner's decision. A Linux
daemon without bot secrets still runs conversations, which is what the VPS is for.

---

## 6. Options for Retinue

Before the options, one fact that shapes all of them: **pairing a device needs the host's owner to
approve it, and today only a window can.** `remote.pair.begin` makes a code and `remote.pair.confirm`
approves it with a tier (`crates/retinue-daemon/src/remote/mod.rs:1070-1145`); both are daemon RPCs the
desktop window calls, and `remote.*` is `never` for paired devices (`tiers.json`). The daemon's
capability token is handed once to the shell that launched it, as a `retinued.ready` line on stdout
(`main.rs:6089-6100`), so a headless host has **no supported way** to approve anything: an operator
could in principle capture that line from the service's output and call the RPC by hand over loopback,
but that puts the capability in a log, which the rules forbid in spirit and in practice. That is true
for a phone and for a machine link alike, and it is the first thing that has to change for the VPS. The
natural shape is a console approval: the same decision, the same tier choice, on the host's own
terminal (`retinued pair`), with the code printed and never logged.

### A. A Retinue daemon on every machine, linked over Tailscale, extended so the window can start and steer work there

**What runs where.** `retinued` and the coding-agent CLIs on the laptop, the home PC and the VPS. Each
machine keeps its own projects, sign-ins, conversations and storage. The laptop's window talks only to
its own daemon; that daemon talks to the others' gateways over Tailscale, as it does now.

**How a machine is added.** As today: the laptop probes `https://<host>.tailnet.ts.net`, pairs with a
code (or the host's password over a pinned certificate), and the host's owner approves with a tier
(`machines.rs:553-736`). On a headless host, through the new console approval.

**Security against `AGENTS.md`.**

- No new listener: the gateway stays the only entry, on loopback, published by `tailscale serve`
  (`remote/mod.rs:11`, `remote/web.rs:5`). Nothing new binds.
- Tiers do the work: starting and steering a conversation is `chat`; moving one stays `operator` and
  only for a `machine` device (`tiers.json`, `handoff.*`). Reading stays `viewer`.
- No copied provider sign-ins: each machine signs its own CLIs in; no token, key or credential crosses
  the link. The link's token is the machine's own device token, in `<data>/machines/`, read per request
  (`machine_client.rs:341-377`).
- One conversation, one machine: `refuse_if_moved` and the handoff lease already enforce this
  (`machines.rs:197-216`).
- Redaction still applies: everything the laptop sees came through the gateway's redaction, and events
  are reloaded rather than streamed with content (`machines.rs:380-384`).

**Work involved.**

1. Linux daemon (section 5): build, data default, Tailscale `PATH` lookup, secrets decision, hook.
2. Headless approval: a console pairing/approval path on the host.
3. New `machine.*` pass-throughs on the laptop's daemon: start a conversation (`agent.conversation.create`),
   prompt (`session.prompt`), steer, cancel and list projects (`project.list`) at `chat`; answer a
   permission (`permission.reply`) at `operator`, because that is the far side's class
   (`apps/desktop/src/remote/tiers.json`). Each is a thin wrapper like `read_through`
   (`machines.rs:473-476`), with a tier check and the existing `failure()` mapping.
4. UI: a composer in `MachineConversation.tsx` and a "New conversation" action on a machine bot row;
   the read-only banner becomes a "runs on <machine>" label.
5. Live updates: the machine client has no event socket (`machine_client.rs` sends and RPCs only), so
   either poll the open conversation or add an event relay. Polling the open conversation is the
   smaller first step and matches the existing read-through.
6. Tests: daemon RPC tests against a fake gateway, mounted UI tests, and the tiers parity test already
   pins the far side.

**What the owner would see.** The same Machines page, but a bot on the home PC or the VPS opens a
conversation she can type in, with "New conversation" next to the bot. Take over stays where it is. The
home PC needs no Linux work, so this half can ship before the VPS does.

**Risks.** The laptop now drives real work on another machine, so a bug is not cosmetic: a wrong tier
check could let a `viewer` start work. The far side is the authority, so the tier check must be
enforced by the gateway (it is) and the laptop must not cache a stale tier (it re-reads `/api/me` every
20 s, `machines.rs:374-376`). Streaming is a design decision, not free.

### B. SSH only: no daemon on the remote, this machine's daemon runs the agent over SSH

**What runs where.** Only an SSH server and the agent CLIs on the remote. The laptop's daemon spawns
`ssh host -- claude` (or codex/opencode) and speaks the agent's stdio protocol over that pipe. The
adapters already use stdin/stdout protocols (`crates/retinue-adapter-*`), so the pipe itself is
plausible.

**How a machine is added.** An SSH target and a key, or Tailscale SSH and a policy rule. No pairing, no
tier, no owner approval.

**Why it does not fit Retinue.** Everything else in the daemon assumes the files are on this machine:

| Feature | Assumption | Evidence |
|---|---|---|
| Worktrees, diffs, checkpoints | `git` runs locally against a local path | `repository.rs`, `worktree.rs`, `checkpoints.rs` |
| Project setup and clone | local scripts, local clone | `project_setup.rs`, `project_clone.rs` |
| Images, previews, file drops | local files, local ports | `media.rs`, `preview.rs` |
| Take over / handoff | a conversation's copy and branch on both machines | `handoff.rs:1-20` |

A conversation over SSH would show text and tool steps but no diff, no file tree, no preview, no
checkpoint, and handoff would have nothing to move. It would also need a new transport in the adapter
layer, SSH authentication from a background process (keys without a passphrase, or Tailscale SSH), and a
second meaning for "the conversation's folder" that is not a local path. It is the Herdr model without
Herdr's terminal, and it contradicts the shape the product already has.

**Security.** No new listener and no new credential store, which is in its favour, but it also has no
tier, no per-device revocation and no audit line, so it is *less* controlled than A, not more.

### C. Options I would not choose, and one worth knowing

- **C1. The window talks to the other daemon's API directly.** Refused: it needs a second listener and
  it would hand the window the capability token, which the architecture forbids.
- **C2. A relay or tunnel service like T3 Connect.** Refused: "no Retinue relay", no third-party
  service. Tailscale already does this job, and the owner has it.
- **C3. Daemon inside the Harbor container on the VPS.** Possible but worse than on the host: the
  container's ports are Harbor's, its lifetime is Harbor's (sleep/wake), and a conversation's files
  would vanish with the container. Worth knowing: Harbor stays the "employee's own computer" for GUI and
  browser work; Retinue's daemon belongs on the host, next to the repositories.
- **C4. Nothing new at all: the browser.** The gateway already serves a full web client when
  `RETINUE_REMOTE_WEB_DIR` points at the built remote app (`remote/web.rs`, `StaticFiles::from_env`;
  built by `npm run build:remote`, `apps/desktop/package.json`; the desktop sets the variable at
  `apps/desktop/src-tauri/src/lib.rs:415`). So once a Linux daemon runs on the VPS with Tailscale and
  Remote on, **the laptop can open `https://<vps>.tailnet.ts.net` in a browser and drive that machine
  today**, with no new Retinue feature. That is not one window, but it is the cheapest possible proof
  that the daemon works there, and it makes the Linux work worth doing on its own.

### Recommendation between them

**A**, and C4 as its first milestone. Reasons: it is the only option that keeps every existing
guarantee (tier, one conversation per machine, no new listener, no copied sign-ins), it reuses the
pairing, token and TLS work that already exists, and it degrades the same way the phone path does. B
would look quick and then hollow out diffs, worktrees, previews and handoff. C1 and C2 break the
architecture; C3 puts a daemon somewhere Harbor can delete.

---

## 7. The VPS specifically

**Order of installation, for option A.**

1. **Tailscale on the VPS.** Install and `tailscale up`; the VPS joins the tailnet. No `tailscale
   serve` by hand: Retinue runs it when Remote is turned on (`remote/tailscale.rs`, `remote/mod.rs:831`),
   and it is `serve`, never `funnel` (`AGENTS.md`, 17 October amendment).
2. **A Linux `retinued`** (section 5), plus the remote web build for C4. A `systemd` unit that sets
   `RETINUE_DB_PATH` and `RETINUE_REMOTE_WEB_DIR`, runs as a non-root user, and restarts on failure.
3. **Remote on, Tailscale method, then one device paired.** Today that last step needs a window; with
   the console approval it needs only an SSH session. The laptop pairs as a `machine` at tier `chat`,
   or `operator` if take over is wanted.
4. **The coding-agent CLIs on the VPS, each signed in there.** Claude Code, Codex, OpenCode: whatever
   the employees should use. This is the point of the "no copied provider sign-ins" rule: the VPS
   signs itself in, and the laptop never holds those credentials.
5. **A project on the VPS.** The repositories the employees should work in, cloned on the VPS.

**What is not needed on the VPS:** a window, Tauri, WebView2, a desktop session, or the Windows
credential store. Dictation and previews lose their Windows-only parts, which is acceptable on a server.

**Next to Harbor's bot computer.** `crates/retinue-daemon/src/computer.rs` links one Harbor-managed
Linux desktop container and gives employees `vps-computer` MCP tools for it: status, wake/sleep, exec,
routines, logs, authorised by a Harbor token held in the credential store (`computer.rs:1-24`). It is
**desktop only** (every method `never` in `tiers.json`), and it is a *tool*, not a place where a
conversation runs. The two fit together without overlapping:

- Harbor's container: the employee's own desktop, browser and GUI work, reached through MCP tools from
  a conversation running anywhere.
- Retinue's daemon on the VPS host: a place where a conversation runs, with a repository, a worktree
  and its own CLIs.

The one thing to avoid is running the daemon *inside* Harbor's container (C3): the container sleeps,
and a conversation's files and processes would sleep with it.

---

## Recommendation

**Option A, in three steps, with C4 as a free check along the way.**

1. **Make the linked machine a place to work, not only a place to look.** Add the daemon half of
   "start and steer there": pass-throughs for starting a conversation, prompting, steering, cancelling,
   answering a permission and listing projects, each gated by the far side's own class (`chat`, and
   `operator` for a permission answer), and refused when the link is offline or revoked. The Director
   then adds the composer and the "New conversation" action. This works between the laptop and the home
   PC with no Linux work at all, and it is the same code the VPS needs.
2. **A Linux daemon.** Build and CI, XDG data default, Tailscale by `PATH`, a Linux hook, and a
   decision about bot secrets on a host with no credential store.
3. **Headless approval.** A console pairing/approval path on the host (`retinued pair`: print the code,
   approve with a tier, never log it), because `remote.pair.confirm` is a window RPC today and the
   capability only reaches the launching shell (`main.rs:6089-6100`).

Then, and only then, the VPS joins the tailnet and the laptop pairs to it.

**Why not B.** SSH-only looks like less work and is more: it would hollow out diffs, worktrees,
checkpoints, previews and handoff, and it carries no tier, no revocation and no audit. Herdr can do it
because Herdr's unit of work is a terminal; Retinue's is a conversation with a repository.

**Why C4 first is worth it.** The gateway already serves the remote web client
(`remote/web.rs`, `StaticFiles::from_env`), so a browser on the laptop can drive the VPS the day the
Linux daemon starts. That proves the daemon before any machine-link work depends on it.

### The first piece of work, sized for one backend lane

> **Task.** In `crates/retinue-daemon/src/machines.rs`, extend the `machine.*` RPC table
> (`machines.rs:794-822`) with pass-throughs to a linked machine's gateway:
> `machine.start` → `agent.conversation.create`, `machine.prompt` → `session.prompt`,
> `machine.steer` → `session.steer`, `machine.cancel` → `session.cancel`,
> `machine.permission.reply` → `permission.reply`, and `machine.projects` → `project.list`.
> Each takes `machineId` plus the far side's parameters, uses `read_through`'s shape
> (`machines.rs:473-476`) and `failure()` (`machines.rs:443-471`) for errors, refuses when the link is
> offline or revoked with the existing messages, and refuses when the stored tier is below the method's
> class: `chat` for start, prompt, steer, cancel and projects; `operator` for `permission.reply`,
> matching `apps/desktop/src/remote/tiers.json`. Add the tier check in one helper, mirroring
> `apps/desktop/src/ui/machines.ts:118`.
> Do not add a listener, do not touch `handoff.rs`, and do not change `tiers.json`: the far side already
> has these rows at `chat`/`approve`.
> **Do not touch:** `crates/retinue-daemon/src/remote/**`, `handoff.rs`, `apps/desktop/src/ui/**`.
> **Rules.** No commit. Gates: `cargo test -p retinue-daemon`, `cargo clippy -p retinue-daemon`, and
> the workspace typecheck if anything in TypeScript changes (it should not).
> **Report.** The methods added, the tier refusal's exact code and message, the tests added, and any
> place where the far side's answer shape did not match what the window would need.

That is one file plus its test module, and it unblocks the UI half, which the Director does.

---

## What I could not confirm

- **T3 Code's phone app and desktop app behaviour.** Everything above comes from the public docs and
  the repository's own documentation; I did not run any of it, and I did not read the client code that
  renders a remote environment beyond the docs' description.
- **What T3 Connect costs** beyond the machine caps. The docs state the caps (3 free, 10 or 100 with a
  paid plan) and that Cloudflare bills per tunnel, not a price.
- **Herdr over Tailscale.** Herdr's docs describe SSH only and never mention Tailscale. The owner's
  "SSH and Tailscale" is most likely Tailscale as the network that makes `ssh workbox` reach the other
  machines; I found nothing in Herdr that talks to Tailscale itself.
- **Whether the VPS's provider allows `tailscale serve` and tailnet traffic.** Not checked. Neither needs
  an inbound port, which is the point of the method.
- **Whether a Linux build of `retinue-daemon` compiles today.** I did not build it (this session is
  read-only on code and Windows-only CI). The `cfg(not(windows))` branches I read suggest it was
  considered; the stubs in `preview.rs` and the missing Tailscale `PATH` lookup say it was never
  finished.

## Proposed skill update (this sandbox is read-only for code, so the Director applies it)

Add to `.claude/skills/retinue-development/references/lessons.md`, dated 2026-11-13:

> **Machine links are read-only plus handoff, and the gap is local.** The gateway already allows a
> `chat` device to `agent.conversation.create`, `session.prompt` and `session.steer`
> (`apps/desktop/src/remote/tiers.json`), but `crates/retinue-daemon/src/machines.rs:794-822` exposes
> only reads and take over. Adding a remote composer is a pass-through in `machines.rs`, not a gateway
> change. Two traps found while reading: pairing needs `remote.pair.confirm`, a window RPC
> (`remote/mod.rs:1070-1145`), so a headless host cannot approve a device; and the take-over note says
> "set this machine to Chat" (`MachineConversation.tsx:34`) while `canTakeOver` requires `operator`
> (`machines.ts:118`).

And to `references/codebase-map.md`, under the remote entry: the gateway serves the phone web client
from `RETINUE_REMOTE_WEB_DIR` (`remote/web.rs`, `StaticFiles::from_env`), built by
`npm run build:remote`; the desktop shell sets it at `apps/desktop/src-tauri/src/lib.rs:415`.


