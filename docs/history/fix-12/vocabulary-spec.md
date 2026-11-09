# FIX12: the characters are "bots" in Retinue and "blobs" in Stone

> **What this is:** Spec for FIX12: the characters are "bots" in Retinue and "blobs" in Stone.
> **Status:** historical
> **Last updated:** 2026-10-15

Written 15 October 2026. Owner directive: follow the Director's recommendation.
The look decides the word. `ui/vocabulary.ts` already exists:

```ts
entityNoun(style: ThemeStyle, { plural?, capital? }): string   // retinue -> bot(s), stone -> blob(s)
useEntityNoun({ plural?, capital? }): string                    // reads useThemeStyle()
```

and `themeStyle()` in `blob/retinue.ts` for non-React code. Default look is Retinue,
so the default wording becomes "bot".

Another lane (FIX11) owns `apps/desktop/src-tauri`, `scripts`, `tauri.conf.json`
and the data-directory code in `crates/retinue-daemon/src/main.rs` (~line 3331) and
the claude adapter temp path. Do not touch those.

## 1. Scope

**Rename the word in what a person reads** in `apps/desktop/src`: visible text,
headings, buttons, `aria-label`, `title`, placeholders, empty states, toasts,
confirm dialogs, onboarding copy, settings copy, hints, help tooltips. Keep case and
number ("Blob name" -> "Bot name", "Your blobs" -> "Your bots", "a blob" -> "a bot").

Examples seen today: "Create blob", "Blob name", "Meet your first blob", "Blob
colour", "Keep your blob on screen while you work", "Keep this computer awake while
blobs work", "Rename blob", "Blob page".

**Do not rename:** CSS class names, file and component names (`BlobPage.tsx`),
`data-*` attributes, setting keys, RPC method names and params, event names, tool
names (`list_blobs`, `message_blob`), localStorage keys, test ids, the look names
**Stone** and **Retinue**, and any text that quotes an external product.

## 2. How

- In components: `const noun = useEntityNoun()` (and `{ capital: true }`,
  `{ plural: true }`). In event handlers and helpers: `entityNoun(themeStyle())` at
  the moment of use, never at module load, so a look change shows up without a
  restart.
- Module-level constants that hold copy become small functions.
- `aria-label` values that code or tests query (`[aria-label="Create blob"]` in
  `App.tsx`) must move together with their queries; prefer a stable `data-testid`
  or class for new queries, and keep behaviour identical.
- Count: expect on the order of 100-200 strings; `git grep -n -i "blob" apps/desktop/src`
  lists them. Classify each hit (copy / identifier) and record the identifier ones
  you kept in the report.

## 3. Daemon prose

Model-facing and notification text should follow the same word, so a model does not
call the user's teammate a "blob" in a Retinue app. In `crates/retinue-daemon/src`
(`team.rs` instructions "You are {name}, a blob: ...", tool descriptions, notices):

- add `fn entity_noun(...)` that reads the `appearance.style` setting through the
  storage handle that is already in scope (default `retinue` -> "bot",
  `stone` -> "blob");
- use it in the sentence builders only; **tool names, JSON field names and ids stay**;
- if a call site has no cheap access to the setting, say "teammate" instead.

Changing instruction text changes the instruction hash, which only causes a re-send
on the next turn; confirm no test pins the old sentence, and update such tests.

## 4. Tests

- Update every test that asserts the old word.
- Add: onboarding shows "Create bot" in Retinue and "Create blob" after
  `setThemeStyle('stone')`; a Settings label flips; the daemon sentence builder
  returns each word for each style.

## 5. Gates

`npm run typecheck`, `npm run test`, `cargo test -p retinue-daemon --bins`,
`cargo clippy --workspace --exclude retinue-desktop --all-targets -- -D warnings`.
Never commit; leave the diff and write `docs/history/fix-12/report.md` (counts, kept
identifiers, anything ambiguous).
