# Retinue on many machines

> **Status:** current, 17 November 2026. The owner's top priority before the next release: the laptop, the home computer, the VPS and the phone see the same sessions live, any of them carries one on, and work starts on any machine from the device in hand, as in T3 Code.
> **Rules:** `AGENTS.md`, "Amendment (17 November 2026): Retinue on many machines".
> **Research:** `docs/research/multi-device/` (Grey's `01` to `03`, the Director's `00`). T3 Code read at `b387e3a` (MIT).
> **Lanes:** Ada lanes build the backend in their own worktrees; the Director builds every screen, reviews each diff, runs the gates and commits. Each lane's contract and report is `docs/reports/multi-device-<lane>.md` in its worktree.

## Owner decisions (17 November 2026)

| Question | Decision |
|---|---|
| The phone on other machines | Watch, start and steer through its host, at the lower of the phone's and the link's tier. No link or device management from the phone. |
| The work log between machines | Full log for device kind `machine`; phones keep today's redaction. |
| Secrets on the VPS | As T3's server: owner-only files (`0700` folder, `0600` files); the Secret Service keyring on a Linux desktop. |
| New projects on another machine | From any of the owner's paired devices, any URL; folder listing confined to a base folder the host sets; off until turned on; logged. |
| Managing another machine's devices | From the owner's PCs only (kind `machine`, `operator`); never from the phone. |
| The list | Machine groups stay; a merged list is an option. |
| Offline machines | Last known conversations, greyed and read-only. |

## Lanes

| Worktree | Lane | Design piece | Owner rule it relies on |
|---|---|---|---|
| `-15` | Live link, machine sessions, "started from", full work log for machines, offline rows | 1, 2, 3 | none for 1 to 3; work log and offline rows from the amendment |
| `-16` | Headless Linux daemon, console pairing, Linux CI, install guide | 8, console part of 7 | VPS secrets |
| `-17` | The phone through its host | 5 | phone reach |
| `-18` | Manage another machine's devices | 7 | managing devices |
| `-19` | New project, folder and clone on another machine | 6 | new projects |

## Screens (Director)

1. The machine conversation opens live (no 3-second polling), with the full work log, an approval card with Allow and Deny, and a "live" or "polling" mark.
2. Machine groups gain a status line, "Started from <machine>" on rows and in the header, and greyed last-known rows when a machine is offline; an optional "All machines" filter shows one merged list.
3. "New project on <machine>": choose a folder under its base folder or clone a URL there, with progress (T3's dialog shape, in our design language).
4. Machines page: "Manage <machine>": version, Remote state, devices, activity, revoke, approve a waiting device.
5. Phone: a machine picker at the top of the roster, "Through <host>", the machine named on every row and refusal.
6. Settings on the host: the base folder for new projects and its switch.

## Diagnose and test through the Retinue MCP (owner, 17 November 2026)

The owner wants the Director, or any agent, to diagnose and test all of this end to end through the Retinue MCP. Once the lanes above land, one lane adds MCP tools that wrap their RPCs:
- **State:** every machine with its state, link liveness and last sequence, version, OS and tier; one machine's conversations, including the last known rows when it is offline.
- **One remote conversation, end to end:** start a conversation on a named machine with a named bot (optionally in a project), send a message, wait for the reply, and read the transcript with its work log.
- **Approvals:** list a machine's pending approvals and answer one.
- **Projects:** browse the shared folder, register a folder, clone, and read the clone's progress.
- **Devices:** a machine's devices and its waiting pairing request.
- **A diagnostic report:** for each link, whether it is reachable, live, which tier it holds and what the far side refuses, each with the plain sentence the UI would show.

These keep the external MCP's tier rules: reads at its read level, starting and messaging at its chat level, answering and managing at operator. Nothing passes a token, a path outside the shared folder, or message text into logs.

## Done means

On the laptop with the home computer and the VPS linked, and the phone paired with the laptop:
- A conversation started on the PC from the laptop shows each message and step on the laptop within about a second, and on the phone.
- An approval raised on the PC is answered from the laptop.
- A project is cloned onto the VPS from the laptop and a conversation runs there.
- The phone picks the VPS, opens that conversation and sends a message.
- Unplugging the PC's network greys its conversations within 20 seconds; they recover on their own.
- A lost phone is revoked at the PC from the laptop.

The Director runs this checklist with the owner on the real machines before the release.
