# Vault, chat channels and Pi: the contracts

> **Status:** current, 16 November 2026. Owner decisions are in `AGENTS.md`, amendment of 16 November 2026. Background: `docs/research/n8n-nod/02-llm-providers.md`, `04-vault.md`, `05-messaging.md`.
> **Lanes:** Ada A (vault), Ada B (channels), Ada C (Pi) build the backend in their own worktrees. The Director builds the UI against these contracts, using preview fixtures until the backend lands. Lanes never commit.

All RPCs go through `dispatch()`. All are `never` in `apps/desktop/src/remote/tiers.json` (the parity test needs a row for each), and their events are `drop`, unless a line below says otherwise. Every timestamp is RFC 3339 UTC. Errors use the daemon's usual `{code, message}`; messages are plain sentences the UI can show.

## A. Vault

Values in Windows Credential Manager, one credential per field: `Retinue/vault/<itemId>/<field>`. Metadata in a new SQLite table. Reuse `secrets.rs` (`wincred`, `Zeroizing`, size limits). Field values up to 2,048 characters.

Item kinds and their fields (`secret` fields are never returned except by `vault.reveal`):

| kind | public fields | secret fields |
|---|---|---|
| `login` | `username`, `origin` (an `https://host` origin, required) | `password`, `totp` (base32 authenticator key, optional) |
| `card` | `holder`, `brand` (derived), `last4` (derived), `expiry` (`MM/YY`) | `number`, `cvc` |
| `apiKey` | `service` (free text) | `value` |
| `note` | none | `text` |

```ts
type VaultItem = {
  id: string; kind: 'login' | 'card' | 'apiKey' | 'note'; title: string
  origin: string | null; username: string | null; holder: string | null; brand: string | null; last4: string | null; expiry: string | null; service: string | null
  hasTotp: boolean
  createdAt: string; updatedAt: string; lastUsedAt: string | null; useCount: number
}
```

- `vault.list {}` → `{ items: VaultItem[] }`
- `vault.create { kind, title, origin?, username?, holder?, expiry?, service?, secrets: { password?, totp?, number?, cvc?, value?, text? } }` → `{ item }`. A card number is checked with Luhn; `brand` and `last4` come from it.
- `vault.update { id, title?, origin?, username?, holder?, expiry?, service?, secrets? }` → `{ item }`. A secret field left out is unchanged; an empty string is refused (delete the item instead).
- `vault.delete { id, confirm: true }` → `{ deleted: true }`. Removes the credentials too.
- `vault.reveal { id, field }` → `{ value }`. Desktop only; writes a `reveal` ledger row. `vault.totp { id }` → `{ code, expiresInSecs }` the same way.
- `vault.activity { itemId?, limit? }` → `{ rows: [{ id, at, itemId, itemTitle, action: 'fill' | 'reveal' | 'create' | 'update' | 'delete' | 'denied' | 'expired' | 'uncertain', where: 'browser' | 'cloud' | 'owner' | 'desktop', origin: string | null, bot: string | null, sessionId: string | null }] }`. Content-free.

Use by a bot (MCP tools on the team bridge, not available in automations or a `bypass` conversation):

- `vault_list { kind?, origin? }` → titles, kinds, origins and ids only.
- `vault_fill { itemId, field, target }`, where `target` is `{ surface: 'browser', ref }` (a `browser_snapshot` reference) or `{ surface: 'cloud' }` (types at the focused field of the linked cloud computer) or `{ surface: 'owner' }` (only inside a live owner-computer grant). The daemon checks the page origin matches a login's `origin` before typing; a mismatch is refused and logged `denied`.
- The call waits for the owner (same pattern as `request_secret`, under 60 s). The tool result is only `filled`, `denied`, `expired` or `uncertain` (typed, but the page changed or the action timed out); never the value.

Owner approval:

- Event `vault.use.requested { requestId, sessionId, bot, itemId, itemTitle, kind, field, surface, origin, expiresAt }`; `vault.use.list {}` → `{ requests: [...] }`; `vault.use.answer { requestId, decision: 'allow' | 'deny' }`; event `vault.use.updated { requestId, status }`. Never answerable from a phone or chat.

Hardening from report 04 to include: the browser snapshot must stop echoing the value of password and filled fields (report 04, idea 1); after a fill, redact the value from snapshot text.

## B. Chat channels (Telegram, WhatsApp) to the CEO

One row per channel kind. Tokens in Credential Manager (`Retinue/channel/<kind>/<name>`), never in SQLite, events, logs or backups. Only one channel of each kind. A channel needs a CEO (`systemRole: manager`); without one, `connect` is refused with "Hire your CEO first."

