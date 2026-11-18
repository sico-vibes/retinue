# Wave 3, release safety (worktree `-11`)

> **Lane:** Ada, backend. **Sources:** `docs/plans/nod-adoption-plan.md` (wave 3, release safety), `docs/research/n8n-nod/07-architecture.md` ideas 4, 7 and 8. Idea 9 (a fingerprinted behaviour suite) is later and is not started.
> **Status:** implemented in this worktree, in the three sections below. Nothing is committed. What ran and what did not is in "Running log" at the end; read that before trusting a line.
> **Read this first:** "Contract" is what the Director builds a screen or a script from. A shape changes only with a line under "Changes" saying so and what the old shape was.
>
> **Changes to an existing shape (nothing else about them changes):**
> - The bot tool `read_conversation` returned a bare JSON **array** of messages; it now returns an **object** with `messages` and the page's cursor (section 3a).
> - `session.get` gained an optional `beforeMessageId` parameter and, for a device only, the window fields in section 3b. The desktop's call and reply are unchanged.

## Contract

### 1. CI split by path, a required `checks` job, and a nightly run (idea 4)

No RPC, no event, no bot tool. What the app and the Director see:

- `.github/workflows/ci.yml` gains a `scope` job and eight checks jobs, and keeps the existing `hygiene` job (formatting, boundaries, the checks' own tests, Gitleaks) content for content.
- `checks` is the one required job: it fails unless every check the `scope` job selected passed, and fails closed when the selection is missing or unreadable.
- A nightly run at 04:10 UTC runs every check regardless of paths.
- `scripts/ci-scope.mjs` is the path-to-check table; `scripts/ci-scope.test.mjs` pins it. Both run under `npm run test:scripts`.

```jsonc
// scripts/ci-scope.mjs, exported
selectChecks({ paths: ["apps/desktop/src/ui/App.tsx"] })
// ->
{ "full": false,
  "checks": { "hygiene": true, "types": true, "uiTests": true, "build": true,
              "daemonTests": false, "clippy": false, "docs": false, "perf": true },
  "reason": "the fast checks always run; the desktop app",
  "unknown": [], "everything": [] }
```

- `CHECKS` is the eight names, in the workflow's order; the workflow has one job per name, and every job is selected by `needs.scope.outputs.<name> == 'true'`.
- `full` is true for a scheduled or manual run, a first push, an unreadable diff, a lockfile or toolchain change, a change under `.github/`, `scripts/`, `apps/desktop/src-tauri/`, `packaging/` or `.signpath/`, or any path the table does not know.
- The CLI writes `$GITHUB_OUTPUT` (`--github-output`), a Markdown summary to `$GITHUB_STEP_SUMMARY` (`--summary`) and the JSON to stdout (`--json`).

### 2. Update round trip and the release gate (idea 7)

- `scripts/update-round-trip.mjs` (new) reads a `latest.json` the way the installed build's updater does, and answers whether the version it offers is newer than the installed one.
- `scripts/release-windows.ps1` runs it on the manifest it just wrote, and refuses when the offered version is not newer than the one `channel-beta` currently offers.
- `.github/workflows/release.yml` gains a `require-ci` job: the build refuses to start unless the CI run for that exact commit succeeded.

```js
// scripts/update-round-trip.mjs, exported
readManifest(json, { platform: 'windows-x86_64' })
// -> { version: '0.2.0-beta.5', notes: '…', pubDate: '2026-11-17T12:00:00Z',
//      url: 'https://…', signature: '…' }        // throws with a plain sentence otherwise
offersAnUpdate(manifest, '0.2.0-beta.4')           // -> true; equal or older -> false
```

```powershell
# release-windows.ps1, new refusal (non-dry-run only: it asks gh for the published channel)
node scripts/update-round-trip.mjs --manifest $manifestPath --installed <published channel version>
```

### 3. Bounded conversation reads (idea 8)

Two surfaces, both return a **window with a cursor**, never the whole conversation.

#### 3a. Bot tool `read_conversation` (Retinue MCP) — **reply shape changes**

Old reply: a bare JSON array of messages. New reply: an object. A client that read the array must read `messages`.

```jsonc
// params
{ "sessionId": "ses…",
  "limit": 50,                                  // 1..200, default 50
  "afterMessageId": "m-100",                    // optional: the messages after this one
  "beforeMessageId": "m-111" }                  // optional: the messages before this one
// reply
{ "messages": [                               // oldest first
    { "id": "m-101", "role": "user | assistant | tool | error",
      "text": "…",                              // a tool row is one line; capped at 30 000 characters
      "createdAt": "2026-11-17T12:00:00.000Z",
      "kind": "message | tool | error | generated_image",
      "origin": { "…": "…" },                   // when the message came from outside (kept as today)
      "mediaId": "med…", "status": "…",         // kind = generated_image, path and bytes stay on the host
      "images": [ { "path": "C:\\…" } ] } ],    // attachments, paths only
  "count": 12,                                  // messages in this reply
  "totalCount": 140,                            // messages in the conversation, whatever the page
  "truncated": true,                            // some message in the requested range is not here
  "firstId": "m-101",                           // pass as beforeMessageId to read the page before
  "lastId": "m-112",                            // pass as afterMessageId to read the page after
  "bytes": 41230,                               // size of `messages` as JSON
  "capBytes": 131072 }
```

- The window is the newest messages when neither cursor is given, at most `limit` of them, cut further to fit `capBytes`; the newest message is always kept.
- `truncated` is true when the requested range held more messages than the reply does, whichever of the two limits cut it.
- An `afterMessageId` or `beforeMessageId` that is not in the conversation is an error: "afterMessageId is not in this conversation." The two cursors are refused together: "Give afterMessageId or beforeMessageId, not both."
- Implemented in `crates/retinue-daemon/src/external.rs` (`read_conversation`, `conversation_page`); the catalogue entry's description changes to match.
- The Manager's own `read_conversation` (`crates/retinue-daemon/src/team.rs:2083`) already returned `{ messages, earlierMessagesOmitted }` with 30 messages of 4 000 characters each. The MCP tool now follows that style (an object with a marker) but with a byte cap and two cursors, so the two tools read alike even though their numbers differ. Nothing in the Manager's tool changed.

#### 3b. `session.get` on a phone — **reply gains fields, params gain one**

The desktop's `session.get` is unchanged: a whole conversation, as today. What a paired device receives is windowed, in one place (`crates/retinue-daemon/src/remote/policy.rs`, mirrored in `apps/desktop/src/remote/remoteTiers.ts`). Limits live in `apps/desktop/src/remote/tiers.json` (`limits`), so both languages read one number.

```jsonc
// params a device sends
{ "sessionId": "ses…", "beforeMessageId": "m-101" }   // optional; the desktop never sends it
// reply, in addition to the session object it already returns
{ "messages": [ { "…": "…", "content": "…", "contentTruncated": true } ],  // newest window, oldest first
  "messagesTruncated": true,          // older messages exist and are not here
  "messagesCursor": "m-101",          // pass as beforeMessageId for the previous window
  "turns":  [ … ],  "turnsTruncated": true,    // newest 40 turns
  "tools":  [ … ],  "toolsTruncated": true,    // newest 100 tool rows
  "files":  [ … ],  "filesTruncated": true }   // newest 100 file rows
```

- Limits: 200 messages, 8 000 characters of one message's `content`, 40 turns, 100 tools, 100 files.
- A paged read (`beforeMessageId`) returns messages before that id and empty `turns`, `tools` and `files` with their `…Truncated` markers true: older history is its messages.
- No event, no new RPC and no new tier row: `session.get` is already `read`/`viewer`, and `beforeMessageId` is a parameter it accepts like any other.
- The phone's screens are the Director's; this file says what arrives.

## Running log

- 2026-11-17: report started with the intended shapes.
- 2026-11-17, CI split: `scripts/ci-scope.mjs` and `scripts/ci-scope.test.mjs` written and **run** (`node --test scripts/ci-scope.test.mjs`, 13 tests pass); `.github/workflows/ci.yml` rewritten with `scope`, eight check jobs, a required `checks` job and a nightly cron. Both workflow files **parsed** with `yaml` 2 from a throwaway folder outside the repository (not added to `package.json`): every job's `needs` resolves, every job has `runs-on`, every `scripts/…` named in either workflow exists, and each rule in the table was probed against a real path. What only CI can prove: that GitHub accepts the file, that the selection matches what the runner computes, and that `checks` is the branch-protection requirement (it must be ticked in the repository settings; nothing in the tree can do that).
- 2026-11-17, update round trip: `scripts/update-round-trip.mjs` (reader mirroring `tauri-plugin-updater` 2.10.1's `RemoteRelease`, semver precedence, the "is it newer" answer) with `scripts/update-round-trip.test.mjs`, **run**: 6 tests pass, including the CLI refusing an equal version and a round trip through `make-update-manifest.mjs`'s own `buildManifest`. `scripts/release-windows.ps1` runs it on the manifest before the `gh` calls and refuses a version not newer than the published `channel-beta` one; the script also **parses** (`[Parser]::ParseFile`), but the release path itself was not run, by rule. `release.yml` gained `require-ci` (parsed).
- 2026-11-17, bounded reads: `read_conversation` returns the page object (3a) and `session.get` gains the phone window (3b). Tests **run**: the shared goldens in TypeScript (`npx vitest run src/remote/policyGoldens.test.ts`, 99 pass, including two new `session-get-*` cases) and in Rust (`cargo test -p retinue-daemon remote::policy`, 11 pass — the Rust port agrees with the TypeScript-generated `expect` values, which is the parity proof for the windowing); the MCP object reply is proven by `failed_turn_shows_its_error_in_wait_for_turn_read_conversation_and_turn_metrics`, **run** and passing; the whole `src/remote` suite passes (240 tests, including the tiers parity test and the phone build). **Not run:** `read_conversation_returns_a_bounded_page_with_cursors` (external.rs) and `session_get_pages_backwards_from_a_cursor_and_leaves_the_desktop_read_whole` (`wave3_tests.rs`). Both compile — `cargo test -p retinue-daemon --bin retinued --no-run` linked the test binary on 17 November, twice — but every freshly relinked daemon test binary is refused by Smart App Control (`os error 4551`) on five attempts, and copying it elsewhere does not help. CI runs them.
- 2026-11-17, clippy: **passed**, with the documented workaround. `cargo clippy -p retinue-daemon --all-targets -- -D warnings` cannot start while the daemon's own build script is refused by Smart App Control (`os error 4551`), so the lane moved `crates/retinue-daemon/build.rs` aside and stubbed `include!(concat!(env!("OUT_DIR"), "/skill_library_files.rs"))` in `skill_library.rs` with the three type aliases the script generates; the run then finished clean and **both files were restored** (`git checkout --` and the file moved back) — `git status` shows neither. The crate's own skill-library tests cannot run under that stub, which is why only clippy used it. `node scripts/check-format.mjs`, `node scripts/check-architecture.mjs`, `node --test "scripts/**/*.test.mjs"` (52) and `npm run typecheck` all pass on this tree.

## Not done, and what is left for the Director

- **Branch protection.** `checks` must be the required status check in the repository settings; nothing in the tree can set that. Until it is, a red run does not block a merge.
- **The nightly run has never run.** The cron is `10 4 * * *` UTC on the default branch. Its first run is the proof that the full selection works on a schedule.
- **The `require-ci` job has never run.** Its `gh api` query is written against `ci.yml`'s file name and `head_sha`; the first dispatched release proves it. A commit whose CI never ran is refused by design — re-run CI on it first.
- **The release script's new step has never run.** `release-windows.ps1` parses and the node part it calls is tested, but the `gh release download channel-beta` path and the refusal after it are only exercised by a real release. A dry run checks the manifest's shape only.
- **The phone's screens.** The window fields exist in the reply; the phone UI (adding "older messages" on a truncated read) is the Director's.
- **Idea 9** (a fingerprinted behaviour suite) is later, as the plan says. Not started.
- **The read itself is still whole.** `session.get` and `session_detail` read the entire conversation from SQLite and then trim; the storage-side windowing that idea 8 also suggests (a `session.messagesAround`-style query) is not done, so opening a very old conversation is still as expensive on the host as before. The reply it sends is bounded, which is what this lane promised.

## Files

New: `scripts/ci-scope.mjs`, `scripts/ci-scope.test.mjs`, `scripts/update-round-trip.mjs`, `scripts/update-round-trip.test.mjs`, `crates/retinue-daemon/src/wave3_tests.rs`, this report.
Changed: `.github/workflows/ci.yml` (rewritten around `scope`/`checks`), `.github/workflows/release.yml` (`require-ci`), `scripts/release-windows.ps1` (the round trip before publishing), `crates/retinue-daemon/src/external.rs` (`conversation_page`, the `read_conversation` reply, the catalogue text), `crates/retinue-daemon/src/external_tests.rs` (the object reply, the new paging test), `crates/retinue-daemon/src/main.rs` (`session_get`, the `wave3_tests` include), `crates/retinue-daemon/src/remote/policy.rs` (`SessionLimits`, `window_session`, `redact_result_with`, the goldens' `limits`), `apps/desktop/src/remote/tiers.json` (`limits`), `apps/desktop/src/remote/remoteTiers.ts` (`windowSessionForRemote`, `SessionLimits`), `apps/desktop/src/remote/policyGoldens.test.ts`, `apps/desktop/src/remote/fixtures/policy-goldens.json` (two cases), `docs/ipc-contract.md`, `docs/operations/release-checklist.md`, `docs/reference/invariants.md`, and the skill (`references/verification.md`, `references/lessons.md`, `references/codebase-map.md`).


