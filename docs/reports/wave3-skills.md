# Wave 3, learned skills and first-run suggestions: backend report

> **Status:** the contract and the implementation are both in this file. Shapes here are the contract the Director builds from; the log at the bottom records the two that changed while I worked. What ran, and what this PC refused to run, is under "What I verified" and "Not run here".
> **Lane:** Ada (backend), worktree `backend-task-from-the-director-claude-10`. Never commits.
> **Plan:** `docs/plans/nod-adoption-plan.md`, wave 3, Learned skills (report 08, ideas 1 and 5).
> **Date:** 17 November 2026.

## What this lane adds

| Idea | What the owner gets | Where |
|---|---|---|
| 1 | Skills a bot writes for itself, held for review | tools `save_skill`, `patch_skill`; RPCs `skills.learned.*`; event `skills.learned.changed` |
| 5 | Three grounded first-run suggestions read from the project folder | RPC `project.suggestions` |

Nothing here changes wave 1 or wave 2's work. The hooks into shared files are listed in section 4.

---

## 1. Learned skills (idea 1)

### The row

A learned skill is a normal skill row (`skills` table) plus four review fields. It is a real folder under `<data>/skills/<name>` like any import, so `load_skill`, `read_skill_file`, backup export and the per-bot opt-out work unchanged.

```ts
// one row, as skills.list and skills.learned.list return it
{
  name: string                    // [a-z0-9][a-z0-9-]{0,63}
  description: string             // one line, <= 200 characters for a learned skill
  source: 'agent'                 // new source value; imports keep claude/codex/opencode/folder/backup
  enabled: boolean                // the pool-wide switch
  disabledFor: string[]           // bots with the per-bot opt-out
  contentSha256: string
  fileCount: number
  totalBytes: number
  importedAt: string              // RFC 3339 ms; for a learned skill this is when it was first saved
  updatedAt: string
  reviewState: 'approved' | 'pending'   // pending: private to the bot that learned it
  learnedBy: string | null        // agent id (always set when source is 'agent')
  learnedByName: string | null    // that bot's current name, resolved at read time
  ownerEdited: boolean            // the bot may no longer patch it
  usedAt: string | null           // last time a bot loaded it; drives least-recently-used eviction
}
```

Rows that are not learned report `reviewState: "approved"`, `learnedBy: null`, `ownerEdited: false`, `usedAt: null`. The window only needs to look at `source === "agent"`.

### Bot tools (team bridge, `mcp.tools.call`)

Offered to every bot **except automation runs** (a scheduled or task run gets the read-only skill tools only, and the dispatch refuses the write tools there as well).

#### `save_skill`

```json
{ "name": "save_skill",
  "inputSchema": { "type": "object",
    "properties": {
      "name": { "type": "string", "description": "A short name for the procedure: lower case and dashes, 2 to 64 characters, like release-gates." },
      "description": { "type": "string", "description": "One sentence saying when to use it, up to 200 characters." },
      "body": { "type": "string", "description": "The procedure in plain Markdown: the steps, what to check, and what not to do. Up to 4000 characters." }
    },
    "required": ["name", "description", "body"], "additionalProperties": false } }
```

Reply text (`content[0].text`), accepted:

```text
Saved the skill "release-gates" for this bot. It stays private to this bot until the owner approves it in Settings, Skills.
```

Replies when a `save_skill` replaces an earlier pending or approved draft of the same name by the same bot:

```text
Updated the skill "release-gates" for this bot. It stays private to this bot until the owner approves it.
```

Refusals (plain sentences, one line each):

| Reason | Text |
|---|---|
| not a running turn the owner started | `Only a turn the owner started can save a skill. Ask the owner in a normal conversation.` |
| automation or task run | `Automation runs cannot save skills.` |
| reserved name | `"retinue" is a built-in skill name and cannot be reused.` / `"api-design" is a skill name Retinue ships and cannot be reused.` |
| another skill already has the name | `A skill called "x" already exists. Pass a different name, or patch your own skill.` |
| secret, card number or sign-in link | `That draft looks like it contains a credential, a card number or a sign-in link. Remove it and save again.` |
| bad name | `name must be 2 to 64 characters: lower case letters, digits and dashes.` |
| empty or long body | `A skill body must be 1 to 4000 characters.` / `A skill description must be 1 to 200 characters.` |
| cap reached, nothing evictable | `This bot has 40 saved skills and the owner edited them all. Forget one in Settings, Skills, before saving another.` |

What the daemon does to the draft before storing it (the memory write rails, plus the placeholder pass):

