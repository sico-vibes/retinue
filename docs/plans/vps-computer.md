# Retinue VPS computer: one 24/7 bot desktop

> **What this is:** the proposal for connecting a Retinue bot to one persistent computer on the VPS. The working MCP bridge is in `tools/vps-computer-mcp/`.
> **Status:** proposal + standalone bridge. No daemon or UI wiring is in this PR.
> **Last updated:** 2026-11-13

## Goal

Give a Retinue bot a computer it can use for browser work, automation, routines and long-running tasks when the owner asks, without giving every bot its own VPS. One computer is provisioned now. The owner can wake it, sleep it, restart it, run routines and see what it is doing from Harbor/Ship.

The VPS side is already implemented in `sico-vibes/sico-vibe-dashboard` PR 107:

- `harbor-bot-retinue-01`, XFCE webtop, persistent `/config`, 2 vCPU / 4 GB / 1024 PIDs.
- isolated Docker network; no Docker socket; loopback-only port.
- owner-only live desktop at `/harbor/computer/view/`.
- owner dashboard at `/harbor/computer`.
- cron routines and a hashed-token MCP API.

## What this PR adds

- `tools/vps-computer-mcp/retinue-computer-mcp.mjs`: a dependency-free stdio MCP bridge.
- `tools/vps-computer-mcp/README.md`: setup and tools.
- This proposal.

The bridge can be used by any MCP client today. It is intentionally not wired into `retinued` yet.

## Integration contract

### Transport

Stdio MCP server. Retinue already builds `ExecOptions.mcp_servers` and passes it to Claude, Codex and ACP/OpenCode. The follow-up adds this bridge as an app-owned `McpServerSpec` instead of asking users to edit provider configs.

### Settings

Add **Settings > Connections > VPS computer**:

- URL: `https://dashboard.synthet.me`
- token: stored in Windows Credential Manager; SQLite keeps a reference only
- enabled: off by default
- computer name: display only

### Injection

When enabled, the daemon appends one `McpServerSpec` to each new bot session:

```text
name: vps-computer
command: node
args: [<bundled bridge path>]
env: HARBOR_COMPUTER_URL, HARBOR_COMPUTER_TOKEN
```

The token is resolved from the credential store at session start; it is never written to instructions, logs or provider config on disk.

### Tools

Expose the bridge tools to the bot. The useful ones for Retinue are:

- `computer_status`
- `computer_wake` / `computer_sleep` / `computer_restart`
- `computer_logs`
- `computer_exec`
- `computer_list_routines` / `computer_create_routine` / `computer_run_routine`
- `computer_desktop_url` (owner-only URL; the bot should return it, not open it)

### UI

- A small **Computer** pill on the bot/conversation header: awake, asleep, working, last routine.
- A **Computer** page under Connections with status, wake/sleep, routines and last output.
- If a task needs the computer, the owner sees "wake computer" before dispatch.
- Cancelling a task should not silently stop a 24/7 computer; offer **sleep** as a separate action.

### Policy

- Computer tools are **never remote** by default. They are owner/desktop-only; add them to `tiers.json` as `never`.
- `computer_exec` is equivalent to running a shell inside the bot's container. Treat it as privileged, show the command in the approval card when approval mode is Ask, and never run it unattended in bypass.
- The MCP token is scoped to one computer and can be revoked in Harbor.
- The bridge never runs on the VPS host; commands run inside the container as `abc`.

### Task integration

- A task can request the computer in its brief.
- The daemon wakes the computer before the first turn only when the task explicitly asks for it.
- Routines belong to Harbor, not Retinue; Retinue reads their state and can trigger a run.
- A 24/7 computer is not stopped when a conversation ends. Only the owner or an explicit tool call sleeps it.

## Why one computer, not one per bot

The VPS has 8 vCPU, 23 GiB RAM and about 6.7 GiB available at the time of writing. One 4 GB webtop is safe; two are not without a RAM upgrade and lower load. One shared desktop is the correct first step. Parallel screen control needs one desktop per active agent, which this VPS cannot support yet.

## Security notes

- The computer has its own Docker bridge, no Docker socket, no host network, no host mounts beyond `/config`.
- The desktop is owner-session gated; the MCP token is separate and can be revoked.
- The bridge has no arbitrary host URL feature: it always calls the configured Harbor computer API.
- A stolen token can wake/sleep/exec in that one computer. Rotate it in Harbor if leaked.
