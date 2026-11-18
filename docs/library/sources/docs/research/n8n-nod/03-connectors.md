# Nod's connectors, compared with Retinue's MCP handling

> **What this is:** research only, for the owner. No Retinue code was changed and nothing was committed.
> **Source read:** `github.com/n8n-io/nod`, shallow clone at commit `47cfee3fe77279e1d543a367131d3ff2c1202e18` (16 November 2026), at `_research\nod-connectors`. The clone was read, never built or run.
> **Licence:** Apache 2.0 for the whole repository. See section 1.
> **Author:** Grey (research). **Date:** 16 November 2026.

## Summary

1. Nod does not write connectors. It has four ways into an app, and only one of them gives "many apps".
2. That one is a hosted broker: every Composio request goes to `composio.n8n.cloud`, signed per installation, and the vendor holds the OAuth tokens and the app definitions. The repository contains no per-app integration code at all.
3. Everything else is one generic client for three shapes: a public HTTPS MCP server (OAuth discovery, DCR and PKCE, or a pasted token), a single HTTP path, or a sealed CLI install.
4. Width is solved on the agent side, not the connector side: one `apps` tool plus `SEARCH_TOOLS` for schemas, and remote MCP tools as deferred `mcp__<server>__<tool>`, so a huge catalogue costs almost no context.
5. Retinue has no third-party connectors today: four app-owned MCP servers and a 250-row remote policy. The fastest route to many connectors inside our rules is nod's second path (owner-added MCP servers, token held by the daemon), not their first.

## 1. What the licence allows

| Fact | Evidence |
|---|---|
| The repository is Apache 2.0: permissive, OSI-approved, usable with attribution. | `LICENSE:1-3`; `package.json:4` (`"license": "Apache-2.0"`) |
| There is no Sustainable Use Licence anywhere in the repository. n8n's core product uses SUL; nod is a separate repository and does not. | searched the tree for "Sustainable Use": no match |
| `NOTICE` covers one GPL-3.0 work, the speech phonemizer. It is not a connector file. | `NOTICE:7-10` |
| Composio and the MCP SDK are pinned dependencies, not vendored code: `@composio/client` 2.0.0-rc.17, `@composio/core` 0.25.0. | `package.json` dependencies |

**Verdict for this topic.** We may legally copy nod's connector code with a notice, because Apache 2.0 permits it. We should not: their shape assumes a Node server with Postgres and a hosted vendor. For every idea below the recommendation is **learn from the design, write our own in Rust**. No idea here needs their code.

## 2. How nod does it

### 2.1 The four ways in

| Path | Where the credential lives | How many | Authorisation |
|---|---|---|---|
| Composio apps, through n8n's connection service | At Composio. Nod never sees an app token. | The vendor catalogue: whatever Composio returns, paged 1,000 at a time, up to 10 pages, filtered to toolkits with managed auth or DCR OAuth | Vendor OAuth; the owner approves on the provider's consent screen |
| Official MCP servers (Notion, Linear, n8n) | Nod, sealed in its own Vault | 3 hard-coded rows | MCP OAuth: discovery, dynamic client registration, PKCE |
| Anything the owner adds (MCP, HTTP API, CLI) | Nod, sealed in its own Vault | Unbounded | Pasted token or key, or MCP OAuth |
| Browser sessions and saved logins | Nod Vault | Unbounded | The owner signs in once in a private browser profile |

Seven Composio toolkits are named in the code (Gmail, Calendar, Drive, Sheets, Docs, Slack, Outlook) and three more are local metadata (Notion, Linear, n8n). Everything beyond that arrives from the vendor catalogue at runtime. `src/shared/integrations.ts:11-31`, `src/providers/composio/integrations.ts:25-33`, `src/server/discovered-connections.ts:141-148`.

### 2.2 The design: one port, three providers

Nod defines one TypeScript interface, `IntegrationProvider`, and implements it per vendor. The interface covers the whole life of a connection: catalogue, authorise, complete, inspect, delete, subscribe to events, and open a call session. `src/ports/integrations.ts:298-378`.

The session object is the part that matters for the agent. It offers `search(query)`, which returns tool schemas and suggested use cases, and `execute(action, args, accountId)`. The agent never sees a vendor SDK. `src/ports/integrations.ts:51-67`.

### 2.3 How a connector is defined

