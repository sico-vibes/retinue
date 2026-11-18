# WhatsApp Web: why this build refuses it

> **Status:** current, 16 November 2026. Owner decisions are in `AGENTS.md` (16 November 2026); the contract is `docs/plans/vault-channels-pi.md` section B. Backend lane B (Ada B).

`channel.whatsappWeb.start` answers `unsupported` with a plain sentence. The channel kind exists in the store and in the UI contract, but no client is shipped, and nothing is written to the database when the owner presses Start. This note records the decision, so the next lane does not have to re-derive it.

## What a WhatsApp Web channel needs

WhatsApp Web is a multi-device protocol: a linked device holds the account's own device keys, performs the Noise handshake, decrypts every chat (Signal double ratchet), and sends as the owner. The QR code is only the first step of a pairing that gives the client the account's identity keys.

There is no Node runtime on the owner's computer that Retinue may rely on, so the two shapes are:

1. **A Rust client crate.** The known candidates are ports of Baileys (`whatsapp-rust` and its forks). At the time of writing none is documented as stable, none states a WhatsApp-compatible release in `docs/operations/`, and none was verified in this lane: the lane had no network access to read a licence, a version or a test report. Adopting one therefore needs, before any code: the licence (must be shippable), the pinned version and a SHA-256, a statement of what it does and does not support (device pairing, groups, media), and a supervised-child design if it turns out to need a helper process.
2. **A pinned third-party binary run as a supervised child**, under the rules that already cover the computer-use driver (`AGENTS.md`, 14 November 2026; `docs/remote/THREAT_MODEL.md` section 19 rule C5): the licence, the version and the SHA-256 recorded in this folder, staged by a script that refuses a wrong hash, run as Retinue's own child behind Retinue's tools. For Baileys that means shipping Node, which is roughly a second of RAM per conversation and a new updater surface, for one channel.

## Why neither is in this build

- The owner accepted the ban risk for WhatsApp Web ("the owner accepts the ban risk", `AGENTS.md`, 16 November 2026). That is a risk decision, not an implementation. Both shapes are weeks of protocol work, not hours, and neither could be verified here.
- A half-built WhatsApp client is worse than none: it would hold the owner's account keys while sending nothing, and the paired sender could not be approved from a UI that expects a QR that never arrives.
- The official Cloud API covers the same need with a supported interface. Its outbound half is built (see the lane report); its inbound half needs a public route and is a separate decision.

## What was built instead

- The `whatsapp-web` channel row, its status machine, `channel.list`, `channel.update`, `channel.disconnect`, and the `channel.qr` event classification, so a later client has its place.
- `channel.whatsappWeb.start` refuses with: "WhatsApp Web isn't available in this build: it needs a WhatsApp client Retinue doesn't ship yet. Use the WhatsApp Cloud API, or ask for WhatsApp Web to be added."

## What a later lane must do first

1. Pick a client and record its licence, pinned version and SHA-256 here.
2. Prove the pairing path end to end against a throwaway number, including the QR expiry (60 s) and the setup window (5 min).
3. Keep the session material in a user-only folder under `<data>/channels/`, excluded from backups and diagnostics, and never in SQLite.
4. Refuse `status@broadcast`, `@g.us` and `@newsletter` before decryption, and allow exactly one sender.
5. Draw the QR in the daemon (the UI has no QR library) as a `data:` URL, and stop when the channel is disconnected.
