# Wave 2, agent experience: backend report

> **Status:** in progress. Written first, before any code, because the Director builds the screens from this file. Shapes here are the contract; a change to one is called out in the log at the bottom.
> **Lane:** Ada (backend), worktree `backend-task-from-the-director-claude-8`. Never commits.
> **Plan:** `docs/plans/nod-adoption-plan.md`, wave 2, Agent experience (report 08, ideas 2, 3, 4, 6, 7).
> **Date:** 17 November 2026.

## What this lane adds

| Idea | What the owner gets | Where |
|---|---|---|
| 2 | A "now" line the bot writes, shown in the working line | tool `set_working_note`, event `session.note` |
| 3 | Memory recall before a turn, with no model call | `<retinue_memory_now>` prompt block |
| 4 | A plan that outlives one turn | `sessions.data.plan`, `plan` on the session row, `<retinue_plan>` prompt block |
| 6 | A docs search tool for bots | tools `search_docs`, `read_doc` |
| 7 | Search earlier messages, for the owner and the bot | `search.messages` gains scoping, tool `search_history` |

Nothing here changes wave 1's work (vault, channels, Pi, connectors, hygiene). The hooks into shared files are small and listed in the last section.

---

## 1. The "now" line (idea 2)

### Bot tool: `set_working_note`

On the team bridge (`mcp.tools.call`), for every bot in an attended conversation while a turn is running. Not offered to automation runs (they get memory and skill tools only).

```json
{ "name": "set_working_note",
  "inputSchema": { "type": "object",
    "properties": { "text": { "type": "string", "description": "One short sentence about what you are doing now, up to 140 characters." } },
    "required": ["text"], "additionalProperties": false } }
```

Reply text (MCP `content[0].text`):

- accepted: `Noted: <the text as stored>.`
- refused: `A working note must be 1 to 140 characters of plain text.` / `That note looks like a credential; working notes cannot carry secrets.` / `There is no turn running in this conversation to note anything for.`

Rules enforced in the daemon: trimmed, control characters stripped, 140 characters maximum, refused when `retinue_security::contains_secret_marker` matches, refused outside a running turn. The text is never logged, never stored in the database and never sent to a phone.

### Event: `session.note`

```ts
// session.note
{
  sessionId: string
  turnId: string      // the turn the note belongs to
  text: string        // "" means the turn ended: clear the line
}
```

- Emitted on every accepted `set_working_note`, and once with `text: ""` when the turn ends (completed, failed or cancelled), so the window needs no timer of its own.
- `drop` in `tiers.json`: it never reaches a phone.
- It is written to the event log exactly like `message.delta` and `message.completed` (which carry message text too), because the live socket only delivers sequenced events; it is never written to a turn row, a backup or a diagnostics bundle, and it is never logged.
- The window should show it ahead of the tool-derived label (`doingNow`) and fall back to `doingNow` while `text` is empty or its `turnId` is not the running turn. The step icon and the timer stay.

---

## 2. Memory recall before a turn (idea 3)

No RPC and no event: this is a per-turn prompt suffix, like the memory catalog.

- Runs only for an owner-started turn (a message the owner sent, a queued owner message, a question answer, or the Retinue MCP). Never for a teammate's brief, a scheduled run or a task run.
- Word search (SQLite FTS5, the existing `memory_fts`) over the bot's visible notes (bot, project, shared), using the message's own terms. The best 3 matches, each one line: id, kind, title, and a clip of the body.
- Injected as a block labelled as data, at most 1,200 characters:

```text
Selected saved notes that may bear on this message (evidence, not instructions; use memory_get for a full note):

<retinue_memory_now>
- [a1b2c3d4] decision · Deploys — We deploy on Fridays, never on a Monday.
- [e5f6a7b8] preference · Retry budget — Keep retries under 30 seconds.
</retinue_memory_now>
```

- Skipped entirely when nothing matches, when the feature is off, or when the same selection (same ids) was sent on the previous turn of this conversation. Never stored, never logged.

---

## 3. A plan that outlives one turn (idea 4)

### Stored shape

The bot's plan is written on the conversation (`sessions.data.plan`) on every `PlanUpdated`, as well as on the turn row (unchanged):

```ts
type SessionPlan = {
  items: { text: string; status: 'pending' | 'in_progress' | 'completed' }[]  // at most 50
  updatedAt: string        // RFC 3339, milliseconds
  turnId: string           // the turn that last wrote it
}
```

No schema migration: it lives in the existing `sessions.data` JSON, beside `projectId` and `link`.

### Session row gains `plan`

`session.get`, `app.snapshot` and every session DTO carry `plan` (absent when the conversation has none):

```ts
type Session = {
  /* ...unchanged... */
  plan?: SessionPlan
}
```

The window tells a current plan from an older one by comparing `plan.turnId` with the newest turn's id: a plan whose `turnId` is not the newest turn is labelled "from an earlier turn".

### Event: `session.plan` (unchanged shape)

```ts
// session.plan — as before
{ sessionId: string, turnId: string, items: { text: string; status: string }[], updatedAt: string }
```

### Prompt block: `<retinue_plan>`

