# nod (n8n labs): Telegram and WhatsApp channels, against Retinue Remote

> **What this is:** A research report on how n8n's `nod` connects to Telegram and WhatsApp, compared with Retinue's Remote mode, with what we could adopt and what we should refuse.
> **Status:** current
> **Last updated:** 2026-11-16

**Source and licence.** Cloned shallow from [github.com/n8n-io/nod](https://github.com/n8n-io/nod) at commit [`47cfee3`](https://github.com/n8n-io/nod/tree/47cfee3fe77279e1d543a367131d3ff2c1202e18) (16 November 2026) and read at `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-messaging`; nothing was installed, built or run. Links below are permalinks at that commit; our own side is cited as `path:line`. The repository root is **Apache 2.0** ([`LICENSE`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/LICENSE#L1-L3), `package.json:4`), not n8n's Sustainable Use License, so a copy with attribution would be legally permitted; this report still treats every idea as **learn only**, and no nod code is copied into Retinue. The transports depend on `chat`, `@chat-adapter/telegram`, `@chat-adapter/whatsapp` and `@whiskeysockets/baileys` (`package.json:38-40,54,56`); their licences are not in the clone (no `node_modules`), so check each before any copy. `NOTICE` shows nod also ships GPL-3.0-or-later speech code — take none of it. This is research only; no product code was changed.

## Summary

1. nod talks to chat apps through two official APIs and one unofficial one: the Telegram Bot API (polling or webhook), the WhatsApp Cloud API (webhook), and WhatsApp Web through Baileys (QR, no public address).
2. Exactly one person may talk to the bot — one Telegram user id or one phone number — and Telegram group chats, WhatsApp groups and status updates are refused structurally, not by a setting.
3. A pairing code is issued as a `t.me/<bot>?start=<code>` deep link, the bot records a *candidate* sender, and the owner still has to approve that candidate in the app with its name and username before anything is accepted.
4. Every inbound message becomes a turn on one shared conversation, carries the channel it came from, and the reply is delivered back only to that channel; approvals ride as plain text (or 👍/👎), are judged by a model with a 0.99 confidence floor, and an ambiguous answer expires the approval and cancels the turn.
5. Retinue has no chat-app channel at all; it has a paired-device gateway (Tailscale or Native), a machine-readable tier table, approvals off by default, and push. A Telegram channel as a *pull-only, chat-tier paired device* is the shape that fits our threat model; WhatsApp Web through Baileys does not.

## 1. How nod does it

### 1.1 Which APIs

| Channel | API | Needs a public address | Pairing |
|---|---|---|---|
| Telegram | Bot API, `getUpdates` long polling (the desktop setup picks it when no public URL is given, [`settings.ts:118-125`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/settings.ts#L118-L125)) or `setWebhook` | No for polling; a public HTTPS URL for webhook | Bot token from BotFather, then `/start <code>` in the chat, then owner approval |
| WhatsApp (Cloud) | Meta WhatsApp Cloud API (Graph v25.0) | Yes: public HTTPS callback plus a verify token | Business token, app secret, phone number id, allow-listed personal number |
| WhatsApp (QR) | WhatsApp Web via `@whiskeysockets/baileys` | No | Scan a QR as a linked device; the QR *is* the consent |

Only two providers exist: [`providerSchema = z.enum(['telegram','whatsapp'])`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channel-settings.ts#L21). The full environment table is [`.env.example:75-87`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.env.example#L75-L87).

### 1.2 Main files

- `src/providers/chat-sdk/index.ts` (956 lines): the Telegram + WhatsApp Cloud transport. Owns the webhook checks, the polling loop, posting, reactions, typing and Telegram's in-place progress edit.
- `src/providers/chat-sdk/settings.ts` (253 lines): verifies credentials before saving them, chooses polling or webhook, registers and removes the Telegram webhook.
- `src/providers/chat-sdk/whatsapp.ts` (854 lines): the Baileys WhatsApp Web transport (QR, device keys, sender matching, drop reasons).
- `src/providers/chat-sdk/telegram.ts`: zod schemas for updates and files, attachment download with size caps, and `telegramPairing`.
- `src/server/channels.ts` (1901 lines): the durable inbox/outbox, the permit re-checks, presentation of replies, delivery windows, progress and typing.
- `src/server/channel-settings.ts` (≈800 lines): encrypted channel settings, the Telegram pairing code and candidate, bind/confirm, reconnect.
- `src/server/channel-media.ts`: inbound files and voice transcription, at most 10 files / 50 MB.
- `src/shared/answer-choices.ts`, `src/server/trust.ts`, `src/server/approvals.ts`: how a chat reply becomes an approval decision.

### 1.3 Data flow

**Inbound.** Telegram delivers a webhook `POST /api/webhooks/telegram` or the poller fetches `getUpdates` ([`index.ts:352-462`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/index.ts#L352-L462)). Both paths call the same `transport.receive()`, which re-checks the secret and parses with zod. Then `channels.accept()` → a permit check → media preparation → an `INSERT` into `channel_inbox` keyed by a unique `source_id` ([`channels.ts:586-600`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L586-L600)). A 2-second drain loop plus an event wake-up runs `inboxStep()`, which re-checks the permit, takes `FOR UPDATE SKIP LOCKED`, and submits one turn ([`channels.ts:999-1094`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L999-L1094)).

**The conversation.** nod has one thread per owner, shared by web, Telegram and WhatsApp ([`AGENTS.md:22`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/AGENTS.md#L22)). Each channel identity is a stable hash, `conversationChannel = provider:sha256(account||sender)[0..32)` ([`channels.ts:195-201`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L195-L201)); the turn records it as `turns.channel`. The user message gets a deterministic UUID derived from owner + `source_id`, so a repeated webhook cannot create a second turn ([`channels.ts:203-211`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L203-L211)).

**Outbound.** `enqueueOutbox()` selects assistant messages whose turn's channel matches an inbox row, so a reply goes back only to the channel that asked ([`channels.ts:1119-1131`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L1119-L1131)). `outboxStep()` blocks a row when the device or the permit changed, when a question is closed, or when the WhatsApp 24-hour reply window has passed ([`channels.ts:1190-1202`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L1190-L1202)). The message is written by a model into chat prose (`presentChannelText`), then sanitised, chunked and posted. Schedules keep the delivery channel of the conversation that created them ([`schedules.ts:879-922`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L879-L922)).

### 1.4 Who may talk to the bot

One identity, checked in several places:

- **Telegram**: `chat.type` must be `private` and `from.id` must equal `chat.id` ([`telegram.ts:49-60`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/telegram.ts#L49-L60)); then `TELEGRAM_ALLOWED_USER_ID` must match ([`index.ts:541-547`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/index.ts#L541-L547)). Groups therefore cannot reach the bot at all.
- **WhatsApp Cloud**: the sender number must equal `WHATSAPP_ALLOWED_NUMBER`; a message from `fromMe` is dropped.
- **WhatsApp QR**: `shouldIgnoreJid` refuses `status@broadcast`, `@g.us` and `@newsletter` before the message is even decrypted, and any number that is neither the owner nor allowed is ignored early ([`whatsapp.ts:422-427`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/whatsapp.ts#L422-L427)). Two modes: a separate *bot number* that only the owner's personal number may message, or *self-chat* on the owner's own number where only messages the owner sent to themselves count ([`whatsapp.ts:219-224`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/whatsapp.ts#L219-L224)).
- **Every inbound** is checked a second time before work and a third time inside the transaction ([`channels.ts:523-532`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L523-L532), `:967`, `:1010`). Attachments are downloaded only after the sender check ([`channels.mdx:123`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/use/channels.mdx#L123)).

### 1.5 Pairing

1. The owner pastes a BotFather token. nod calls `getMe`, compares the bot id to the token prefix, and refuses a bot that already has a webhook ([`settings.ts:96-125`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/settings.ts#L96-L125)). It picks polling when no public URL is given.
2. With no allow-listed user yet, it issues a 24-byte base64url code, stores only its SHA-256 with a 15-minute expiry, and gives the owner `https://t.me/<bot>?start=<code>` ([`channel-settings.ts:305-326`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channel-settings.ts#L305-L326)). The desktop opens `tg://resolve` only after a strict shape check ([`main.mjs:718-737`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/desktop/main/main.mjs#L718-L737)).
3. `/start <code>` in the chat produces a pairing request; a constant-time compare against the stored hash records a **candidate** with id, name and username, and nothing is accepted yet ([`channel-settings.ts:592-637`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channel-settings.ts#L592-L637)).
4. The owner sees the candidate in the app and confirms. Only then is `TELEGRAM_ALLOWED_USER_ID` written and the bot replies in chat ([`channel-settings.ts:638-685`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channel-settings.ts#L638-L685)). A test walks every refusal: a stranger's text does nothing, a wrong code does nothing, the right code from the wrong sender does nothing, and messages are still ignored until confirmation ([`channel-delivery.test.ts:3200-3249`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/tests/channel-delivery.test.ts#L3200-L3249)).
5. WhatsApp QR has no approval step: the owner scans a QR valid for 60 seconds, the whole setup expires after 5 minutes, and the device keys are stored encrypted in the database ([`whatsapp.ts:458-464`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/whatsapp.ts#L458-L464), `:582-593`).

### 1.6 Approvals and attachments

- A confirmation is a normal chat message. Replying to it with text, or reacting 👍/👎, answers it. A model judges the words with a 0.99 confidence floor and three outcomes (yes / no / other) ([`trust.ts:344-363`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/trust.ts#L344-L363)). "Other" is not a rejection you can retry: the approval is expired, the waiting turn is cancelled, and an open task is told its work stopped ([`approvals.ts:632-676`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/approvals.ts#L632-L676)). An attachment or a voice note never approves ([`channels.mdx:131`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/use/channels.mdx#L131)); a reply must target the current confirmation.
- Questions with choices are flattened to a numbered list in the chat ([`answer-choices.ts:50-57`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/shared/answer-choices.ts#L50-L57)).
- **Attachments in**: at most 10 files and 50 MB total per message, Telegram downloads capped at 20 MB, WhatsApp at 50 MB, voice transcribed separately ([`channel-media.ts:44-88`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channel-media.ts#L44-L88)). Files become thread artifacts.
- **Attachments out**: Telegram up to 50 MB, WhatsApp Cloud only PNG/JPEG under 5 MB, WhatsApp QR up to 50 MB of any type; anything refused becomes a "get it in Nod" line ([`index.ts:694-732`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/index.ts#L694-L732)).
- **Private steps never travel in chat.** Sign-in, saved logins, card security codes, payments and browser takeover are redirected into the app; the channel reply is stripped of loopback and private links ([`channels.ts:213-259`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/channels.ts#L213-L259), [`channels.mdx:153-175`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/use/channels.mdx#L153-L175)).

### 1.7 The stranger problem, in their own words

Their answer has six layers: one allow-listed identity; chat type restricted to a private DM (or self-chat); the sender check repeated before work and inside the write; no media download before the check; a one-time code plus a candidate card plus owner confirmation before the first accepted message; and content-free, rate-limited drop logs — a fixed reason, the addressing type and a suppressed count, once a minute ([`whatsapp.ts:169-190`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/whatsapp.ts#L169-L190), [`channels.mdx:151`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/use/channels.mdx#L151)).

They also say plainly that the QR connection "uses Baileys, an unofficial WhatsApp client, so there's a small risk of account restrictions" ([`channels.mdx:32`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/use/channels.mdx#L32)).

## 2. How Retinue does it now

There is **no chat-app channel**. `grep` over `crates/`, `apps/desktop/src/` and `docs/` finds no Telegram, WhatsApp, Slack or Discord transport. What exists is Remote:

- **Two methods, one listener.** Tailscale `serve` publishing a loopback gateway, or Native: HTTPS on `remote.nativePort` (default 48621) with a self-made certificate. The daemon API stays on `127.0.0.1` with the capability token (`AGENTS.md`, 17 October amendments; `crates/retinue-daemon/src/remote/mod.rs:652-666`).
- **Pairing.** One 8-character code from a 32-symbol alphabet, 5 minutes, 5 wrong attempts burn it, cookies `__Host-retinue_pair` / `__Host-retinue_remote`, only the token's SHA-256 stored (`crates/retinue-daemon/src/remote/pairing.rs:29-43`). The phone submits first, the desktop raises the approval card, the owner picks viewer / chat / operator (`remote/mod.rs:1094-1136`).
- **One table, two languages.** Every daemon method has a row in `apps/desktop/src/remote/tiers.json`: 42 read, 19 chat, 2 approve (operator), 1 handoff, 186 never. `authorize()` enforces it in Rust (`crates/retinue-daemon/src/remote/policy.rs:292-344`) and the same table drives the TypeScript client; a parity test fails the build when a method has no row.
- **Defaults that carry the model.** Remote off; approvals from remote off; Chat preselected; 30-day idle expiry; quiet hours make the host read-only before dispatch (`docs/remote/THREAT_MODEL.md:78-86`, `:305`).
- **What may leave the host.** A redaction policy over the snapshot and every event; tool detail off by default; attachments refused from remote in v1 (`crates/retinue-daemon/src/remote/web.rs:397-407`).
- **Push.** Web Push, RFC 8291/8292, payload exactly `{title, sessionId}` (`crates/retinue-daemon/src/remote/push.rs:469-474`; `THREAT_MODEL.md:265-281`).
- **Inbound from outside.** Only the signed automation hook `POST /api/hook/<id>`: HMAC-SHA-256 over `<timestamp>.<body>`, constant-time, 300-second window, replay cache, everything failing as the same 404 (`crates/retinue-daemon/src/webhooks.rs:25-101`; `THREAT_MODEL.md:283-301`).

The one sentence that governs all of it: *a message from a remote device is a prompt to a coding agent on the host, so remote access to prompts is remote code execution bounded by approvals* (`THREAT_MODEL.md:16-21`).

## 3. What each side does better

**nod does better**

1. **Reach without an install.** A device the owner already uses, no PWA, no tailnet, no port forward. Works from anywhere while the host is awake.
2. **Polling as a first-class mode.** Telegram `getUpdates` with a persisted offset, reset after 24 hours without an update ([`index.ts:357-386`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/index.ts#L357-L386)). No listener, no public address, no certificate.
3. **A candidate card before approval.** The owner approves a named sender (`@username`) rather than typing an id, and messages are still ignored until they do.
4. **Reply hygiene for private steps.** The reply is sanitised, private links are stripped, and a browser handoff or a payment sends the owner to the app instead.
5. **Delivery follows the channel.** A schedule created in Telegram answers in Telegram, verified against the channel identity of the message that created it.
6. **Fail-closed ambiguity on approvals.** An unclear answer expires the approval and cancels the turn rather than guessing.
7. **Idempotent intake.** A unique `source_id` plus a deterministic message UUID, so a retried webhook or a reconnect cannot double-run a request.

**Retinue does better**

1. **One policy table, enforced in two languages, with a parity test.** nod's authorisation is spread across `permitted()`, the transport's channel checks and the unattended/proactive guards. Nothing fails a build when a new method forgets a rule.
2. **A master switch over approvals, and quiet hours as a hard read-only override applied before dispatch.** nod has channel enable/disable and quiet hours that defer background assessments, but no equivalent of `approvalsFromRemote` gating whether a channel reply may ever answer a confirmation.
3. **Redaction is a field-level policy with golden fixtures in Rust and Vitest.** nod's channel sanitiser is a regex pass over reply text; it has no policy over what a *tool result* may carry into chat.
4. **The content never leaves the owner's own transport.** Remote stays on the tailnet or on the owner's certificate; push carries two fields. nod deliberately sends full prompts and replies through Telegram or Meta.
5. **The webhook is signed over the body with a timestamp window and a replay cache.** Telegram's webhook secret is a static header, so a leaked secret forges any body ([`index.ts:481-495`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/index.ts#L481-L495)).
6. **Attachments are refused from remote in v1.** nod's inbound media path (10 files, 50 MB, transcription, artifacts) is its widest surface.
7. **Machines and phones share one pairing, approval and tier path.** nod has no cross-device model beyond "one owner".

## 4. Ideas we could adopt

Ordered by how much of the threat model they move. "Learn only" means the design, not the code (Apache 2.0 would allow a copy with attribution, but our rule is no copied code).

### 4.1 A chat channel as a new paired-device kind, pull-only (Telegram first)

`kind: "channel"` beside `phone|tablet|browser|computer|machine` (`crates/retinue-daemon/src/remote/pairing.rs:407-410`), allowed only `viewer` and `chat` classes, and only over long polling so no listener and no public address exist.

- **Effort:** large. Telegram client, inbox/outbox tables, pairing candidate flow, a channel row on the conversation, delivery routing for schedules and tasks.
- **Risk:** the prompt and the reply now pass through Telegram's servers. One sender id is the only identity. A bot token is a host credential that must live in the credential store like a webhook secret.
- **Rules touched:** loopback rules and "no new listener" (17 October amendment) — polling keeps both; Remote tiers — a new device kind and two new tier notes; the 14 November amendment's rule that a sign-in, a payment or a post pauses for the owner — a chat reply must never carry those.
- **Learning from:** the pairing candidate card, the one-identity rule, the channel-on-the-turn routing.

### 4.2 Sanitised outbound replies, and private steps sent to the app

A channel reply may not contain a loopback URL, an app route such as `/settings` or `/approvals`, or an attachment path; a sign-in, 2FA, payment or post says "open Retinue" and stops there.

- **Effort:** small (a sanitiser plus tests), if done before any channel ships.
- **Risk:** an over-eager stripper eats a legitimate link; a "blocked" reply that says nothing is a dead end. Mitigate with the same fallback line nod uses.
- **Rules touched:** §19 C2 of the threat model (a bot acting as the owner on a signed-in site), and the rule that logs never carry content.
- **Learning from:** `channelText` plus `secureAction`/`browserHandoff`.

### 4.3 Pairing: code in chat, candidate card on the desktop, exact-hash compare

Reuse `pairing.rs`. Issue the code from the desktop, show a `t.me/<bot>?start=<code>` deep link, store only its hash with a short expiry, record the candidate with name and username, and accept nothing until the owner confirms.

- **Effort:** small; the state machine already exists.
- **Risk:** a stranger who guesses the code can register a *candidate* — bounded by the expiry, the attempt limit and the owner's card. A code typed into a chat is visible to the chat service.
- **Rules touched:** the Remote pairing rules (5 minutes, 5 attempts, owner approval) — reuse them verbatim so there is one policy, not two.
- **Learning from:** `channel-settings.ts` `issuePairing`/`pair`/`bind`, and the test that walks every refusal.

### 4.4 Approvals from chat: exact words or inline buttons, never a model

Retinue's `permission.reply` and `proposal.decide` are already class `approve`, tier `operator`, and refused unless `approvalsFromRemote` is on (`tiers.json:572-579`, `policy.rs:311-326`). A channel should keep that, and then answer only on an exact token ("approve" / "deny", case-folded) or a Telegram inline-button callback bound to the confirmation's own nonce. No second model decides consent.

- **Effort:** medium (callback handling, nonce, the race rules nod already solved with `FOR UPDATE OF a`).
- **Risk:** a channel cannot show Retinue's approval card, so the owner loses the diff, the command and the "Always allow" reasoning. Mitigate by never allowing `operator` on a channel in v1 — a chat device can ask and read, not approve.
- **Rules touched:** "approvals from remote are off by default"; the rule that Full access never travels; the audit rule (every decision gets a content-free line).
- **Learning from:** nod's fail-closed shape (an unclear answer expires the approval and cancels the turn); *not* its model interpreter — a model with a 0.99 floor is still a model deciding consent over text a stranger could have typed into a stolen phone.

### 4.5 Idempotent inbound ledger keyed by the provider's message id

One row per inbound message, a unique `source_id` as `channel:<provider>:<account>:<messageId>`, a state machine (pending → submitted / discarded / blocked), and a deterministic conversation message id so a retried webhook or a reconnect cannot start the same turn twice.

- **Effort:** medium. Retinue already has idempotent sends (`client_message_id`, `message_by_client_id`, `queue_item_by_client_id`), so the pattern is familiar.
- **Risk:** a duplicate-detection window too long would swallow a genuine re-send of the same text; key on the provider id, never on the text.
- **Rules touched:** none of the security rules; it is a storage and queue change, with the usual "never weaken a test" note.
- **Learning from:** `channel_inbox`/`channel_outbox`, and the deterministic UUID.

### 4.6 Typing and one edited progress line

A short "typing…" presence while a turn runs, and on Telegram one quoted line edited in place with the current step. nod's own docs are explicit that the final answer always arrives separately ([`channels.mdx:181`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/use/channels.mdx#L181)).

- **Effort:** small, given `ui/doingNow.ts` already produces the step text on our side.
- **Risk:** rate limits and edit failures; a progress line that carries a path or a command. Cap the text the way nod caps it (2500 characters, then 1000 for the status line).
- **Rules touched:** "no message text in logs"; redaction of tool detail.
- **Learning from:** `progressStep` and `Telegram.progress`.

### 4.7 WhatsApp: Cloud API only, if ever; not WhatsApp Web

The Cloud API needs a public HTTPS route, which for us means a §18-style route on the gateway, not a new listener — and only while Remote is on. Its inbound side is text only (the message schema accepts `type: "text"` and nothing else, [`index.ts:183-190`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/chat-sdk/index.ts#L183-L190)); outbound it sends PNG or JPEG under 5 MB; a reply is refused once 24 hours have passed since the owner's last message.

- **Effort:** large (a public route, a new threat-model section, Meta app review).
- **Risk:** a public route on Native is reachable by the whole internet; the 24-hour window makes a reply fail after a delay; Meta sees the content.
- **Rules touched:** the Native threat rows N1–N12 and §18's failure uniformity. Never copy Telegram's static-header check; use our HMAC-over-body with a timestamp window.
- **Verdict on Baileys:** do not adopt. It stores the owner's whole WhatsApp account as device keys and acts as the owner in every chat — the C2 risk in a stronger form — and their own docs warn of account restrictions. Learn only, and only to know what to refuse.

### 4.8 What we should not copy

- A model as the approval interpreter (§4.4).
- One shared conversation for every channel identity: fine for one owner, wrong the moment a second sender is allowed. If we ever allow a list, the conversation must be per sender.
- No per-source bucket or lockout on their webhook routes: the rate limiter is off globally and keyed by owner (`app.ts:309-312`), so a flood is bounded only by the platform's signature check. Our §18 bucket and lockout are better and should stay.

## 5. Open questions for the owner

1. **Is a chat app a paired device or a separate feature?** A new `kind: "channel"` reuses pairing, tiers and revocation; a separate feature needs its own switch, its own table and its own review. I recommend the device kind, capped at `chat`.
2. **Is "your prompts and replies pass through Telegram or Meta" acceptable?** It is a new egress row, stronger than push's `{title, sessionId}`. Should it be per-conversation opt-in, like the owner's signed-in browser in the 14 November amendment?
3. **May a chat device approve anything at all?** My recommendation for v1: no. Ask and read only; approvals stay in the app, where the card shows the diff.
4. **Polling only, or a public route when WhatsApp Cloud is wanted?** Polling keeps the loopback rule and needs no certificate; a public route is a new §18-class surface with a new threat section.
5. **One sender, or a small allow list?** nod allows exactly one. A list means one conversation per sender, which changes the "one thread per owner" model.
6. **Do we ship this at all before the Remote checklists in §8, §11 and §13 are closed?** A channel multiplies the ways a prompt can arrive; the owed review should cover it, not run beside it.

## 6. What this report did not establish

- Nothing was run. The clone was read, never installed or built.
- The `chat`, `@chat-adapter/*` and Baileys licences were not read (not in the clone).
- No claim here about Telegram's or Meta's current terms beyond what nod's own docs say.
- Retinue's Remote has not been tested against a live tailnet, a real phone or an outside attacker (§12 and §13 of the threat model); that limits how confidently any of this can be promised.