```ts
type Channel = {
  kind: 'telegram' | 'whatsapp-web' | 'whatsapp-cloud'
  status: 'off' | 'connecting' | 'pairing' | 'connected' | 'error'
  detail: string | null            // a plain sentence for errors or what to do next
  account: string | null           // "@retinue_bot" or the WhatsApp number, masked "+44 •••• 1234"
  peer: { name: string; handle: string | null } | null   // the paired sender
  approvals: boolean               // Allow/Deny buttons in chat; default true (owner, 16 Nov 2026)
  sessionId: string | null         // the CEO conversation for this channel
  qrDataUrl?: string | null        // WhatsApp Web while pairing
  pairing?: { link: string | null; code: string; expiresAt: string } | null   // Telegram while pairing
  candidate?: { name: string; handle: string | null; at: string } | null    // waiting for desktop approval
  lastMessageAt: string | null
}
```

- `channel.list {}` → `{ channels: Channel[] }` (all three kinds, `off` when not set up).
- `channel.telegram.connect { botToken }` → `{ channel, pairing: { link: 'https://t.me/<bot>?start=<code>', code, expiresAt } }`. Validates with `getMe`. Long polling only; no listener.
- `channel.whatsappWeb.start {}` → `{ channel }`, then events `channel.qr { kind, qrDataUrl, expiresAt }` until linked. The daemon draws the QR (for example with the `qrcode` crate) as a `data:image/png;base64,…` or `data:image/svg+xml;base64,…` URL; the UI has no QR library and shows it as an image. `channel.list` also carries the latest `qrDataUrl` while `status` is `pairing` for WhatsApp Web, so a reopened page can show it. The linked device's session lives in a user-only folder under `<data>/channels/`, excluded from backups and diagnostics.
- `channel.whatsappCloud.connect { phoneNumberId, accessToken, appSecret }` → `{ channel, webhook: { url: string | null, verifyToken } }`. Design the inbound path (a webhook needs a public address: the linked cloud computer is the candidate) and write it up in your report before building more than the outbound half; ask the Director if it needs a new listener or relay.
- `channel.pairing.answer { kind, approve }`: the first sender who uses the code becomes a candidate (event `channel.pairing.candidate { kind, name, handle, at }`); the owner approves on the desktop. Codes last 10 minutes, five attempts.
- `channel.update { kind, approvals? }`, `channel.disconnect { kind }` (removes the token and the paired sender), `channel.test { kind }` (sends "Retinue is connected." to the paired sender).
- Event `channel.updated { channel }` on every status change.

Routing: an inbound message from the paired sender becomes a user message in the CEO's channel conversation (create it on first use, titled "Telegram" or "WhatsApp"), with `meta: { channel: kind }`. Only the CEO's final reply text goes back (sanitised: no thinking, tool output, local paths, secrets; long replies split at the platform limit). Groups, other senders and status messages are dropped. Keep an idempotent inbound ledger keyed by the provider's message id (report 05, idea 4.5).

Approvals: when `approvals` is on, each CEO approval card is also sent to the chat with Allow and Deny buttons (Telegram inline keyboard; WhatsApp reply buttons or interactive list on Cloud, numbered replies "1 Allow / 2 Deny" on Web). The button carries an opaque one-use id; the answer is applied through the normal approval path. Never for vault uses, `owner_*` or `computer_*` grants, Full access or settings.

WhatsApp Web: no Node runtime is guaranteed. Prefer a Rust crate (check `whatsapp-rust` and similar) with a licence we can ship; otherwise a pinned third-party binary run as a supervised child (licence, version and SHA-256 in `docs/operations/`). Say which you chose and why.

## C. Pi

The owner chose "Retinue calls models through Pi" with keys held by Retinue. Run Pi's coding agent (`@earendil-works/pi-coding-agent`, check the real package and its non-interactive JSON or RPC mode) as a supervised child, and give it the keys through its environment at spawn. Pi appears as a new coding agent ("Pi") next to Claude Code, Codex and OpenCode: discovery, a model catalogue with efforts, turns, streaming, tool steps, approvals where Pi supports them, usage when reported (unknown stays unknown).

- `pi.providers {}` → `{ installed: boolean, version: string | null, providers: [{ id, name, keyEnv, hasKey, models: number | null, docsUrl: string | null }] }`.
- `pi.key.set { provider, key }` → `{ ok: true }`; `pi.key.delete { provider }`; `pi.key.test { provider }` → `{ ok, detail }` (one cheap call, or the provider's model list).
- Keys never leave the daemon except into that child's environment. `pi.*` is `never` remote.
- If Pi is not installed, `installed: false` and the runtime row says how to install it.

## Every lane

- Tests next to the code; run `cargo test -p retinue-daemon` (and any crate you touch) and `cargo clippy --workspace --exclude retinue-desktop --all-targets -- -D warnings`. On this PC Smart App Control can refuse fresh binaries (see the skill's `verification.md`).
- Add your threat-model rows to `docs/remote/THREAT_MODEL.md`.
- Report: what you built, file:line, what you verified and how, what is not done, and proposed skill updates.