- strips control characters except line breaks, normalises line endings, trims each line;
- refuses the whole draft when `retinue_security::contains_secret_marker` matches, when a 13 to 19 digit run passes the card-number check (Luhn), or when it contains a sign-in link (`login`, `signin`, `sign-in`, `auth`, `token`, `password`, `otp` in a URL path or query);
- replaces e-mail addresses with `<email>`, absolute URLs with `<link>`, and runs of 6 or more digits with `<number>`, so a saved procedure carries placeholders rather than the owner's data;
- refuses text that reads like an instruction to the reader in the memory module's sense (`retinue_memory::injection_phrase`), because a learned skill is guidance for a bot, not a message from someone else.

Stored shape: `<data>/skills/<name>/SKILL.md` written with frontmatter (`name`, `description`) and the body, then the row upserted with `source: "agent"`, `reviewState: "pending"`, `learnedBy: <agent id>`.

#### `patch_skill`

```json
{ "name": "patch_skill",
  "inputSchema": { "type": "object",
    "properties": {
      "name": { "type": "string", "description": "The learned skill's name." },
      "description": { "type": "string", "description": "A replacement one-line description (optional)." },
      "body": { "type": "string", "description": "The replacement procedure, up to 4000 characters." }
    },
    "required": ["name", "body"], "additionalProperties": false } }
```

Reply: `Patched the skill "release-gates". It stays private to this bot until the owner approves it.` (or `…the owner approves the change.` when the skill was already approved).

Refusals: `No skill called "x" was saved by this bot.` / `The owner edited "x", so only they can change it now. Ask them first.` / `That is a shared skill this bot did not save. Patch only skills you saved yourself.` / the same credential and length refusals as `save_skill`.

A patch never changes `learnedBy`, never clears an owner approval beyond marking the row `pending` again if the owner had already approved it (so the owner sees the change before another bot uses the new text).

### RPCs for the review queue

All four are desktop-only: class `never` in `tiers.json`.

```ts
// skills.learned.list { agentId?: string, reviewState?: 'pending' | 'approved' | 'all' }  (default 'all')
{ skills: LearnedSkill[] }

type LearnedSkill = {
  name: string
  description: string
  reviewState: 'pending' | 'approved'
  learnedBy: string
  learnedByName: string | null
  ownerEdited: boolean
  importedAt: string       // when it was first saved; the same field skills.list carries
  updatedAt: string        // when its text last changed
  usedAt: string | null
  enabled: boolean
}
```

- `agentId` narrows to one bot's learned skills; `reviewState: 'pending'` is the review queue itself.
- Rows are ordered newest change first.
- The body is **not** in this reply. The review screen reads `skills.get { name }` (existing RPC, returns `body` and `files`).

```ts
// skills.learned.approve { name: string }
{ skill: LearnedSkill }        // reviewState 'approved'
```

```ts
// skills.learned.edit { name: string, description?: string, body?: string }
{ skill: LearnedSkill }        // ownerEdited true, reviewState 'approved'
```

- The edit applies the same length caps and the same credential refusal; it does **not** run the placeholder pass (the owner is allowed to write what they mean), but a secret is still refused.
- Once edited, the bot's `patch_skill` and `save_skill` for that name are refused from then on.

```ts
// skills.learned.forget { name: string, confirm: true }
{ forgotten: true }
```

- Deletes the row and the folder. `confirm` must be `true`; a learned skill is not recoverable.

Errors, as elsewhere in the daemon: `invalid_argument`, `not_found` (`No learned skill is called "x".`), `conflict` (`"x" is not a learned skill.`).

### Event

```ts
// skills.learned.changed
{
  action: 'saved' | 'patched' | 'approved' | 'edited' | 'forgotten' | 'evicted'
  name: string
  agentId: string | null     // the bot who learned it; null after a forget when the row is gone
}
```

- `drop` in `tiers.json`: a learned skill's text and its bot's id never reach a phone. The phone's `skills.list` shows the row, but the approval is a desktop decision.
- Emitted once per change, from both the bot tools and the owner RPCs, so the Skills page and the pending badge update without polling.

### Rails, summarised (who may do what)

| | owner-started turn | teammate turn, task run | automation run | owner (RPC) |
|---|---|---|---|---|
| `save_skill` | yes, own name space | refused | refused | - |
| `patch_skill` | yes, own rows only | refused | refused | - |
| reads (`list_skills`, `load_skill`, `read_skill_file`) | yes | yes | yes | - |
| see a pending skill | the bot that learned it only | same | same | yes |
| see an approved skill | every bot, subject to the pool switch and the per-bot opt-out | same | same | yes |
| approve, edit, forget | - | - | - | yes |

