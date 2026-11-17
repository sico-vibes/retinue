# Adopting what we learned from nod

> **Status:** current, 16 November 2026. Owner directive: "do all the recommendations Grey came back with", plus MCP connectors. Sources: `docs/research/n8n-nod/01` to `09`. Each item names its report and idea number.
> **Lanes:** Ada lanes build the backend in their own worktrees (at most five OpenCode lanes at once); the Director builds every screen, reviews each diff, runs the gates and commits. A lane never commits.
> **Not adopted, as the reports advise:** nod's VM sandbox (01, idea 8: our cloud computer on the VPS already covers it), a model API layer of our own beyond Pi (02, idea 6), CLI connectors, a hosted aggregator such as Composio, and OpenAPI-generated connectors (03, ideas 8 to 10).

## Wave 1 (running)

| Lane | Work | Source |
|---|---|---|
| Ada A | Vault | 04, ideas 1 to 5; `docs/plans/vault-channels-pi.md` A |
| Ada B | Telegram and WhatsApp to the CEO | 05, ideas 4.1 to 4.7; plan B |
| Ada C | Pi | 02; plan C |
| Ada D | Connectors: owner-added MCP servers (below) | 03, ideas 1, 3, 4, 5, 6, 7 |
| Ada E | Engineering hygiene | 07, ideas 1, 2, 3, 5, 6 |

### D. Connectors contract

The daemon is the MCP client (Streamable HTTP; `initialize`, `tools/list`, `tools/call`). Tokens in Credential Manager (`Retinue/connector/<id>`). HTTPS only (localhost allowed for a server on this PC), DNS pinned to the validated address, no cross-origin redirects with credentials, response size cap, and a returned secret masked in its plain, URL-encoded and base64 forms (03, ideas 6 and 7). `connector.*` is `never` remote; events `drop`.

```ts
type Connector = {
  id: string; name: string; url: string
  auth: 'none' | 'token' | 'oauth'        // oauth comes in wave 3
  enabled: boolean
  status: 'ok' | 'error' | 'unchecked'; detail: string | null
  toolCount: number | null
  unattended: 'off' | 'readOnly'          // what automations may call (03, idea 5)
  lastCheckedAt: string | null
}
type ConnectorTool = { name: string; title: string | null; description: string; readOnly: boolean | null; rule: 'ask' | 'allow' | 'deny' }
type CatalogueEntry = { id: string; name: string; url: string; docsUrl: string; auth: 'token' | 'oauth'; tokenHint: string }
```

- `connector.list {}` → `{ connectors: Connector[], catalogue: CatalogueEntry[] }`. The catalogue is a short static table of official hosted MCP servers (03, idea 3), for example GitHub, Linear, Notion, Sentry, Stripe, Supabase; check each address before listing it.
- `connector.add { name, url, token? }` → `{ connector }` (runs a check). `connector.update { id, name?, token?, enabled?, unattended? }` → `{ connector }`. `connector.remove { id, confirm: true }`. `connector.test { id }` → `{ ok, detail, toolCount }`.
- `connector.tools { id }` → `{ tools: ConnectorTool[] }`; `connector.tool.rule { id, tool, rule }`. `readOnly` comes from the server's `readOnlyHint` annotation; unknown stays `null`.
- Bot tools on the team bridge (03, idea 4): `connector_search { query }` returns matching tools with their schemas; `connector_call { connector, tool, arguments }`. The prompt carries one line per enabled connector, not every schema. A call asks the owner through the usual approval card (tool shown as `<connector>: <tool>`) unless the tool's rule is `allow`; `deny` refuses. "Always allow" on the card sets the rule (owner decision, 16 November 2026: standing per-tool rules are allowed). Automations may call only tools marked read-only, and only when `unattended` is `readOnly`; never in a `bypass` conversation without the same rules.

### E. Engineering hygiene (07)

1. A secret scan and a format check, in CI and in the pre-commit hook. 2. An import-boundary check for the daemon modules and the UI layers, run in CI and as a test. 3. `docs/reference/invariants.md`: each rule from AGENTS.md and the skill, naming the test that pins it (or "untested"). 5. Tests that see only what they created (temp data folders, environment restored). 6. One failure sink in the daemon that tests can assert on. Keep to scripts, CI, docs and test helpers; touch `main.rs` only for the failure sink's hook.

## Status, 17 November 2026

- Wave 1 is merged on branch `integrate/nod-wave1`: Telegram works end to end; the vault, Pi, connectors and engineering hygiene are in. **WhatsApp is set aside for now (owner, 17 November 2026):** WhatsApp Web needs a client library whose licence and version we can verify, and the Business API needs a public webhook route; neither is built.
- Wave 2 started on 17 November 2026 in worktrees `-7` (automations), `-8` (agent experience) and `-9` (safety and providers). Each lane's contract and report is in `docs/reports/wave2-<lane>.md`.
- Wave 2 is merged on the same branch, with its screens: the schedule card's cost line and projection, run costs, "skip when nothing changed" and the spend check-in in the editor; "Checking back" on tasks; the bot's own "now" line; a kept plan marked "From an earlier turn"; message search scoped to the open conversation; a "What it can reach" card and the scratch folder in the conversation rail; price and unattended-pause cues in the model menu; the Doctor's Safety area.
- **Not done in wave 2** (lane 9's list): building the coding agents' environment instead of inheriting it; the grant clock (nothing runs locally yet); learning and masking a value typed during Take control (needs the vault's per-conversation set, now merged); capability rows in the provider table; a per-lane model choice for automations (needs an owner decision: same model or a named cheaper one); "your own model server" through a CLI's config. A spend check-in ends a run as `cancelled` with reason `spend_check`, not a new state.

## Wave 2

| Lane | Work | Source |
|---|---|---|
| Automations | Tell a late run it is late and what it missed; show what unattended work cost and its projection (unknown cost stays unknown); a task that checks back without a schedule; a "couldn't finish" message in the run's conversation; skip a run when the project can't have changed; a spend check-in for unattended runs, counted only from costs the provider reported; record who authorised a schedule and re-check on change | 06, ideas 4.1 to 4.7 |
| Agent experience | A "now" line the bot writes; memory recall before a turn, without a model call; a plan that outlives a turn; a docs search tool for bots; search earlier messages | 08, ideas 2, 3, 4, 6, 7 |
| Safety and providers | Say what a bot can reach; build the CLI environment instead of inheriting it (with an opt-out list and a doctor check); the "may have acted" outcome for killed commands and computer tools; ACL the data folder; head-and-tail output caps; the grant clock for local execution; a scratch folder per conversation; learn and mask a value the owner types during Take control; capability rows in the provider table; a per-model failure ledger that pauses only unattended work; capability and price cues in the model menu; a per-lane model choice; "your own model server" through a CLI's config | 01, ideas 1 to 7; 04, idea 6; 02, ideas 1 to 5 |

## Wave 3

| Lane | Work | Source |
|---|---|---|
| Learned skills | Skills a bot writes, through a review queue and the memory write rails; first-run suggestions grounded in the project | 08, ideas 1 and 5 |
| Release safety | Split CI by path with required checks and a nightly run; prove the update round trip and gate release on green CI; bound what a conversation read returns; later, a fingerprinted behaviour suite | 07, ideas 4, 7, 8, 9 |
| MCP OAuth | Discovery, dynamic client registration, PKCE, loopback callback, refresh and revoke; store state beyond the 2,048-byte cap | 03, idea 2 |
| Prompts | Edits to our system prompts and tool descriptions | 09 (in progress) |