- **Composio apps:** by a vendor toolkit slug. There is no local definition.
- **MCP, HTTP, CLI:** by an "access path" object: kind, endpoint, auth (`bearer`, `x-api-key`, `oauth`), and for a CLI its command, the environment variable names it reads, and its install source (npm, PyPI, or an HTTPS release with a sha256). `src/runtime/tools.ts:555-586`.
- **MCP servers themselves** define their tools. Nod takes each tool's name, description and input schema from `tools/list`, and removes `pattern` and `patternProperties` before local validation, so a hostile schema cannot run a regex. `docs/src/content/docs/connect/mcp-api-cli.md:48`.

### 2.4 Discovery

Three sources:

1. `manage_apps list` reads the owner's connected accounts. `manage_apps discover` searches the Composio catalogue by app name or task. `src/runtime/tools.ts:535-591`.
2. `SEARCH_TOOLS` searches the *tools* of connected accounts by task query and returns matching schemas plus the account ids. `src/server/integrations.ts:1647-1798`.
3. For MCP servers, `tool_search` loads the deferred tools of a connected server. `src/runtime/tools.ts:764-803`.

The catalogue is cached for 15 minutes and refuses to loop: at most 10 pages, and a repeated cursor is an error. `src/providers/composio/integrations.ts:281-352`.

### 2.5 Install and authorise

**Composio.** `connect()` mints a 32-byte nonce, puts its hash in an `app_connections` row with a 10-minute expiry, asks the provider for a sign-in URL, and rejects anything that is not HTTPS or carries credentials in the URL. The callback is not trusted on its own: `verifyIntegrationCompletion` hashes the returned state, compares it to the stored hash and account id, then calls the provider to ask whether that account is actually active. `src/server/integrations.ts:907-1037`, `src/server/integration-connection.ts:15-70`.

**Broker signing.** Every Composio call is rewritten to `https://composio.n8n.cloud` and truncated to four path prefixes. It carries an Ed25519 signature over `nod-composio-v1`, method, origin, path, timestamp, request id and body hash. Redirects are refused, the body is size-capped, and one clock-skew retry uses the service's `date` header. A separate enrolment call registers the installation; the broker can then revoke it (`installation_revoked`). `src/providers/composio/proxy.ts:50-63, 118-180`; `src/ports/integrations.ts:111-124`.

**Owner-added MCP.** The agent proposes an access path; the owner finishes it on a private page in the app (`/apps?connect=<id>`, 15 minutes). The token is typed there, never in chat, and stored sealed. `src/server/discovered-connections.ts:389-460, 691-757`.

**OAuth for MCP servers.** The MCP SDK's OAuth client does discovery, client metadata, PKCE, code exchange and refresh; Nod persists client information, tokens, expiry and the code verifier. `src/providers/discovered/oauth.ts:31-113`.

**Refresh.** One refresh per account at a time: a second caller waits on the first promise, the new token is written with a version compare-and-swap so two writers cannot clobber each other, and refresh starts 60 seconds before expiry. A refresh failure marks the account "sign in again". `src/server/discovered-connections.ts:1079-1163`, `src/server/vault-app-grants.ts:75-83`.

**Sealing.** Grants are encrypted with AES-GCM, keyed on owner and grant id, with a per-write version counter. `src/server/vault-app-grants.ts:26-90`.

**Scopes.** Nod never chooses them. Composio's auth configuration does, and the docs say plainly that reconnecting cannot add a scope the configuration does not request; the operator must change the configuration. `docs/src/content/docs/connect/apps.mdx:106`.

### 2.6 How the agent calls and chooses

The agent has one tool for app actions, not one per app: `apps {action, args}`, where `action` is a vendor tool name such as `GMAIL_FETCH_EMAILS`. `src/runtime/tools.ts:480-488, 1182-1188`.

Choosing is a loop the agent runs itself:

1. The prompt carries a one-line index per connected MCP server: name, account, the `mcp__<ns>__<tool>` pattern, how many tools, and a hash of the server's guide. It does not carry the tools. `src/runtime/prompt.ts:111-128`.
2. `SEARCH_TOOLS` with a narrow task query returns up to 25 schemas per path. Above six results it returns three in full plus an index and tells the agent to query an exact tool name. `src/server/integrations.ts:1755-1794`, `src/server/discovered-connections.ts:1269`.
3. A skill (`connected-apps`) teaches the order: reuse known schemas, search only when a schema is missing, prefer the app action, fall back to the website with the same account, and report a refused search as a gap rather than an error. `src/runtime/skills/connected-apps/SKILL.md:3-12`.