Cap: **40 learned skills per bot**. On a save that would exceed it, the least recently used row that the owner has not edited is evicted (`usedAt`, else `updatedAt`), its folder removed, and `skills.learned.changed` emitted with `action: "evicted"` before the save's own event. When every row is owner-edited, the save is refused (text above).

`usedAt` is touched when a bot loads a learned skill with `load_skill` (never on the index build, which would write on every turn).

A purged bot's learned skills are forgotten with it (rows deleted in `agent_purge`, folders removed by the daemon).

---

## 2. First-run suggestions (idea 5)

One RPC, read-only, no event, no model call. The window offers it after the CEO is created and after a new project is added.

```ts
// project.suggestions { projectId: string }
{
  projectId: string
  folder: string | null       // the project's folder on this PC, when it has one
  degraded: boolean           // true when fewer than three suggestions could be grounded in the folder
  suggestions: Suggestion[]   // always exactly 3
}

type Suggestion = {
  id: string                  // stable per kind: 'readme', 'script', 'commit', 'agents', 'static-1'
  kind: 'readme' | 'script' | 'commit' | 'agents' | 'static'
  title: string               // short card label, <= 80 characters
  prompt: string              // exactly what the composer sends, <= 500 characters
  source: { file: string | null, detail: string } | null
}
```

`source.file` is a project-relative name from a fixed list; `source.detail` says what was read, for example `heading: Invoice parser`, `script: test`, `commit: 4f2a1c Set up the parser`, `heading: Rules for agents`. `source` is `null` only for a `static` fallback.

### What is read

Only these names, only inside the project's own folder, each capped at 64 KiB and refused when the canonical path escapes the folder or is a link:

| File | What is taken | Suggestion kind |
|---|---|---|
| `README.md`, `README`, `readme.md`, `Readme.md` (first found) | the first Markdown heading | `readme` |
| `package.json` | the first `scripts` entry, preferring `test` | `script` |
| `Cargo.toml` | the `[package] name` and any `[[bin]]` name | `script` |
| `AGENTS.md` | the first heading after the title, or the title | `agents` |
| git history (through `git log` in the folder, arguments only, 2 s limit) | the five newest subjects | `commit` |

### How the three are chosen

Ground each kind once, in this order, and take the first three that could be grounded: `readme`, `script`, `commit`, `agents`. Any gap is filled with a `static` suggestion.

Texts (the prompt is what the card sends; the title is the card face):

| Kind | title | prompt |
|---|---|---|
| readme | `Get oriented` | `Walk me through this project: what it does, how it is laid out, and where I should start. Its README begins "<heading>".` |
| script | `Run the checks` | `Run the project's checks (npm run test) and tell me what fails, then propose the smallest fix.` (for a Rust project: `(cargo test for <crate>)`) |
| commit | `Pick up the last change` | `These are the newest commits here: "<newest>"; "<next>"; … Read what the newest one changed and tell me what is still open or worth doing next.` |
| agents | `Follow the project's rules` | `Read this project's AGENTS.md ("<heading>") and tell me the rules I must not break while working here.` |
| static | `First look` | `Read this project and tell me what it does.` |
| static | `Find the first fix` | `Find the most useful small fix in this project, explain it, and make it if I say yes.` |
| static | `Explain the build` | `Explain how this project is built, tested and released.` |

Fallbacks are used in order, so a project with no folder at all returns the first three in the table's last rows.

### Rules

- Nothing runs: the reply is text only, and the window decides what to do when the owner sends a card.
- The folder is the project's own stored path, canonicalised. An unreadable, missing or empty folder returns the static three with `degraded: true` and `folder` as stored (or `null`).
- A file that is a link, is larger than 64 KiB, or resolves outside the folder is skipped silently, never read.
- Content-free logs: nothing is logged, and there is no event. The reply does name the host folder, which is why the RPC is `never` remote.

---

## 3. Remote policy rows

| Kind | Name | Class / policy | Why |
|---|---|---|---|
| method | `skills.learned.list` | `never` | a review queue is a desktop decision |
| method | `skills.learned.approve` | `never` | approving makes an instruction every bot reads |
| method | `skills.learned.edit` | `never` | writing skill text from a paired device |
| method | `skills.learned.forget` | `never` | destructive, and irreversible |
| method | `project.suggestions` | `never` | the reply names the host's project folder |
| event | `skills.learned.changed` | `drop` | carries a bot id and a skill name; the body never leaves the daemon |

`skills.get` and `skills.list` keep their existing `read` / `viewer` rows, so the review screen can read a body over the same path it already uses.

---

## 4. Hooks into shared files (kept small for merging)

