# 04 — The vault: cards, passwords and other secrets

Research for the owner, 16 November 2026. Read-only: no Retinue code was changed, no commit was run.

**Their source:** public, at https://github.com/n8n-io/nod. Clone at `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-vault`, shallow, commit **`47cfee3fe77279e1d543a367131d3ff2c1202e18`** (16 November 2026, "fix(agent): balance feedback during long tasks (#975)"). Every `repo path:line` below is a link to that commit.

**Licence:** `LICENSE:2` is the **Apache License 2.0**, copyright n8n GmbH, with a `NOTICE` file (`NOTICE:1-3`). It is *not* n8n's Sustainable Use License, and it is OSI open source. Copying code is therefore permitted with attribution. We still should not: `AGENTS.md` and the brief for this research say not to, and a copied file would need a line in `docs/legal/third-party-notices.md`, a "modified" notice on the file, and the NOTICE text. Everything below is design-level, so the answer for every idea is **learn from the design; do not copy code**. One caveat: the desktop app bundles a phonemiser under GPL-3.0-or-later (`NOTICE:6-10`), which is a different licence from the root; it is not in the vault path.

## Summary

1. Nod keeps website logins, authenticator keys and cards as **AES-256-GCM** rows in its Postgres database, bound to owner, origin and record id, under a 32-byte master key kept outside the database: the macOS Keychain, a 0600 file, `NOD_VAULT_KEY`, or Electron's `safeStorage` (DPAPI on Windows) in the desktop app.
2. **The model never receives a value.** A `sign_in` tool returns only "filled", an approval question, or a private form link; the server types the secret into the page itself, then redacts it from page text and masks it in screenshots and videos.
3. Consent is per use: a private form the owner types into, or the LLM-based trust check reading the owner's messages, bound to the exact origin and item; each use is claimed once in a durable row that ends `submitted` or `uncertain` and is never replayed.
4. Every use, reveal and held attempt goes to a content-free `vault_activity` ledger: action, origin, item id, the task's first line, the reason and the owner's basis, never a value.
5. Retinue has no website vault. We store bot secrets as **Windows Credential Manager** entries read only at spawn, and we hand a one-use secret to the **model** as a private file. Our gaps: the model holds the value, our browser snapshot echoes field values, our sign-in pause is a keyword hint, and we have no use ledger.

## How nod does it

### The pieces