**The gate.** Every call passes `trust.check(turn, action, args)` before it runs, and the result goes through `approvals.request`, which either runs it or asks the owner. The judgement is a model comparison of the proposed action against the owner's words, the schedule's instruction and the owner's standing rules. A separate classifier decides whether an action only reads; reads and drafts run without asking, everything else needs owner words. App results are never shown to the checker, so an email cannot authorise an action. `src/server/composition.ts:2630-2731`, `src/server/trust.ts:24-46`.

### 2.7 Data flow of one Gmail read

1. Agent calls `apps {action: "GMAIL_FETCH_EMAILS", args: {...}}`.
2. `integrations.prepare` resolves the app from the action prefix, checks the action belongs to a connected account, and pins the account id. `src/server/integrations.ts:1498-1516, 1810-1904`.
3. `trust.check` judges it a read, so no approval card.
4. `executeApproved` opens a provider session bound to that account and calls it.
5. The signed request leaves for `composio.n8n.cloud`; the broker calls Composio; Composio calls Google with the token Nod never had. `src/providers/composio/proxy.ts:118-180`.
6. The result comes back, is redacted (the token in plain, JSON, URL and form encodings), and is wrapped as untrusted data before it reaches the model. `src/server/discovered-connections.ts:170-198`, `src/server/integrations.ts:235`.

### 2.8 Numbers

| Measure | Nod |
|---|---|
| App connectors named in code | 7 Composio toolkits, 3 official MCP rows |
| App connectors available at runtime | Up to 10,000 catalogue entries (10 pages of 1,000) |
| Accounts per service | 10 |
| Schemas per search reply | 25 per path, plus 200 names |
| Connector token held locally | Only for non-Composio paths |
| Own code per app | None |

## 3. How Retinue does it now

**What a bot gets.** `team::attach` gives a bot session up to four app-owned MCP servers, all of them Retinue's own programs talking back to the daemon through the stdio bridge: `retinue` (team, memory, skills, schedules), `vps-computer` (16 tools), `vps-browser` (7 tools), `owner-computer` (8 tools, only while the owner's switch is on). `crates/retinue-daemon/src/team.rs:570-606`, `browser.rs:1165-1185`, `computer.rs:1448-1470`, `owner_computer.rs:1006-1028`.

**No third-party connector exists.** The daemon is an MCP *server* to the coding agent; it is never an MCP *client*. There is no outbound MCP call, no connector table, no per-app code. `crates/retinue-daemon/src/mcp_bridge.rs:289-311`.

**The one connector-shaped surface is the Retinue MCP.** It runs the other way: an outside client such as Claude Code drives Retinue. Its catalogue is 19 tools in a Rust vector, each with a tier: 12 viewer, 4 chat, 3 operator. Nothing is discovered at runtime. `crates/retinue-daemon/src/external.rs:386-405, 121-133`.

**Policy.** `apps/desktop/src/remote/tiers.json` has 250 method rows: 42 `read`, 19 `chat`, 2 `approve`, 1 `handoff`, 186 `never`. Every `computer.*`, `browser.*`, `owner_computer.*` and `external.*` row is `never`, and their events are `drop`.

**Secrets.** A bot's environment variable can point at a named secret in Credential Manager, read only at spawn. No RPC returns a value. The cap is 2,048 bytes, and a one-use request allows 16,384. `crates/retinue-daemon/src/secrets.rs:1-14, 28-40`.

**Settings.** Settings › Connections › Integrations has three pages: Retinue MCP, Computer, Secrets. There is no app or connector page. `apps/desktop/src/ui/SettingsSheet.tsx:76`, `ui/ConnectionsPage.tsx:1-4`.

**How a call is approved.** The coding agent's own permission request reaches the daemon, which decides by mode and by saved rules. For an MCP tool the match key is the tool name, so "Always allow" can pin one tool permanently. `crates/retinue-daemon/src/main.rs:2336-2356`; `apps/desktop/src/ui/ApprovalRulesPage.tsx`.

**One accident worth knowing.** Retinue launches Claude Code with `--mcp-config` and pre-approves only `mcp__retinue`. It does not pass `--strict-mcp-config`, so Claude Code's own user-level MCP servers still load for a bot. Those tools are outside Retinue's policy, and every one of their calls asks, or runs free in Full access. `crates/retinue-adapter-claude/src/lib.rs:310-331, 818-827`.

**Already planned.** Our own accepted plan item 5 asks for an MCP connector catalogue: official registry entries and custom servers over stdio or Streamable HTTP, secrets in Credential Manager, SQLite holding references only, exact per-agent and per-project grants, an allow-once approval for every external action, and OAuth with state and PKCE. `docs/plans/reference-expansion-plan.md:17`.