- `crates/retinue-storage/src/lib.rs`: `ensure_skills_schema` (the `source` CHECK gains `'agent'`, four new columns, a table rebuild for older databases), `SKILL_COLUMNS` and `skill_dto_from_row`, seven `skills_learned_*` methods, one line in `agent_purge`.
- `crates/retinue-daemon/src/skills.rs`: the draft rails, the two bot tools, the four RPC arms, the turn-origin record, the visibility rule for pending rows.
- `crates/retinue-daemon/src/team.rs`: two tool schemas in the attended list, two dispatch arms, and the skill schemas split so an automation run gets only the read-only three.
- `crates/retinue-daemon/src/main.rs`: the `skills.learned.*` dispatch arm, one line where a turn is recorded as owner-started, one line in `agent.purge`, `mod suggestions;`, and the `project.suggestions` arm.
- `crates/retinue-daemon/src/suggestions.rs` (new): the read path for the folder.
- `apps/desktop/src/remote/tiers.json`: four method rows and one event row.
- `docs/ipc-contract.md`: one new section at the end.
- **No `build.rs` change and no new daemon `build.rs` table.**

---

## Log

- 2026-11-17, start: contract written before any code.
- 2026-11-17: both pieces are implemented and their tests are written. **Two shapes changed from the draft above, and one behaviour in a shared file:**
  1. `skills.learned.list` does not carry a `learnedAt` field: the row carries `importedAt` (when it was first saved) and `updatedAt` (when its text last changed), the same names `skills.list` already uses for every skill. Read `importedAt` where the draft said `learnedAt`.
  2. The stored row also carries `sourcePath`, `contentSha256`, `fileCount`, `totalBytes` and `disabledFor`, because a learned skill is an ordinary pool row; the `LearnedSkill` type above lists what the queue needs.
  3. `owner_started` (wave 2's `TurnDriver` flag, which gates memory recall) now counts a chat-channel message as the owner, because the new `owner_started_origin` helper is the one expression both features use. A Telegram message therefore gets memory recall, as an owner message should.
- 2026-11-17: the storage half is executed and green (180 tests). The daemon's test binary is written and compiles but cannot run on this PC (see below).

## What is done, and where

| Piece | Code |
|---|---|
| The stored row and the schema rebuild | `crates/retinue-storage/src/lib.rs:847` (`ensure_skills_schema`: the `agent` source, `review_state`, `learned_by`, `owner_edited`, `used_at`, a table rebuild for an older pool), `:885` (`SKILL_COLUMNS`), `:1711` (`skill_dto_from_row`), `:1750` (`fill_learned_names`), `:7990` (`skills_upsert` keeps a learned row's source and review state on a restore) |
| The storage methods | `lib.rs:8070` `skills_learned_upsert`, `:8114` `skills_learned_list`, `:8132` `skills_learned_approve`, `:8143` `skills_learned_owner_edit`, `:8153` `skills_learned_update` (the one guarded update), `:8174` `skills_learned_forget`, `:8191` `skills_learned_touch`, `:8201` `skills_learned_evictions`; `agent_purge` deletes the bot's learned rows |
| The draft rails | `crates/retinue-daemon/src/skills.rs:1059` `sanitize_draft` (bounds, tidy, secrets, card runs, sign-in links, injection phrases, placeholders), `card_like`, `sign_in_link`, `mask_home`, `mask_placeholders`, `tidy_body` above it; `:1080` `place_learned_file` (staged and moved, so a failed write leaves no half a skill) |
| The owner-started rule | `skills.rs:897` `note_turn_origin`, `:910` `owner_started_now`, `turn_is_owner_started`; `main.rs:4434` `owner_started_origin`, `:4756` the one write per turn |
| The two bot tools | `skills.rs:1121` `tool_save`, `:1179` `tool_patch`, `:1402` `tool_schemas(writable)`; `team.rs:628` (attended list), `:636` (read-only list for automation runs), `:794`/`:795` the dispatch arms |
| Visibility of a pending skill | `skills.rs:704` `learner_may_use`, used by `visible_rows`, `tool_load` and `tool_read_file`; `used_at` is written by `touch_learned` on a load |
| The review RPCs | `skills.rs:1326` `skills.learned.list`, `:1336` `.approve`, `:1342` `.edit`, `:1378` `.forget`; `main.rs:1206` the dispatch arm |
| The event | `skills.learned.changed`, emitted from `tool_save`, `tool_patch`, the four RPCs and the eviction; `apps/desktop/src/remote/tiers.json` (`drop`) |
| A purged bot | `main.rs:1441` `skills::forget_folders` after `agent.purge` (the rows go in storage) |
| First-run suggestions | `crates/retinue-daemon/src/suggestions.rs` (whole file: `rpc` at `:46`, `read_inside` at `:90`, `readme_suggestion`, `script_suggestion` at `:149`, `commit_suggestion` at `:203`, `agents_suggestion`, the static fallbacks); `main.rs:87` (`mod suggestions`) and `:1252` (`project.suggestions`) |
| Remote policy | `tiers.json`: four `skills.learned.*` rows and `project.suggestions` as `never`, `skills.learned.changed` as `drop` |
| Tests | `crates/retinue-daemon/src/wave3_tests.rs` (eight tests, included at `main.rs:14608`; every RPC goes through `dispatch`, every tool through the team bridge), five unit tests in `skills.rs` (`a_learned_draft_is_bounded_and_tidied`, `the_placeholder_pass_replaces_people_addresses_and_numbers`, `cards_and_sign_in_links_are_refused`, `a_learned_name_is_two_to_sixty_four_lower_case_characters`, `only_the_running_owner_started_turn_may_write`), and the storage test `learned_skills_round_trip_and_an_older_pool_is_rebuilt` |
| Documents | `docs/ipc-contract.md` ("Wave 3"), `docs/remote/THREAT_MODEL.md` section 22, this file; skill updated (`references/codebase-map.md` wave 3 entry, two `references/lessons.md` entries) |

## What I verified

- `cargo check -p retinue-storage`, `cargo check -p retinue-daemon` and `cargo check -p retinue-daemon --tests`: clean. The last one was re-run after the final edit and after `touch`ing the two crate roots, because a shared target had held another lane's `retinue-storage` (`no method named skills_learned_*`); see `lessons.md`, "A shared `CARGO_TARGET_DIR` between worktrees…".
- `cargo clippy -p retinue-daemon -p retinue-storage --no-deps --all-targets -- -D warnings`: **exit 0**, no warnings. That run covered everything except a six-line pre-check added to `skills.learned.approve` afterwards, which `cargo check --tests` then compiled.
- `cargo test -p retinue-storage`: **180 passed, 0 failed**, including `learned_skills_round_trip_and_an_older_pool_is_rebuilt` (the learned row round trip, the owner's edit lock, the cap and eviction rules, and the rebuild of an older pool table with its rows intact).
- `node scripts/check-format.mjs`: passed (1,480 files). `node scripts/check-architecture.mjs`: passed.
- Line endings: every touched file is still CRLF throughout (`git diff --numstat` shows 3 to 540 changed lines, never a whole file).

## Not run here, and why

- **The eight new daemon tests, and the daemon suite as a whole.** The test binary links (`cargo test -p retinue-daemon --bin retinued --no-run` finishes, including after the last edit) but this PC's Smart App Control refuses to execute it: `could not execute process ...\retinued-efbea7b8d3b15610.exe (never executed)` / `An Application Control policy has blocked this file. (os error 4551)`. I retried after four relinks (deleting the `.exe` first), tried running copies from `C:\tmp`, and tried the older `retinued-*.exe` another lane had left in the same target; every attempt was refused, so the block is on the file, not the path. Clippy `--all-targets` compiled the same test target with `-D warnings` and `cargo check --tests` compiled the final state, so the tests are proven to compile, not proven to pass. **The Director or CI must run `cargo test -p retinue-daemon --bin retinued learned`, `... skill`, `... suggestions` (or the whole suite) before merging.**
- `npm test` / `npm run typecheck` / the remote parity tests: this worktree has no `node_modules`. I checked the parity rules by hand: every new method name is in a dispatch arm (`main.rs:1206`, `:1252`), and `skills.learned.changed` is emitted with a string literal, so both are visible to `remoteTiers.parity.test.ts`, and both have rows.
- The browser preview and any screen: no UI in this lane.
- `cargo test --workspace`: the other lanes are building these crates in the same target.

## Open, for the Director

- The eight UI pieces are yours: the pending badge, the review card with Approve / Edit / Forget, the "Learned" label on a skill row, the bot name, and the three suggestion cards. Shapes are in sections 1 and 2; `skills.get` reads a body, `skills.learned.list` is the queue.
- A learned skill's folder is a normal pool folder, so the existing backup export carries it. A restore keeps the row's review state and its bot (`skills_upsert` no longer overwrites `source` for a learned row), but a fresh database sees it as an imported skill with no bot; that is honest, and it is worth a line in the Backup page only if you want one.
- `docs/reference/invariants.md` has no wave 1 or 2 rows either; I left it alone so the page stays the Director's to land.
- The learned-skill cap is per bot, not per project, because a learned skill is attached to the bot that learned it until it is approved.