| Concern | File | What it does |
|---|---|---|
| Encryption | [`src/server/vault-crypto.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-crypto.ts#L3-L19) | `aes-256-gcm`, 12-byte random nonce, 16-byte tag, associated data = `JSON.stringify(binding)`, stored as `base64(nonce‖tag‖ciphertext)` |
| Entries, cards, requests, routes | [`src/server/vault.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L541-L541) (2,400 lines) | save, list, request, private form, consume, fill |
| One-use claims | [`src/server/vault-use.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-use.ts#L66-L181) | `vault_uses`: `running` → `submitted` or `uncertain` |
| Consent and audit | [`src/server/vault-access.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-access.ts#L46-L215) | `vault_activity`, `authorize()` |
| Key store, 3 backends | [`src/ports/key-store.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/ports/key-store.ts#L1-L5), `providers/{keychain,filesystem,environment}/key-store.ts` | `getKey(): Promise<Buffer>` only |
| Private entry into a page | [`src/providers/agent-browser/index.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L1355-L1433), [`devtools.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/devtools.ts#L46-L104) | `privateEntry`, `arm`/`insert`, `mask` |
| Cards and payments | `src/server/payments.ts`, [`docs/…/trust/payments.mdx`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/payments.mdx#L28-L43) | one approved payment, security code never stored |
| Desktop key | [`desktop/main/settings.mjs`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/desktop/main/settings.mjs#L160-L242) | Electron `safeStorage` protects a generated key |
| Docs | [`docs/…/trust/vault.mdx`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/vault.mdx#L54-L148) | the owner-facing description |

### Where the key comes from

Nod picks one backend at boot ([`composition.ts:328-342`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/composition.ts#L328-L342)):

1. `NOD_VAULT_KEY` set → environment backend (base64 of exactly 32 bytes, [`environment/key-store.ts:3-19`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/environment/key-store.ts#L3-L19)). This is also how the desktop app works: its main process passes the key in the engine's environment.
2. Desktop mode with no key in the environment → the environment backend with no value, i.e. Vault refuses to open until the key arrives.
3. macOS source install → Keychain, service `dev.nod.vault`, account `master-key-v1`, through `/usr/bin/security find-generic-password` ([`keychain/key-store.ts:22-59`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/keychain/key-store.ts#L22-L59)). It creates the key only when no sealed row exists yet.
4. Linux source install → `NOD_DATA_ROOT/vault/master-key`, created `0600` inside a `0700` folder, read with `O_NOFOLLOW`, ownership and mode re-checked, file capped at 128 bytes, written through a temp file then `link()` so a race cannot clobber it ([`filesystem/key-store.ts:14-102`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/filesystem/key-store.ts#L14-L102)).
5. **Windows source install** → environment backend with no value, i.e. Vault is *unavailable* until `NOD_VAULT_KEY` is set. The docs' table lists only macOS, Linux and self-hosted ([`vault.mdx:135-140`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/vault.mdx#L135-L140)); there is no DPAPI or Credential Manager backend in the server.

On Windows the working path is only the desktop app. Electron's `safeStorage` encrypts a randomly generated 32-byte key into the settings file ([`settings.mjs:383-391`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/desktop/main/settings.mjs#L383-L391)), then hands it to the engine as `NOD_VAULT_KEY`. They explicitly refuse to run when the Linux keyring falls back to plain text (`getSelectedStorageBackend() === 'basic_text'`, [`settings.mjs:164-176`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/desktop/main/settings.mjs#L164-L176)). There is **no master password, no passphrase and no unlock step**: the key is unwrapped whenever the server starts.

Before any use, the key is checked against stored data ([`vault.ts:262-330`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L262-L330)): a `vault_key_check` row holds a known plaintext sealed with the key; if that row exists, the key must open it; if it does not, every existing ciphertext row is tried. A wrong key fails closed with "The Vault key does not match" and nothing is overwritten. `checkedKeyStore` caches the verified key for the process.

### What is encrypted, and how it is bound

Each value is sealed with associated data that names its purpose and its row, so a copied ciphertext cannot be moved to another row, owner or table ([`vault.ts:408-436`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L408-L436)):

- login: `['nod-vault-v1', owner_id, origin, id]`, JSON `{username, password, authenticator?}`
- card: `['nod-vault-card-v1', owner_id, id]`, JSON `{cardholder, cardNumber, expiryMonth, expiryYear}` ([`vault.ts:508-521`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L508-L521))
- app grant: `['nod-app-grant-v1', owner_id, id]` ([`vault-app-grants.ts:6-9`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-app-grants.ts#L6-L9))

The database still holds, in clear: the label, the **exact origin**, and `username_hint`, which is the **first two characters** of the username ([`vault.ts:747`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L747)). Cards keep brand, last four and expiry in clear ([`vault.ts:524-535`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L524-L535)). The **security code is never stored** ([`payments.mdx:24`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/payments.mdx#L24)).

An origins rule applies at save time: `https:` only (loopback may use `http:`), no path, query, fragment, user or password in the address ([`vault.ts:363-406`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L363-L406)). A login is therefore usable on exactly one origin: no subdomain, no protocol change, no port change ([`vault.mdx:84-93`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/vault.mdx#L84-L93)).

### Data flow: a secret reaching a page

1. The model calls `sign_in` with the origin, a label, the field selectors it saw, and optional `submitSelector` ([`runtime/tools.ts:384-419`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/tools.ts#L384-L419)).
2. The server inspects the page and **rejects** if the live page origin differs from the requested origin, if the chosen fields are the same, or if a code target repeats ([`vault.ts:1124-1181`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1124-L1181)). The model can choose where the value goes, but not on which site.
3. Consent is decided (`authorize`, below). If it is not granted, the tool returns an approval question instead of filling.
4. `enterLogin` opens one transaction: it locks the thread, claims the request (`vault_uses` row, `prepare`), re-reads the entry `FOR SHARE`, decrypts it, and marks the request `consumed` ([`vault.ts:1537-1762`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1537-L1762)). A second private entry of the same kind and origin for the same task step is refused by `automaticUse` ([`vault.ts:1059-1076`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1059-L1076)).
5. `perform` calls the provider's `injectCredentials`. That checks the page origin again, checks **each field's real origin including frames**, focuses the field, **arms** it (marks it private, turns off autocomplete, selects and deletes its contents), then types the value **one character at a time** with `document.execCommand('insertText')` in an isolated world ([`index.ts:1402-1433`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L1402-L1433), [`devtools.ts:791-811`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/devtools.ts#L791-L811)). A DOM change during entry aborts with "check the result before another attempt". The submit control is clicked only if it is on the same page and its origin is allowed.
6. The whole operation runs inside `privateEntry`, which sets `state.injecting`, stores the document id as `privateDocument` and `saved.sensitive`, and holds a lock. While it is set, page scripts, evaluate, screenshots and videos are refused with "Browser observations are paused during credential entry" ([`index.ts:1355-1400`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L1355-L1400), class at `:120-122`).
7. The completion step re-locks, records the selection (so a later password step for the same account is bound to it), writes the audit row, and resumes the waiting turn ([`vault.ts:1778-1843`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1778-L1843)).
8. The model gets `{filled: true}` or a page to inspect. It never gets the value.

### What the model receives

| Item | To the model |
|---|---|
| Saved login | name, origin, first two characters of the username ([`vault.mdx:74-82`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/vault.mdx#L74-L82)) |
| Saved card | name, brand, last four, expiry |
| Password, code, card number | only that the private step finished |
| Authenticator key | nothing; the server computes the 6-digit code and types it |
| Page after sign-in | live page, with secret values replaced by `[redacted]` in text and masked on screen |

Three layers enforce that last row:

- **Text redaction.** Every observation, script result, dialog message, page error and URL passes through `redactBrowserText`/`redactBrowserUrl`, which remove the plain value, its JSON-escaped form, its percent-encoded forms with either case, and for 12–19 digit numbers a digit-separated match ([`index.ts:128-152`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L128-L152), `:198-221`). URL query and fragment parameters named `password`, `token`, `otp`, `session`, `ticket`, `signature` and similar are replaced outright ([`index.ts:202-206`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L202-L206)).
- **Screen masking.** A script hides any field holding a known secret (`opacity: 0`, or `type="password"` in snapshot mode) and any text node containing one, then re-applies on DOM mutations ([`devtools.ts:48-104`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/devtools.ts#L48-L104)); screenshots call it before capture ([`index.ts:1965-1993`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L1965-L1993)).
- **Script pause.** Read-only scripts are fine; a document changed by a script must reload before entry, and a page where a private value is held refuses scripts until it reloads or navigates ([`index.ts:1150-1184`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L1150-L1184)).

The prompt also tells the model, in words, never to ask for a password, card number or key in chat and to use `sign_in` or `pay` instead ([`runtime/prompt.ts:56`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/prompt.ts#L56), [`server/communication.ts:34`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/communication.ts#L34)).

One extra trick worth naming: when **the owner** types into a password field during a takeover, Nod reads `field.value` afterwards and adds it to the secret set, so it is masked from then on ([`index.ts:2048-2074`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L2048-L2074)).

### Consent per use

Two routes, and a yes is narrow.

- **The owner types it.** The tool returns `{url: '/vault/<token>'}`. `<token>` is a 43-character HMAC of the request id ([`vault.ts:1183-1187`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1183-L1187), `:1326-1330`); only its SHA-256 is stored. The page lists the saved logins for that origin **with their full usernames**, so the owner can pick the right account, and their passwords never appear ([`vault.ts:1379-1443`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1379-L1443), `vault.mdx:82`). Entering a value in that form *is* the consent for that step ([`AGENTS.md:38`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/AGENTS.md#L38)). A form is single use: a second submit is refused because the request is no longer `pending` (`consume`, [`vault.ts:1846-1936`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1846-L1936)).
- **The owner already said yes in chat.** `authorize` first looks for a completed `NOD_vault_allow` approval in this task's turn chain with exactly these arguments; failing that it asks the **trust check** ([`vault-access.ts:163-215`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-access.ts#L163-L215)). The check is an **LLM decision**, not a rule engine: its instructions are in [`trust.ts:23-46`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/trust.ts#L23-L46). If it cannot confirm, the tool returns a question card; the owner's yes is stored against the exact origin, kind, item id and session origin, and covers the rest of that task. "A domain mention does not grant consent" (`AGENTS.md:38`). A scheduled run is checked against the instruction that made the schedule.

Both routes end in the same one-use claim. `vault_uses` rows are keyed by id and refused on conflict, a `running` row left by a crash becomes `uncertain` at next boot, and an `uncertain` operation is reported as "it may have reached the website; check the result before another attempt" rather than retried ([`vault-use.ts:74-181`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-use.ts#L74-L181)).

### Autofill, saved sessions, payments

- **Autofill** exists only inside Nod's own browser: its own Chromium profile per thread, never the owner's browser. Clicks and typing in the general browser tool are separate from `sign_in`, and the trust check judges each committing step ([`AGENTS.md:36`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/AGENTS.md#L36)).
- **Saved browser sessions** are the fallback for magic links and phone codes. They are stored as **the browser profile: cookies and site storage, not encrypted by the vault** ([`browser-sessions.md:43-55`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/browser-sessions.md#L43-L55)). Using one needs the same check, and the check also considers which site the session belongs to.
- **1Password**: the SDK runs in a separate helper process, lists only what 1Password would fill on that exact origin, and the picked item is **copied into Vault**, so later background runs do not need 1Password (`AGENTS.md:38`, `vault.mdx:44-52`).
- **Payments** are stricter than sign-in: one request bound to the card's version, the shop origin, the exact page, the real origin of every card field including payment frames, the one submit control, and the amount and currency read from the page; the owner then enters the **security code for that payment** ([`payments.mdx:28-43`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/payments.mdx#L28-L43)). A newer message in the same conversation, a changed card or Stop makes it unusable.

### Sync and backup

- **No sync.** A desktop client uses the *server's* vault and the server's key; the client's own key store protects only its `serverToken` ([`vault.mdx:144`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/vault.mdx#L144)). Every linked desktop talks to one database, so there is nothing to merge.
- **Backup is the database plus the key, separately.** If you set the key yourself there is no recovery: keep a copy, because a database backup alone cannot recover saved credentials, and Nod never generates a replacement key when sealed data exists ([`configuration.md:123-125`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/deploy/configuration.md#L123-L125), `vault.mdx:146-148`). The docs also tell admins to keep an encrypted backup outside the machine ([`boxd.md:78-84`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/deploy/boxd.md#L78-L84)).
- **Import only.** CSV import from Chrome, 1Password or Bitwarden, including authenticator keys; there is no CSV export ([`vault.mdx:30`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/vault.mdx#L30), `vault.ts:755-844`).

### Audit

`vault_activity` stores, per action: `at`, `action` (`sign_in`, `code`, `session`, `card`, `held`, `asked`, `reveal`), `origin`, `item_id`, `item_kind`, the task's first prompt line cut to 120 characters, a `reason` (`asked`, `schedule`, `owner`, `entered`, `picked`), an optional `basis` JSON (which message or schedule the consent came from, and whether that message still exists), and for payments the `amount_minor` and `currency` ([`vault-access.ts:59-70`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-access.ts#L59-L70), `:103-144`, `:235-251`). No value, no selector, no page text. It is shown in Usage with a weekly count of sign-ins, payments and held attempts ([`vault-access.ts:294-331`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-access.ts#L294-L331)), and a reveal is logged too ([`vault.ts:2262-2293`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L2262-L2293)). Analytics gets only a kind and a status: `secure_entry_changed {kind: payment|code|sign_in, status}` ([`vault-use.ts:17-29`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault-use.ts#L17-L29)).

### Weaknesses I found in their design

1. **No unlock, no step-up.** Anyone who can run as the owner's OS user, or who has the app's access cookie, can open every saved login. `/api/vault/:id/password` returns the password and `/api/vault/:id/card-number` returns the card number, with an empty body and no re-authentication; the only gate is the access cookie, which is `HttpOnly; SameSite=Strict` and valid for 400 days ([`access.ts:170`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/access.ts#L170), [`vault.ts:2262-2293`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L2262-L2293)). They say so: "There's no isolated key service or hardware-backed key release" ([`security-model.md:88`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/security-model.md#L88)).
2. **The master key sits in a child process's environment.** In the desktop app the main process decrypts `vaultKey` and passes it to the engine as `NOD_VAULT_KEY` ([`settings.mjs:391`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/desktop/main/settings.mjs#L391)). A child's environment is readable by other processes of the same user, and it is inherited by anything the engine spawns, so the blast radius of a leak is *all* saved logins, not one. Their mitigations are container isolation for the workspace and adding the key to a log-redaction set ([`main.mjs:2236`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/desktop/main/main.mjs#L2236)).
3. **Consent is model-decided.** The gate between a saved password and a page is an LLM reading the owner's messages. It is well prompted and benchmarked, but a misclassification is a silent approval, and the thing it is protecting is a credential. Retinue's equivalent decisions (permission cards) are deterministic.
4. **Saved browser sessions are unencrypted.** A session cookie is a full bearer credential for the site, and it sits in browser data with only the machine's file permissions ([`browser-sessions.md:54`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/browser-sessions.md#L54)).
5. **No key rotation.** A changed key fails verification and the check row is re-sealed with whatever key is present ([`vault.ts:262-330`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L262-L330)); with no re-encrypt path and no export, rotating means rewriting everything by hand.
6. **A second encrypted copy of a password, briefly.** `credential_account_selections` holds the staged username and, between the identifier step and the password step, the **held password**, sealed, for up to ten minutes ([`vault.ts:1078-1117`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/vault.ts#L1078-L1117), `:1675-1678`, `:1804`). More copies, more windows.
7. **Redaction is string matching.** A password rendered with spaces, split across elements, drawn to a canvas or transformed would not match. They handle digit separators for card numbers only ([`index.ts:142-144`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L142-L144)).
8. **The page's own JavaScript can read the field.** `insertText` puts the value into the site's DOM; the protection is from the *model*, not from the site. That is unavoidable for autofill, but it means a malicious script on a trusted origin still sees the password, and their origin checks only prove which origin it is.
9. **Windows is a second-class server platform.** No DPAPI or Credential Manager backend exists outside the Electron wrapper; a Windows source install or a Windows self-host needs the key in an environment file.
10. **A deceptive site can mislabel a control.** They state it plainly: the trust check sees the label, not the page text, so a script-driven control can claim to do one thing and do another ([`security-model.md:92`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/trust/security-model.md#L92)).
11. **`username_hint` is plaintext metadata.** Two characters of each username identify accounts without the key.

## How Retinue does it now

We have no website vault and no autofill. Our secret surface is five things.

### 1. Bot secrets: Windows Credential Manager

`crates/retinue-daemon/src/secrets.rs`. A bot's environment variable can point at a named secret (`agent.envRefs`), stored as a generic credential `Retinue/<name>` with `CRED_PERSIST_LOCAL_MACHINE` (`secrets.rs:174-273`). The value is read **only at spawn** into the child's environment (`secrets.rs:113-124`), never stored in SQLite (only names, `crates/retinue-storage/src/secret_refs.rs:12-24`), never returned by any RPC (`secrets.rs:294-313`), never exported. The module says the honest limit: "A coding agent can read its own environment, so this protects the value at rest, not from the bot" (`secrets.rs:7-8`).

Refused variable names are fixed (`PATH`, `NODE_OPTIONS`, `RETINUE_*`, the agents' own config dirs, `secrets.rs:52-61`), so a bot cannot use a secret to change how it starts. Deleting a secret that a bot still references is refused (`secrets.rs:314-323`).

### 2. One-use secret requests: the model *does* see the value

`request_secret` (`secrets.rs:386-401`, `:423-494`): the request lives **in memory only**, one open per conversation, the turn waits up to 50 s, and the owner gets a private dialog (`apps/desktop/src/ui/SecretsPage.tsx:69-106`, mounted by `SecretRequests` at `:107`). On an answer the daemon writes a file in `<data>/one-use/<id>` created empty then chmod-ed to the user then filled (`secrets.rs:496-510`), and tells the bot the path: "Read it when you need it, and don't print it. It is deleted when this turn ends." The file is removed when the turn ends (`secrets.rs:513-521`) and swept at startup (`:407-415`). Unattended conversations are refused before a dialog is shown (`secrets.rs:435`, `crates/retinue-daemon/src/questions.rs` `may_ask`).

So the model reads the plaintext, by design, and the value is in a file for the rest of the turn.

### 3. Browser and computer sign-in: a keyword pause

`crates/retinue-daemon/src/browser.rs`. The linked computer's Chromium is in Retinue's own profile, one per conversation, never the owner's (`browser.rs:1-35`). Before a click, a type or a key, the daemon asks the page what the target is and calls `pause_reason` (`browser.rs:505-539`): a password field, a `cc-*` field, a `one-time-code` field, a field plus button that look like a sign-in form, or a post/pay/submit button each return `needs_owner` with an instruction to ask the owner and end the turn (`browser.rs:541-547`). This matches the 14 November rule "every sign-in, 2FA, payment or post pauses for the owner" (`AGENTS.md:10`). Typing records only a count, never the text (`browser.rs:870-876`, `AGENTS.md:14`).

### 4. Tokens and hashes around the daemon

- **Daemon capability:** a fresh random token on every start, held in memory (`crates/retinue-daemon/src/main.rs:6061`), required on every RPC (`main.rs:520-529`), never accepted from a non-loopback peer (`main.rs:465-470`), never the outside-hook or MCP token (`main.rs:471-495`). Bridges get the conversation's own capability written into a user-only endpoint file (`browser.rs:1127-1150`), swept at startup (`browser.rs:1152-1163`).
- **Remote password:** Argon2id PHC string in a setting, never logged or returned, a dummy-hash verify so a wrong username costs the same time (`crates/retinue-daemon/src/remote/native.rs:59`, `:142-197`). Backups refuse every `remote.*` key (`crates/retinue-daemon/src/backup.rs:151-160`).
- **Machine-link device token:** a user-only file `<data>/machines/<id>.token`, never the database, read for one request, never returned (`crates/retinue-daemon/src/machine_client.rs:1-18`, `:341-370`; `AGENTS.md:33`). Files are restricted with `icacls /inheritance:r /grant:r <user>:F` (`remote/native.rs:381-399`).
- **Harbor token for the linked computer:** Credential Manager, read per request, never sent to the window (`crates/retinue-daemon/src/computer.rs:9-11`, `:321-334`).
- **Usage sign-in read:** off unless the setting is exactly `true` (`main.rs:986-989`), refused by the backup export and by a restored backup (`backup.rs:159-161`, tests at `main.rs:7438-7448`); `AGENTS.md:44`.

### 5. What leaves, and what is recorded

- **Backups** carry a bot's environment *variable names* only, never secret names or values (`backup.rs:221-222`); settings whose key contains `token`, `secret`, `password`, `auth`, `cookie` or `key` are refused, with `companion.hotkey` the one exception (`backup.rs:176-179`).
- **Diagnostics** pass every log line through a redacting writer and scrub settings and events; keys named `secret`, `password`, `credential`, `authorization`, `cookie`, `api_key`, `capability`, `token`, `hash` are replaced (`crates/retinue-daemon/src/diagnostics.rs:67-102`, `:164-187`), and Argon2 strings are matched directly (`diagnostics.rs:77-78`).
- **Remote:** `secret.list`, `.set`, `.delete`, `secret.request.list`, `.answer`, `.decline` are all `never` for a device, and `secret.request.changed` is `drop` (`apps/desktop/src/remote/tiers.json:313-351`, `:1779`).
- **The conversation** gets one note: "You gave <bot> <name> for this turn" (`secrets.rs:481`). That is the whole audit trail.

### Weaknesses on our side

1. **The model holds the value** (`secrets.rs:482`). One leaked transcript, one `cat` in a tool row, one echo in a shell and the secret is in the conversation, which is stored and sent to a provider.
2. **Our browser snapshot echoes field values.** The element extractor uses `el.value` as the element's name (`browser.rs:165`, and `:191`, `:202` for the target under the pointer), so a value typed into any field — a password field included — comes back in the next snapshot, to the model, and into the transcript. Nothing masks a `type="password"` value.
3. **The sign-in pause is a hint, not a gate.** `pause_reason` is keyword matching on labels the site itself writes, and `needs_owner` is an instruction. A page that labels a field "PIN", "passcode", "card security code"-adjacent or in another language passes, and a bot that ignores the instruction is not stopped.
4. **No use ledger.** There is no durable, content-free record of "this secret was handed to this conversation at this time" beyond the note, and nothing the owner can review later.
5. **50 seconds is short** for finding a token (`WAIT_SECS`, `secrets.rs:39`); a missed answer means the bot carries on without it.
6. **A screenshot of the linked computer is not masked.** `computer.screenshot` is a media attachment (`AGENTS.md:9`); if a bot typed a secret on screen, the picture can carry it, and screenshots of the *linked* computer do not need the owner's frame approval (only the owner's own computer does, `AGENTS.md:13`).

## What they do better, and what we do better

**Theirs is better at**

| Topic | Their design | Our equivalent |
|---|---|---|
| Model exposure | the model never receives a value; the server types it | the model reads the one-use file |
| Sign-in gate | server-side fill with per-field real-origin checks and a one-use claim | a keyword pause and an instruction |
| Replay | `vault_uses` claim, `running` → `uncertain`, never auto-retried | a lost answer is simply lost; nothing to replay, nothing to check |
| Screen and text hygiene | known-secret set, masked screenshots, redacted text and URLs, scripts paused | none of it |
| Audit | `vault_activity` with reason, basis, task, amount | one note |
| Payment | one request bound to card version, page, field origins, control, amount; security code per payment | a pause |

**Ours is better at**

| Topic | Our design | Their equivalent |
|---|---|---|
| Key placement | one Credential Manager entry per secret, read at spawn | one master key, decrypted at boot, in a child's environment |
| Reveal | no RPC and no UI returns a value at all | a reveal button returns the password and the card number |
| Remote | secrets are `never` for a phone, requests cannot be answered remotely | the same access cookie reaches `/api/vault/*` |
| Unattended work | a scheduled run cannot ask for a secret | background work can use saved logins after a check |
| Backup and diagnostics | key-name and pattern redaction, env-ref names only | not shown, but a database dump plus the key is the whole vault |
| Consent determinism | permission cards and rules, not a model | an LLM decides the sign-in |

## Ideas we could adopt

All of these are **learn from the design**. None needs their code; none should be copied.

### 1. Stop the snapshot echoing field values, and redact known secrets (recommended first)

**What.** (a) In `browser.rs`'s element extractor, never use `el.value` as the name for `type="password"`, and mask a value that matches a known secret; print "(filled)" or a length instead. (b) Keep a per-conversation set of values the daemon knows: what the owner typed into the answer dialog for `request_secret`, and — later — what a `fill` tool typed. (c) Replace those values in every observation, `describe`, dialog message, error and URL, and mask them in screenshots.

**Effort:** small for (a) and a redaction helper; medium for (b) and (c). **Risk:** low. It removes a leak rather than adding a capability, and the set never leaves the daemon.

**Rules it touches:** `AGENTS.md:14` (what is recorded is content-free) and `AGENTS.md:9-13` (a screenshot is a media attachment, screens are sent only when the owner sends them). No new egress.

### 2. A content-free use ledger for secrets

**What.** A `secret_uses` table: `id, secret_name, session_id, turn_id, at, outcome` and, like their `task` column, the conversation's title cut short. Written when an answer is given, when a bot's spawn reads a referenced secret is *not* logged (too noisy) — only handoffs to a turn. Surface it in Settings › Secrets ("used 3 times, last on 12 Nov in *Invoice parser tests*") and expose the same fields through the existing `secret.list` shape. `drop` it on Remote, as `secret.request.changed` is now.

**Effort:** small. **Risk:** low. The hard part is choosing the fields; the discipline is not to add the value or the reason text.

**Rules:** `AGENTS.md:14`; `tiers.json` needs a `drop` row for the new event; `backup.rs`'s `exportable_setting` is unaffected because this is a table, but consider whether a backup should carry it (recommend no).

### 3. A one-use claim with an `uncertain` outcome, for anything with a side effect

**What.** Their `vault_uses` pattern generalised: a row claimed before the operation (`running`), finished as `submitted`, or left `uncertain` on a crash and reported as "may have happened; check before retrying". Retinue already has pieces of this (`client_message_id` idempotency, `delivery_unknown` for sends, the pull-request draft). The new part is the durable, per-operation claim that a crash turns into an instruction, not a retry.

**Effort:** medium. **Risk:** low, if scoped to new operations only (a `fill` tool, an owner-approved sign-in step, a payment step). Rewriting existing paths would be large and is not needed.

**Rules:** the daemon owns decisions; content-free records; `never` on Remote for the underlying operations.

### 4. Fill a website field from the daemon, so the model never holds the value

**What.** A browser tool `browser_fill_secret {ref, secretName}` for the linked computer. The daemon: refuses if the target is not a password, OTP or card field by the page's own type and autocomplete; checks the field's real origin against the page's origin; reads the value from Credential Manager **inside the daemon**; sends it to the container's Node server over the existing loopback MCP path; types it there; returns `{filled: true}` and nothing else; records a ledger row with the host only. The value never enters the model's context, the transcript, an event or a log. Pair it with Idea 1 so the next snapshot cannot echo it.

This is the core of their design and the biggest win. It also replaces the "the model reads a file" step for browser work, and it is a natural companion to `request_secret` (which can stay for shells and CLIs).

**Effort:** large. The fill itself is small; the surrounding work is a consent card, a `needs_owner` path when the page looks like a sign-in rather than a step inside one, tests for the origin checks, and a decision about what the transcript shows.

**Risk:** medium-high. It creates a new path where the daemon hands an owner secret to a web page on the owner's linked computer. It must stay attended-only (as `browser.rs` already is), `never` on Remote, and the field's origin must be checked in the container against the page, because the container is where the page lives. Consider also refusing to fill on a page whose URL is not the same origin as the field.

**Rules:** the 14 November amendment (`AGENTS.md:4-17`) — this is browser work under the pause rule, and it needs a sentence of its own in that amendment ("the daemon fills; the model does not see"); `AGENTS.md:14`; the Remote tiers (`never`, and the existing `browser.*` family stays `never`); `docs/remote/THREAT_MODEL.md` gains a row for "the model causes a stored secret to be typed into a page". It does **not** touch the loopback rule (the container call is loopback) and does **not** touch "no copied provider authentication".

### 5. Bind a sign-in approval to the exact origin and item, once per task

**What.** Replace (or back up) the keyword pause for sign-in pages with an approval card: the daemon raises "Sign in to reports.example.com as work@…" with the origin, the field and the account hint; a yes is stored against that origin and item **for the rest of this conversation's task**; the card is single use; a later message in the conversation does not extend it. For a step the bot cannot do, the card offers the owner a link to take over the browser instead.

**Effort:** medium. **Risk:** medium: a miss where a bot acts as the owner is exactly what the 14 November rule exists to prevent, so the pause must stay even when an approval exists.

**Rules:** `AGENTS.md:10`; the Remote tier table (an approval from a phone stays off); the content-free record rule.

### 6. Learn a value the owner types during Take control, and mask it

**What.** In the linked computer's browser, when text is typed into a password field while the owner has control, read the field value once, keep it in the conversation's secret set (in memory, in the daemon), and redact and mask it from then on. Their version is [`index.ts:2048-2074`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/agent-browser/index.ts#L2048-L2074). Do the same for the owner's own computer only if the accessibility tree cannot already mask it (`AGENTS.md:13` says it does).

**Effort:** small. **Risk:** medium: it is the opposite of "never read the value" — the daemon reads a value the owner typed, and must keep it in memory for the conversation only, never in a log, event, backup or screenshot.

**Rules:** `AGENTS.md:9` and `:14`; the owner's computer stays `never` and attended.

### Where I would stop

A full password vault (their `vault_entries`, `vault_cards`, import, 1Password, sessions, payments) is a product-sized liability: no unlock, a long-lived access cookie, an LLM for consent, unencrypted sessions and no rotation, as listed above. Retinue's job is coding agents. Ideas 1 to 3 make our present paths safe; 4 and 5 cover the browser case the 14 November amendment opened; 6 closes a screenshot hole. A vault of our own should wait for an explicit owner decision and its own threat-model section.

**Recommended order:** 1 (fix the leak), 2 (ledger), 3 (claim), then 5 (approval card), then 4 (daemon fill), then 6. 1 and 2 are weeks apart in size and independent of any new product decision.

## Open questions for the owner

1. **May the model ever hold a secret?** Today `request_secret` hands it one. If the answer is no, Idea 4 becomes the default and `request_secret` keeps only shell and CLI use. This one answer decides the shape of everything else.
2. **Do we want stored site logins at all**, or only "fill this one value, once, from a secret the owner already saved"? A vault means import, edit, reveal, rotate and recover; nod shows how much that is.
3. **If we ever add an encrypted store**, do we want an OS-keychain-per-item design like today's Credential Manager, or a database with a master key and a key-check row? Their fail-closed check is good; their key-in-environment and no-rotation problems are the price.
4. **Should an approval from a phone ever cover a sign-in?** Today `secret.*` is `never`; a browser sign-in card would be the first candidate for an exception, and I would keep it off until streaming and step-up exist.
5. **Does a use ledger belong in a backup?** I would exclude it, like `usage.*`, but the owner may want the history to survive a move.
6. **Step-up unlock.** Nod deliberately has none. If we add a vault, does the owner want a passphrase (or Windows Hello) before a reveal or a fill that is not already covered by a card?
7. **A Windows gap on their side is worth knowing for the comparison:** their server has no DPAPI or Credential Manager backend at all. Our per-item Credential Manager choice is the stronger one on Windows, and I would keep it.

## Evidence

- Clone: `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-vault`, commit `47cfee3`. Read only; nothing built or run.
- Their files cited above by permalink. Owner-facing design: `docs/src/content/docs/trust/vault.mdx`, `payments.mdx`, `browser-sessions.md`, `security-model.md`, `docs/src/content/docs/deploy/configuration.md`.
- Our files: `crates/retinue-daemon/src/{secrets.rs,browser.rs,computer.rs,machine_client.rs,backup.rs,diagnostics.rs,questions.rs}`, `crates/retinue-daemon/src/remote/native.rs`, `crates/retinue-storage/src/secret_refs.rs`, `apps/desktop/src/ui/SecretsPage.tsx`, `apps/desktop/src/remote/tiers.json`, `AGENTS.md` (14 November 2026 amendment, 31 October machine-link amendment, 30 October usage amendment).