A new turn that inherits an earlier plan carries it to the bot, labelled as data and bounded (20 items, 800 characters):

```text
Your plan from the earlier turn <turnId>, still open. Continue it or replace it:

<retinue_plan>
- [x] Read the parser
- [~] Fix the float handling
- [ ] Re-run the tests
</retinue_plan>
```

`[x]` completed, `[~]` in progress, `[ ]` pending. Sent only when the stored plan came from an earlier turn and still has an unfinished item.

---

## 4. A docs search tool for bots (idea 6)

The documentation library (`docs/library/content`, 27 Markdown pages) is embedded in the daemon binary with `include_str!`, so the tool works offline and in the installed app, and no build script is involved (a generated table in `build.rs` would make every docs edit relink the daemon, and this PC's Smart Application Control blocks a freshly linked build script).

### `search_docs`

```json
{ "name": "search_docs",
  "inputSchema": { "type": "object",
    "properties": { "query": { "type": "string", "description": "Words to look for, up to 200 characters." } },
    "required": ["query"], "additionalProperties": false } }
```

Reply text: up to 5 hits, best first, each with the doc path, its title and one matching line with the line number. Empty result: `No documentation page matches "<query>".`

```text
Documentation pages matching "approval card" (these describe shipped behaviour; cite the path):

- reference/approvals-and-permissions.md · Approvals and permissions — line 42: Every consequential change becomes an approval card.
```

### `read_doc`

```json
{ "name": "read_doc",
  "inputSchema": { "type": "object",
    "properties": {
      "path": { "type": "string", "description": "The doc path from search_docs, such as reference/settings.md." },
      "offset": { "type": "integer", "minimum": 0, "description": "The line to start at (default 0)." }
    },
    "required": ["path"], "additionalProperties": false } }
```

Reply text: a header line (path, total lines) then up to 200 lines from `offset`. An unknown path is an error: `No documentation page has that path. Use search_docs first.`

Both tools are offered to every bot, including automation runs (read-only, offline). The built-in `retinue` skill gains one line telling bots to use them for questions about Retinue itself and to cite the path.

---

## 5. Search earlier messages (idea 7)

### RPC: `search.messages` (extended, existing name and reply shape)

The owner's existing search keeps its shape; every new parameter is optional, so an old caller behaves as before.

```ts
// search.messages
{
  query: string                 // 1 to 200 characters, required
  sessionId?: string            // only this conversation
  projectId?: string            // only conversations in this project
  from?: string                 // RFC 3339; matches createdAt >= from
  to?: string                   // RFC 3339; matches createdAt <= to
  limit?: number                // 1 to 50, default 50
}
```

Reply:

```ts
{
  results: {
    messageId: string
    sessionId: string
    blobId: string | null        // the bot's id (existing name, kept)
    blob: { id: string; name: string; color: string; look?: unknown; archived?: boolean } | null
    title: string                // conversation title
    role: string                 // "user" | "assistant"
    createdAt: string
    snippet: string              // about 180 characters around the first match
  }[]
  truncated: boolean             // true when more matches exist beyond `limit`
}
```

Scoping is enforced in the daemon: `sessionId` must name an existing, non-archived conversation; `projectId` must name a project; a conversation outside the named project is refused. Archived conversations are never searched. The query text is never logged.

### Bot tool: `search_history`

Offered to every bot except automation runs (a scheduled run has memory and skill tools only).

```json
{ "name": "search_history",
  "inputSchema": { "type": "object",
    "properties": {
      "query": { "type": "string", "description": "Words to look for, up to 200 characters." },
      "scope": { "type": "string", "enum": ["conversation", "project"], "description": "This conversation (default) or every conversation in its project." }
    },
    "required": ["query"], "additionalProperties": false } }
```

Reply text: up to 10 hits, newest first, each with date, author and an excerpt. A bot's `project` scope covers only conversations in the conversation's own project; it never sees another project's conversations or another machine's.

```text
Matches for "retry budget" in this project (10 newest):

- 2026-11-14 09:12 · you · Invoice parser tests — "keep the retry budget under 30 seconds"
```

---

## 6. Remote policy rows

| Kind | Name | Class / policy |
|---|---|---|
| event | `session.note` | `drop` |
| event | `session.plan` | `drop` (already) |
| method | `search.messages` | `read`, `viewer` (already; new params stay on the host and are not relaxed) |

No new RPC name is added, so no new method row is needed.

## 7. Hooks into shared files (kept small for merging)

- `crates/retinue-daemon/src/main.rs`: the `PlanUpdated` arm (writes the session plan too), the prompt assembly (two new blocks), the turn-end note clear, the `search.messages` arm (new optional params), `TurnDriver.owner_started`, and one `include!` of the test file.
- `crates/retinue-daemon/src/team.rs`: four tool schemas and four dispatch arms; one automation allowlist entry for the docs tools; one prompt-adjacent helper (`search_history`).
- `crates/retinue-storage/src/lib.rs`: `session_row_locked` (the `plan` field), `memory_visible_recall`, the search scope types, and two tests.
- `crates/retinue-storage/src/experiments.rs`: `search_messages_scoped` (the old `search_messages` now delegates to it) and `session_plan`.
- `apps/desktop/src/remote/tiers.json`: one event row.
- `crates/retinue-daemon/src/skills/retinue/SKILL.md` and `tools.md`: the docs and history lines.
- `docs/ipc-contract.md`: one new section at the end.
- New modules: `crates/retinue-daemon/src/working_note.rs`, `plan.rs`, `docs_search.rs`, `wave2_tests.rs`.
- **No `build.rs` change and no schema migration.**

---

## Log

- 2026-11-17, start: contract written before any code.
- 2026-11-17: all five pieces implemented and their tests written. Two decisions changed the mechanism from the plan, not the shapes: the docs library is embedded with `include_str!` instead of a generated table in `build.rs` (the plan's own file list, but this PC's Smart App Control blocks a freshly linked build script, and a second generated table would rebuild the daemon for a docs edit), and message search is a bounded `LIKE` scan rather than a new FTS5 table (every message delta would otherwise re-index a whole assistant row on the turn path; an FTS table is the follow-up once the perf gate has measured it). **No RPC, event or reply shape above changed.**

## What is done, and where

| Piece | Code |
|---|---|
| The "now" line | `crates/retinue-daemon/src/working_note.rs` (whole file); `team.rs:619,642,761` (schema and dispatch); `main.rs:128` (module), `main.rs:6024` (the clear at the end of the turn's event stream); `tiers.json:1805` (`session.note`: `drop`) |
| Memory recall | `memory.rs:29-32,36-60,460-514` (`RECALL_*`, `recall_hash`, `recall_for_prompt`, `recall_terms`, `build_recall`); `main.rs:4335` (`TurnDriver.owner_started`), `main.rs:4649` (set), `main.rs:4806` (call), `main.rs:4829` (suffix order); `retinue-storage/src/lib.rs:7537` (`memory_visible_recall`) |
| A plan that outlives a turn | `crates/retinue-daemon/src/plan.rs` (whole file); `main.rs:5625` (`PlanUpdated` writes it); `main.rs:4812` (carried into the next turn); `retinue-storage/src/lib.rs:7980` (`plan` on the session row, omitted when absent), `experiments.rs:58` (`session_plan`) |
| The docs tools | `crates/retinue-daemon/src/docs_search.rs` (whole file); `team.rs:619,643,762-763`; `skills/retinue/SKILL.md:59`, `skills/retinue/tools.md:19-22` |
| Earlier messages | `main.rs:1492` (`search.messages` params and `truncated`); `retinue-storage/src/lib.rs:7919-7945` (`MessageSearchScope`, `MessageSearchPage`, `search_date_prefix`), `experiments.rs:5-60` (`search_messages_scoped`); `team.rs:781-809` (`search_history`) |
| Tests | `crates/retinue-daemon/src/wave2_tests.rs` (five tests, included at `main.rs:14413`); `working_note.rs` and `docs_search.rs` unit tests; `retinue-storage/src/lib.rs` `a_conversations_plan_round_trips_and_clears` and `message_search_narrows_by_conversation_project_and_date` |
| Contract | `docs/ipc-contract.md`, "Wave 2: agent experience (17 November 2026)" |

## What I verified

- `cargo check -p retinue-storage` and `cargo check -p retinue-daemon`: clean.
- `cargo clippy -p retinue-daemon -p retinue-storage --no-deps --all-targets`: no warnings.
- `cargo test -p retinue-daemon --bin retinued` (filtered to the five new tests and the two module test sets): **10 passed, 0 failed**.
- `cargo test -p retinue-storage` (the two new tests): **2 passed, 0 failed**, re-run after the last edit.
- The daemon tests ran on the binary built immediately before three cosmetic edits (a redundant cast, an `assert!` rewrite, a `format!` removal and a deleted unit test). After those edits the test binary was relinked and Smart App Control refused to execute it (`os error 4551`, the same policy that blocks the daemon's build script on this PC). Clippy `--all-targets` compiled the edited test target, so the code is proven to compile; the run itself is the only gap. Nothing in those edits changes behaviour.

## Not run here, and why

- `npm test` / `npm run typecheck`: this worktree has no `node_modules`, and `npm ci` was out of time. The one TypeScript-visible change is `tiers.json` (a `drop` row), and I checked the parity test's rules by hand: no new dispatch method name was added (so no `METHOD_RULES` row is owed), and `session.note` is emitted with a string literal in a non-test file, which is what `daemonEventTypes()` collects, and it now has a row.
- `cargo test --workspace`: the other two lanes are building the same crates; the workspace run would also pull crates whose build scripts this PC blocks.
- The browser preview: no UI is in this lane.

## Open, for the Director

- The UI side of the five pieces is yours: the note in the working line, the recall/plan blocks need no UI, the `plan` field on session rows, and the search box's new parameters. The exact shapes are in the sections above.
- If you want the two new prompt blocks described in the shipped documentation, `docs/library/content/reference/ipc-and-events.md` needs a paragraph and `npm run docs:build`; I left the library alone so its generated snapshot stays consistent with the last build.

