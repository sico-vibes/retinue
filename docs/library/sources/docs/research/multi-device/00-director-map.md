# Multi-device: the Director's first map (17 November 2026)

> **Status:** working notes, written while Grey researches (`01-t3-vs-retinue.md`). Checked against code on `main` at `b7c3a32` and T3 Code at `b387e3a` (17 November 2026, clone at `C:/Users/Sico Vibes/Desktop/DevCN/_research/t3code`). Grey's report supersedes anything here it contradicts with evidence.

## The owner's bar

Four devices (laptop, home computer, VPS, phone) see the same sessions live; any session can be carried on from any of them; each conversation shows where it runs and where it came from; from the device in hand the owner can start a project or session on another device (clone there, or pick a folder on that disk); everything about the other devices is managed from wherever the owner is; and all of it is easy and reliable.

## How T3 does it (from its docs; code not yet traced)

- **One server per machine, many connections per client.** Each machine runs its own environment server, which owns providers, execution, files and state (`docs/internals/remote.md`). Every client (desktop, hosted web, phone) connects to every environment directly and shows their projects and threads together. A thread always runs where it lives; any client can drive it.
- **Routes, not machine types.** A saved environment has an ordered list of routes (LAN, Tailscale, VPN, public URL, SSH, T3 Connect); the client connects over the first that works, re-checks earlier ones when the network changes and every minute, and learns the server's current LAN and tailnet addresses (`docs/user/remote-access.md`, "Reach one machine several ways"; `packages/client-runtime/src/connection/driver.ts`, `supervisor.ts`).
- **Headless hosts.** `t3 up` runs it as a background service on a server; with no configured host it listens on every IPv4 interface but only accepts loopback, private and tailnet peers (`docs/internals/remote.md`, last part of "Identity is independent of the route").
- **Pairing.** A one-time pairing link or QR per device; optional approval of new devices with a matching code (`remote-access.md`, "Approve new devices").
- **Placing new threads.** The composer picks a machine for a new thread (manually, or "Auto balance"); existing threads stay where they started.
- **Peer links** are a separate thing: one environment's agents working in another's, through its MCP endpoint, fenced by the access the link was paired with.

### What makes T3 feel reliable (`docs/internals/connection-runtime.md`)

- **One retry owner per machine** (`packages/client-runtime/src/connection/supervisor.ts`): jittered exponential backoff capped at five minutes, reset only after a connection stays up; offline and sign-in failures wait for a wake-up instead of retrying blindly.
- **Probe, don't trust "offline"**: on foregrounding, a network change or a retry it probes the live socket; a socket that the OS killed during sleep is detected (no frame within 500 ms, then a fresh request) and replaced within seconds.
- **Transport health and data freshness are separate states**: a socket can be up while a list is still catching up; the UI never says "reconnecting" for a data problem.
- **Cached lists stay readable offline** and never overwrite newer live data; each machine's rows hold still until its own snapshot arrives, so lists don't jump on wake.
- **Live threads stay subscribed** on desktop (a keep-alive mounts every running thread), so opening one needs no replay; a replay cursor is kept five minutes for back navigation.

## Retinue today

| Requirement | What exists | Where | Gap |
|---|---|---|---|
| Same sessions on every device, live | Another machine's bots and conversations are listed in a separate sidebar group, and a conversation opens in a separate panel | `ui/MachineBots.tsx` (`MachineGroups`), `ui/MachineConversation.tsx` | Not live: the panel polls every 3 s, lists every 15 s, the daemon refreshes links every 20 s (`machines.rs:54`). `machine_client.rs` makes single calls and never subscribes to the other machine's events. Remote conversations are not in the main projects tree. |
| Carry on from any device | `machine.prompt`, `machine.steer`, `machine.cancel`, `machine.permission.reply`; `machine.takeOver` moves a conversation here | `machines.rs:895-953`, `handoff.rs` | Works from the desktop only. The phone pairs with one host, and every `machine.*` method is `never` remote (`tiers.json:1769-1814`), so the phone cannot see or drive other machines through it. `tool.changed` is `drop` for devices, so a phone sees messages but not what a session is doing. |
| Where it runs and came from | `MachineBadge`; `link.movedTo` and the `MovedBar` after a takeover | `ui/MachineBots.tsx`, `ui/MachineConversation.tsx` | Badges only inside the machine group; no origin mark on a conversation in the main list. |
| Start a project or session on another device | `machine.start` (a conversation for an existing bot), `machine.projects` (list) | `machines.rs` | No folder picker on the other machine's disk, no clone there, no "new project there". |
| Manage the other devices from here | Pair, rename, remove, refresh, probe | `machines.rs`, Settings | Mostly there for desktops; nothing for the phone. |
| The VPS as a host | None | CI and release build on `windows-latest` only; the daemon has about 42 `cfg(windows)` sites | Retinue cannot run on the Linux VPS. The VPS today runs the cloud computer (Harbor), not a Retinue daemon. **This is the largest gap.** |

## Rules that stand in the way (the owner decides)

- 31 October 2026 amendment: "Only one machine runs a conversation. Another machine's conversations are read-only here. Take over is the only action. Sending a message to another machine's bot isn't part of this." The code has since added `machine.prompt` and friends, so this rule and the code already disagree; the amendment needs updating either way.
- 31 October 2026: "The window never talks to the other machine." Live sync can respect this if our daemon subscribes to each linked machine's event stream and relays it to the window.
- `machine.*` is `never` remote: the phone seeing every machine through one host needs an amendment, or the phone pairs with each host itself (T3's way).

## First guess at the build order (to check against Grey)

1. Live: the daemon subscribes to each linked machine's gateway events and relays them; remote conversations open in the normal chat view, not a panel.
2. One list: every machine's projects and conversations in the main sidebar, each with a machine badge.
3. Start anywhere: pick the machine in the composer or the new-project flow; folder picker and clone on that machine.
4. The phone sees every machine (through its host, or by pairing with each).
5. A headless Linux daemon for the VPS.