## 4. What each side does better

**Nod is better at:**

- **Breadth without code.** A vendor catalogue plus one generic client covers thousands of apps with no per-app work.
- **Keeping tokens away from the model.** The broker holds them, or the server injects the header. Retinue injects environment variables, and a coding agent can read its own environment.
- **Context cost.** One meta-tool, query-based schema lookup, a one-line server index and deferred tools. Our four servers publish 31 tools outright, and every connector would add more.
- **Partial honesty.** A failed half of a search returns the results plus `incomplete: "... Matching tools may be missing."` A bad `SEARCH_TOOLS` returns `status: "rejected"` with a code instead of an exception. `src/server/integrations.ts:1755-1794`.
- **Token hygiene on refresh.** Single-flight refresh, compare-and-swap writes, 60-second early start.
- **Outbound safety.** HTTPS only, DNS pinned to the validated address, credentials stripped on cross-origin redirects, size caps. `src/providers/discovered/remote.ts:69-191`.
- **Credential redaction.** It removes the token in plain, JSON-escaped, URL-encoded and form-encoded forms from results and errors, and withholds a file that contains one. `src/server/discovered-connections.ts:170-198`.

**Retinue is better at:**

- **Policy you can read.** 250 named method rows with a reason each, and a parity test that fails the build. Nod has no per-method table; a phone or a channel sees the same tools as the desktop.
- **Consent by asking.** Our gate is an approval card the owner reads. Theirs is a model judging the owner's words, and their docs say plainly that "the trust check uses a model and can make mistakes". `docs/src/content/docs/connect/apps.mdx:48`.
- **No vendor in the path.** Nothing a bot reads or writes passes through a third party. That is the point of the 14 November amendment: browsing happens in Retinue's own profile, and the owner's signed-in browser is a per-conversation opt-in with pauses at every sign-in, payment and post.
- **Everything local and typed.** One Rust daemon, SQLite, no Postgres, no Node server.
- **Honest defaults.** Off is off: no server, no tools. Nod's catalogue arrives from the vendor, and a broker refusal can take every app away at once.

## 5. Ideas we could adopt

