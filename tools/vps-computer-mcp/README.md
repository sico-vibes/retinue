# Retinue VPS computer MCP bridge

Standalone stdio MCP bridge from a Retinue/Claude/Codex/OpenCode session to one Harbor bot computer on the VPS.

## Setup

1. In Harbor, open **Computer**, create an MCP token, and copy it once.
2. Run the bridge from the machine that hosts the agent:

```bash
HARBOR_COMPUTER_URL=https://dashboard.synthet.me \
HARBOR_COMPUTER_TOKEN=hrb_... \
node retinue-computer-mcp.mjs
```

Any MCP client can launch it. The token is stored only in the environment on the agent machine.

## Tools

`computer_status`, `computer_wake`, `computer_sleep`, `computer_restart`, `computer_logs`, `computer_exec`, `computer_list_routines`, `computer_create_routine`, `computer_run_routine`, `computer_desktop_url`.

`computer_exec` runs inside the bot computer as the desktop user in `/config`. It does not run on the VPS host.

## Native Retinue integration

The daemon on `main` already carries app-owned `McpServerSpec` entries into each provider session. The follow-up integration should add a **Connections > VPS computer** card (URL + token in Windows Credential Manager), inject this bridge as an app-owned MCP server, and show the computer state beside the bot. The proposal is in `docs/plans/vps-computer.md`.