| # | Idea | Effort | Risk | Rules it touches |
|---|---|---|---|---|
| 1 | Owner-added MCP servers, called by the daemon, token in Credential Manager | Medium to large | Medium | Loopback; no copied provider authentication; Remote tiers; 14 Nov amendment |
| 2 | MCP OAuth (discovery, DCR, PKCE, refresh) | Large | Medium | Same, plus egress and sign-in pauses |
| 3 | A short built-in catalogue of official MCP servers | Small | Low | Same as 1 |
| 4 | One meta-tool plus schema search for connector tools | Small to medium | Low | None; it is an agent-side change |
| 5 | Per-tool allow rules and a read-only split on first use | Small | Low | Approvals; unattended rails |
| 6 | Outbound safety kit: HTTPS only, DNS pinning, no cross-origin redirects, size caps | Small | Low | Loopback and egress |
| 7 | Encoded-form secret redaction for tool results | Small | Low | Never log secrets |
| 8 | Sealed CLI connectors (install from npm, PyPI or a release hash) | Large | High | No third-party service; unattended rails; supply chain |
| 9 | Hosted aggregator (Composio, or n8n's connection service) | Small to build, large to justify | High | No third-party service; content stays private; Remote egress |
| 10 | Connectors generated from OpenAPI specs | Large | High | Egress; SSRF surface; no clear owner need |

### 1. Owner-added MCP servers, called by the daemon (recommended first step)

The owner adds a name, an HTTPS address and a token on a new Settings › Integrations › Connectors page. The token goes into Credential Manager. The daemon becomes an MCP client: `initialize`, `tools/list`, `tools/call`, over Streamable HTTP. It publishes those tools through a **new** app-owned bridge mode, for example `connectors`, as `connector_list`, `connector_search` and `connector_call`.

Why this shape: every wire decision (which host, which tool, which arguments) stays in the daemon, where `tiers.json` and the approval card already live. The bot never holds the token. Every third-party product that matters now ships a hosted MCP server (Notion, Linear, GitHub, Sentry, Stripe, Supabase), and one client reaches all of them with no per-app code.

**Licence:** learn only.

### 2. MCP OAuth, as a later step

Add discovery, dynamic client registration, PKCE and a bounded loopback callback, then refresh and revoke. This removes the "paste a token" step for servers that offer OAuth.

One hard fact to plan around: our secret store caps a value at 2,048 bytes, and an OAuth state (client information, tokens, expiry, verifier, discovery) can exceed that. Either raise the one-use cap for this one class and seal it to a file, or store several named secrets with a version. Nod solved this with a sealed database row and a version counter.

**Licence:** learn only.

### 3. A short built-in catalogue

A static table of official MCP endpoints with a Connect button, in the style of nod's three rows. Small, and it makes the feature discoverable without a vendor.

**Licence:** learn only.

### 4. One meta-tool plus schema search

If connectors grow past a handful, do not publish every tool to every bot. Publish `connector_search` and `connector_call`, return schemas on demand, and keep a one-line index per connector in the prompt. This is the single most useful lesson in their runtime code, and it costs little.

**Licence:** learn only.

### 5. Per-tool allow rules and a read/write split

An external tool will otherwise ask on every call in an asking mode, and run free in Full access, which is worse. Our existing rule key is the tool name, so the card can offer "Always allow `mcp__linear__create_issue`" once. A per-connector "read-only tools" switch would let unattended runs use reads and nothing else.

Note the conflict with our own plan item 5, which says allow-once with no standing approval. This needs an owner decision.

**Licence:** learn only.

### 6 to 7. Safety kit and redaction

Both are small and worth doing whatever else happens: pin DNS to the validated address, refuse cross-origin redirects with headers attached, cap the response, and mask a returned secret in its encoded forms.

**Licence:** learn only.

### 8 to 10. What not to adopt yet

- **CLI connectors** install third-party packages and then run them with credentials. Nod isolates each one as its own Linux user. On Windows, with our rules, this is a supply-chain feature disguised as a connector. Defer.
- **A hosted aggregator** is the only way to reach Composio-scale breadth quickly, and it is exactly what our rules forbid: content passes through a vendor, tokens live there, and the vendor can cut the installation off. Their own docs describe the trade-off honestly. If the owner ever wants it, it needs a switch, a threat-model row, a contract, and an explicit "your app content leaves this computer" sentence in onboarding.
- **Spec-generated connectors** are not something nod does, and nothing suggests owners need them. Their HTTP path is a single saved method and path, not a generated tool set.

## 6. Which approach gives Retinue many connectors fastest, within our rules

**Owner-added MCP servers, called by the daemon, with the token in Credential Manager.**

The reasons, in order of weight:

1. It is already our accepted plan, item 5, in almost the same words, including the approval card we would rather have than nod's model judgement.
2. It reuses four things we already have: the stdio bridge and its per-session capability, Credential Manager with no RPC that returns a value, the approval card and its per-tool rules, and `tiers.json` for remote exposure.
3. It adds one new wire risk instead of thousands: the owner types one host, and the daemon talks to that host only.
4. No per-app code. The count of connectors becomes the count of MCP servers in the world, which is now most of the products an owner would name.
5. It does not touch the 14 November browser work. The browser stays the fallback for a site with no MCP server, which is the same fallback nod teaches its agent.

Expected shape, in order: storage for connectors (never the token); an outbound MCP client with timeouts; a `connectors` bridge mode; the Settings page; approval wiring; `connector.*` RPCs all `never` with `connector.changed` dropped; one threat-model row ("a bot's tool call reaches a third-party service under the owner's own token"); tests; a docs page. OAuth second, once the token path works.

## 7. Open questions for the owner

1. **Do you want third-party connectors at all?** The 14 November amendment already lets a bot use your browser, signed in as you, with a pause at every consequential step. That covers "any app" without a vendor. A connector is faster and more reliable, and it is one more token on your machine.
2. **Is a hosted broker ever acceptable?** It is the only route to thousands of apps in weeks. It puts app content through a vendor and gives that vendor the tokens. If the answer is no, say so and we can close idea 9 for good.
3. **Grants per bot, per project, or per conversation?** Plan item 5 says per agent and per project.
4. **May connector tools run unattended** (a scheduled task, a turn a teammate woke), or attended only, as the screen tools are now?
5. **Allow-once or standing rules?** Plan item 5 says allow-once. Nod's whole design assumes the opposite. Which do you want, and may a saved rule ever cover a write?
6. **Do you want CLI connectors** (an npm or PyPI tool run with a token in its environment)? That installs third-party code, and I would say no for now.

## 8. Evidence

Their files, all at commit `47cfee3fe77279e1d543a367131d3ff2c1202e18`; permalink prefix `https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/`.

| Claim | File and line |
|---|---|
| Apache 2.0, no SUL | `LICENSE:1-3`, `package.json:4`, `NOTICE:7-10` |
| Ten local app names | `src/shared/integrations.ts:11-31` |
| Seven Composio toolkits | `src/providers/composio/integrations.ts:25-33` |
| Catalogue paging, filter, 15-minute cache | `src/providers/composio/integrations.ts:281-352` |
| Session search and execute | `src/providers/composio/integrations.ts:500-660` |
| Broker origin, signed envelope, path allowlist | `src/providers/composio/proxy.ts:50-63, 118-180` |
| Provider port | `src/ports/integrations.ts:298-378` |
| Broker refusal codes and enrolment | `src/ports/integrations.ts:109-124` |
| Connect, nonce hash, 10-minute link | `src/server/integrations.ts:907-1037` |
| Callback is verified, not trusted | `src/server/integration-connection.ts:15-70` |
| Action-to-app allowlist | `src/server/integrations.ts:1498-1516` |
| SEARCH_TOOLS, index, partial honesty | `src/server/integrations.ts:1647-1807` |
| Prepare, account pin, file pinning | `src/server/integrations.ts:1810-1904` |
| Trust check, then approval | `src/server/composition.ts:2630-2731`; `src/server/trust.ts:24-46` |
| Official MCP rows | `src/server/discovered-connections.ts:141-148` |
| Proposal, endpoint checks, CLI command deny-list | `src/server/discovered-connections.ts:389-460` |
| Save, sealed grant, resume the turn | `src/server/discovered-connections.ts:691-757` |
| Single-flight refresh, compare-and-swap | `src/server/discovered-connections.ts:1079-1163` |
| Sealed grants and version | `src/server/vault-app-grants.ts:26-90` |
| MCP OAuth client | `src/providers/discovered/oauth.ts:31-113` |
| HTTPS only, DNS pin, redirect stripping | `src/providers/discovered/remote.ts:69-191` |
| One `apps` tool | `src/runtime/tools.ts:480-488, 1182-1188` |
| `manage_apps` | `src/runtime/tools.ts:535-591` |
| Deferred `mcp__` tools | `src/runtime/tools.ts:764-803` |
| One-line MCP index in the prompt | `src/runtime/prompt.ts:111-128` |
| Skill guidance | `src/runtime/skills/connected-apps/SKILL.md:3-12` |
| Docs: vendor path, controls | `docs/src/content/docs/connect/apps.mdx:16, 35-43, 106, 112-116` |
| Docs: access paths, CLI sealing, schema hardening | `docs/src/content/docs/connect/mcp-api-cli.md:40-73` |

Ours:

| Claim | File and line |
|---|---|
| Four app-owned servers attached per bot | `crates/retinue-daemon/src/team.rs:570-606` |
| Browser server, 7 tools | `crates/retinue-daemon/src/browser.rs:1165-1185` |
| Computer server, 16 tools | `crates/retinue-daemon/src/computer.rs:1448-1470` |
| Owner-computer server, 8 tools | `crates/retinue-daemon/src/owner_computer.rs:1006-1028` |
| Daemon is an MCP server, never a client | `crates/retinue-daemon/src/mcp_bridge.rs:289-311` |
| 19-tool catalogue with tiers | `crates/retinue-daemon/src/external.rs:386-405, 121-133` |
| 250-row remote policy | `apps/desktop/src/remote/tiers.json` |
| Secrets doc and 2,048-byte cap | `crates/retinue-daemon/src/secrets.rs:1-14, 28-40` |
| Integrations settings group | `apps/desktop/src/ui/SettingsSheet.tsx:76` |
| Retinue MCP page | `apps/desktop/src/ui/ConnectionsPage.tsx:1-4, 26-30` |
| Pre-approval is per server; no strict MCP config | `crates/retinue-adapter-claude/src/lib.rs:310-331, 818-827` |
| Rule key is the tool name | `crates/retinue-daemon/src/main.rs:2336-2356` |
| Plan item 5, the connector catalogue | `docs/plans/reference-expansion-plan.md:17` |
| Licence reuse and remote content rule | `docs/plans/reference-expansion-plan.md:30-31` |
| Browser and computer threat rows | `docs/remote/THREAT_MODEL.md:303-316` |

**Not verified.** Whether Claude Code merges its user-level MCP servers when `--mcp-config` is passed without `--strict-mcp-config` is inferred from the CLI's flags, not run. The number of apps Composio actually serves was not measured; the code allows up to 10,000 entries.
