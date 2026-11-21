# Importing conversations from other coding agents, fitted to employees

> **What this is:** research for the Director. It reads how Synara, T3 Code and Orca bring outside conversations in, states what Retinue does today with `path:line` evidence, proposes an import that fits the employee model, and splits the work into backend and frontend with a smallest useful first version. No product code was changed.
> **Status:** research. Every Retinue claim is read from this worktree at `cb8d736` (21 November 2026); every third-party claim is from the source or documentation named in *Sources*.
> **Last updated:** 2026-11-21

---

## Summary

**What Synara does.** Since 0.9.0 (21 September 2026) it scans local Codex and Claude Code data, groups conversations by **project folder**, offers a collapsible project card with a search box and an "include archived" toggle, and on import makes a **frozen native copy** of each provider session so the chat can continue on its own copy while the original history is untouched (`trysynara.com/docs/features/project-import`; `projectImport{Catalog,Route}.ts`). It never copies tool output or attachments as live messages, shows per-account errors instead of an empty list, dedupes by a project-origin key and by recorded native bindings, and marks imported rows "Already present" (`ProjectImportProjectCard.tsx:150-176`). Its search only matches project titles and folders, not conversation titles, and it has no date filter.

**What T3 Code and Orca do.** T3 Code bulk-imports projects and conversations from Claude Code, Codex and Pi in the **first-run wizard only** (`WelcomePage.tsx:1853-1856`), keeps 200 messages and no tool activity, records each transcript's file identity so a repeated import is skipped, and binds the imported thread to the **original** provider session so the next message resumes it (`AgentSessionScanner.ts:84-91`, `AgentSessionImporter.ts:304-322`). Its own version migration does the opposite thing honestly: it does not recreate the old live session and starts a fresh provider conversation from a bounded handoff (`docs/user/thread-migration.md:22-33`, `portable-handoffs.md`). Orca does not import at all: it lists the CLIs' own on-disk sessions in a side panel and **resumes the CLI in place** in a new terminal, or opens it in its structured chat, leaving the original as it was (`orca.dev/docs/agents/session-history`).

**Retinue already has an import (v0.2.0-beta.4).** `import.scan` → `import.preview` → `import.apply` → `import.distribute` → `import.assign`, with a ledger keyed by source + conversation id + SHA-256, a read-only guard, normalised text and tool labels, and no credentials, thinking or tool output (`crates/retinue-daemon/src/import.rs`; `docs/ipc-contract.md:900-914`). It is closer to finished than the brief assumes; what is missing is the *employee* half: project grouping, a suggestion before anything is written, hiring the right provider, a visible read-only state, and a "continue" that is one click.

**The proposal (section 4):** an import is not a conversation until an employee owns it. Every source conversation ends in one of three states: **Continue** (same provider, file still there, employee on that provider: it becomes live and resumable), **Carry over** (any other employee: the import stays read-only and a new conversation starts from a bounded, editable draft that names the imported conversation so the employee can read the rest), or **Keep as history** (read-only, readable as context). The picker groups by project, suggests an employee per conversation before writing anything, offers to hire one when no employee speaks that provider, and can import a whole project in one action.

**The first version (section 5):** fix the paging, the OpenCode rows and the source health; add `import.projects` and a project mode on `import.apply`/`import.assign`; make `import.distribute` accept preview rows and suggest a hire; surface `imported` on the session summary; then the window's project-first dialog and the read-only banner with "Continue with …" (reusing `session.handoff.draft`). Backend S–M, frontend M, and no change to what leaves the machine.

---

## 1. Synara: what it does, with evidence

Sources: the documentation page (fetched 21 November 2026) and a read-only clone of `github.com/Emanuele-web04/synara` at `c8c1ed4` (18 November 2026). I did not build or run it.

### 1.1 Where it looks

| Provider | Where | Evidence |
|---|---|---|
| Codex | The desktop app's own state: `state_<n>.sqlite` (tables `projects`, `project_roots`, `threads`, `thread_spawn_edges`) plus `.codex-global-state.json` for legacy project ids and assignments | `apps/server/src/provider/codexProjectImport.ts:76-102,138-158,217-280` |
| Claude Code | `~/.claude/projects/<project>/<uuid>.jsonl` (honouring `CLAUDE_CONFIG_DIR`), with dates and titles from the Agent SDK's `listSessions()` and a bounded transcript read as fallback | `apps/server/src/provider/claudeProjectImport.ts:45-65,118-130,163-214` |

It reads **local data, not the CLI**: a provider being installed is not required to list. Sub-agent threads are skipped on both sides (`codexProjectImport.ts:187-198`; `claudeProjectImport.ts:124` skips `isSidechain`). Discovery is bounded: 32 MiB of metadata and 8 MiB per line for Claude (`claudeProjectImport.ts:11-13`), unsupported Codex schemas raise a typed `unsupported` error rather than guessing (`codexProjectImport.ts:126-136,313-319`).

### 1.2 How it groups

Grouping is by **project (workspace root)**, merged across providers:

- each source's declared project roots and every existing Synara project are canonicalised (real path), so a conversation found by both Codex and Claude lands in one card (`projectImportCatalog.ts:98-130`);
- a conversation whose cwd is inside a known root joins that root; otherwise the git root decides; a worktree maps back to the original folder (`projectImportCatalog.ts:134-146,228-247`);
- Claude has no native project ids, so its `~/.claude/projects/<slug>` folders are treated as cwd hints only (`projectImportCatalog.ts:186-203`);
- cards are sorted by title; conversations within a card newest first (`projectImportCatalog.ts:260-269`);
- the key is `provider + canonical source home + session id`, so two accounts sharing one store list a conversation once and the default account is attributed (`projectImportRoute.ts:220-221,231-233,283-289`).

### 1.3 The picker

From `apps/web/src/projectImport/`:

| Element | What it shows |
|---|---|
| Entry points | Landing page and first-run announcement (`CHANGELOG.md` 0.9.0: "Landing-page and first-run entry points make imports discoverable"), plus onboarding (`OnboardingDialog.tsx` opens the dialog) |
| Source picker | A checkbox per provider (Codex, Claude Code) and "Find projects" / "Scan again" (`ProjectImportPanel.tsx:188-230`) |
| Search | One box filtering **project title and workspace path** only (`ProjectImportPanel.tsx:180-184,255-261`); no conversation-title search |
| Archived | "Include archived" checkbox (`:262-269`); discovered items start selected (`:106-112`) |
| Project card | Title, full folder path, "N conversations", "Adds to existing" or "New project", provider icons, and "Folder unavailable" with a relink box and folder picker (`ProjectImportProjectCard.tsx:61-138`) |
| Conversation row | Provider icon, title, an account badge for non-default accounts, and either the **date** or "Already present"/"Archived" (`:148-176`) |
| Ordering | Projects by title; conversations by `updatedAt` descending. No date filter and no sort control |
| Progress | "Importing X of Y · title", "Stop after current", a failure list with "Retry failed N" (`ProjectImportPanel.tsx:323-404`) |
| Copy | "Projects keep their existing folders, and conversations are copied into Synara. Nothing in your current projects changes." (`packages/shared/src/locales/en/workspace.json:754`) |

Their own hint admits the limit: *"History shows text messages; tool details and attachments may be missing."* (`workspace.json:741`).

### 1.4 What is copied

- **Messages:** text, with dates; the first page is applied to the thread in 20-message chunks and older pages are read lazily (`projectImportRoute.ts:121-135,615-686`).
- **Tool calls and attachments:** not reconstructed as live messages; the docs say so plainly ("This does not promise that every historical attachment or provider-internal artifact can be reconstructed", `project-import.md:21`; 1.0.0 notes "historical tools, reasoning and attachments are not reconstructed as live messages").
- **Files:** nothing is written into the repository. "Preserves the imported task context while assigning the destination; this is not a Git clone or a request to overwrite repository files" (`project-import.md:15`).
- **The provider's session id:** it copies **provider-session state for continuation instead of resuming directly into the original source session** (`project-import.md:21`). Mechanically: the provider adapter's `forkThread` must return a **new** native id, and the import refuses if it returns the source id (`projectImportRoute.ts:594-608`; `ProviderService.ts:4049-4063`). So the conversation can continue natively on its own copy.

### 1.5 When the source CLI is not installed

- **Listing** still works from data. The docs warn: "The scan depends on accessible local provider data. A provider being authenticated does not guarantee that another machine's history is available locally. Review source errors before treating an empty result as an empty account." (`project-import.md:11`). A failed source is shown per provider and account, not swallowed (`ProjectImportPanel.tsx:239-251`).
- **Copying** needs the provider enabled and an adapter that can fork: `ensureProviderEnabled` raises "… is disabled in Settings > Providers" (`enabledProviderAdapter.ts:22-33`) and a provider without a fork answers "This provider cannot copy native conversations" (`ProviderService.ts:3988-3993`). If the service is missing it answers "Native conversation copying is unavailable" (`projectImportRoute.ts:479-486`).
- **Version mismatch** is a first-class message: "An incompatible Codex CLI reports an upgrade instruction instead of attempting an unbounded import" (`project-import.md:31`).
- A disabled account is refused with "The account this conversation was found in is unavailable. Refresh the list." and older history with "The source account is unavailable. Enable it to load older messages." (`projectImportRoute.ts:165-182,382-386`).

### 1.6 How it avoids duplicates and recovers

- A **project-import origin** row per source conversation (`sourceKey, provider, sourceHome, externalId, projectId, threadId, status`) is reserved under one keyed lock before any copy, and `complete`d after (`projectImportRoute.ts:83,488-504,719`; `apps/server/src/persistence/projectImportRepository.ts`).
- `listProjectImports` marks a conversation `alreadyImported` when its origin is completed, or when a recorded native binding already names that external id (`projectImportRoute.ts:283-289,325-328`).
- `importProject` returns `already-present` for a completed origin or a known binding before doing any work (`:360-373`).
- Retrying a failed import reuses the reserved thread id and deterministic command ids, so nothing is duplicated ("The ledger and native binding survive failures… deterministic command IDs prevent replay", `:591-592`).
- Each request imports **one item** (`projectImport.ts:60-68`), so progress, stop and retry are between durable steps; the catalog itself is cached for 30 minutes (`projectImportRoute.ts:73`).

**Licence:** Synara is MIT; no code was copied here. The design is described, not transplanted.

---

## 2. T3 Code and Orca

### 2.1 T3 Code (clone `b387e3a`, 17 November 2026)

**It imports outside sessions, once, in the wizard.** A "Projects" section on the welcome page offers one checkbox for "Import N projects and their threads", a "Review all N" disclosure, "Select all"/"Select none" and the project list (`apps/web/src/components/onboarding/WelcomePage.tsx:823-895`); the default selection is recently active *new* projects (`:813-817`). The sources are Claude Code, Codex and Pi (`:1853-1856`). Docs: "Imported projects include Codex and Claude conversations active within the last 30 days. You can continue those conversations in T3 Code" (mirror of `docs/user/welcome-wizard.md`, quoted in *Sources*; that page is not in the local clone).

| Aspect | T3 Code | Evidence |
|---|---|---|
| Window | 30 days hard-coded; home, temp, `~/Downloads` and `~/Documents/Codex` excluded | `AgentSessionScanner.ts:84,858-869,1989` |
| What is kept | 200 messages: the first user prompt and the newest remaining user/assistant text; **tool activity and attachments omitted** | `AgentSessionScanner.ts:86-91` |
| Budgets | 4 GiB per transcript, 4 GiB and 100 transcripts per project, 100,000 records per call; unreadable or malformed conversations skipped | `AgentSessionScanner.ts:86-91` |
| Dedupe | A transcript identity (path, size, mtime, device, inode, birthtime) recorded on the runtime; an identical file is `AlreadyImported` and skipped | `AgentSessionScanner.ts:800-836,2130-2152`; `AgentSessionImporter.ts:280-289` |
| Continuation | The imported thread stores a **resume cursor for the original session** (`{resume: providerSessionId}` for Claude, `{threadId}` for Codex) and `nativeThreadRef`; the next turn resumes it | `AgentSessionImporter.ts:304-322,400-409` |
| Refusals | A Claude id that is not a UUID is "unresumable"; a thread with non-imported activity is refused | `AgentSessionImporter.ts:84,293-301` |
| Their own migration | V1→V2 copies the database; "does not recreate the old provider's live session"; the first new message starts a fresh provider session | `docs/user/thread-migration.md:1-33` |
| Handoff budget | 16,000 tokens by default (clamped 1,024–64,000), a separate 64,000-byte ceiling for imported history, one token charged per UTF-8 byte, references to omitted history the agent can read | `docs/user/portable-handoffs.md:31-47` |
| Alternative | Outside agents attach over MCP instead of importing | `docs/user/outside-agents.md:1-10` |

The cautionary evidence is theirs, not mine: import is reachable only from onboarding (issue #10961), there is no per-session picker (discussion #18025), and the 30-day window, excluded folders and one-time snapshot silently hide sessions with no refresh (discussion #27585). All fetched 21 November 2026.

### 2.2 Orca (docs fetched 21 November 2026)

Orca **resumes outside sessions rather than importing them**:

- "Agent Session History" is a right-sidebar panel that scans the CLIs' own stores (Claude, Codex, OpenCode, Cursor, Muse, Pi, OMP, Copilot and more); nothing has to be enabled, and it refreshes on demand (`orca.dev/docs/agents/session-history`, "Where the transcripts come from").
- The header shows `12 shown · 47 recent` and a search box that matches title, working directory, branch, model and preview text; scope is Workspace / Project / All; grouping is Project / Folder / Agent; per-agent toggles, "Hide empty sessions", and sort by Last updated or Created (`ibid.`).
- **Resume** "opens a new terminal in the session's cwd and runs the agent's resume command (e.g. `claude --resume <id>`, `codex resume <id>`)", with the same session id and cwd. **Resume in New Native Chat** opens it in the structured chat; the original conversation is left as it was (`orca.dev/docs/agents/native-chat`, "Move a conversation between chat and terminal").
- Resume only works from a **local** workspace; a remote workspace can browse but not resume. Cross-computer search is opt-in per computer and the data stays on each computer.
- No copy, no ledger: duplicates are impossible because nothing is copied. The cost is that resuming runs the CLI on the original session, so there is no frozen history if the file disappears.

### 2.3 What the comparison means for Retinue

Three models exist and Retinue sits between two of them:

| Model | Who | A second app can… | Cost |
|---|---|---|---|
| Frozen native copy | Synara | continue natively, original untouched | needs a provider fork per source; two histories to keep |
| Bind and resume in place | T3 Code, Orca | continue natively on the original | the file is a single point of failure; a resumed CLI shares the session |
| Read-only copy + bounded draft | Retinue today | read it; continue only when the same CLI still has the file | the safest, and the only one that works with no CLI installed |

Retinue should keep its read-only copy as the floor and add the two actions around it. Copying provider state (Synara's model) would need a `forkThread`-equivalent in each Retinue adapter, for a foreign session, which none has today; that is a later option, not a first version (section 6, option C).

---

## 3. Retinue today

### 3.1 The pipeline

The daemon reads three stores, each with an opt-in env override: `~/.claude/projects/<project>/<session>.jsonl`, `~/.codex/sessions/<YYYY>/<MM>/<DD>/rollout-*.jsonl`, and OpenCode's `opencode.db` (`import.rs:102-140`). OpenCode is copied to a temporary folder (WAL first) and opened `query_only`, so the owner's store is never written (`import.rs:260-297`). Never read: `.credentials.json`, `auth.json`, OpenCode's `credential`/`account`/`control_account`, account ids, instruction and environment blocks, thinking, tool output, image bytes, and Claude's `file-history-snapshot` lines (`import.rs:11-18`; pinned by `import_tests.rs:140-229`).

The five methods, their shapes and their caps are already documented in `docs/ipc-contract.md:902-914`; the window's flow is four steps plus a review (`ImportDialog.tsx:16-17`). Nothing here should be rebuilt.

**What an import writes** (`phase10.rs:90-185`): one `sessions` row (`provider` = source, `state = completed`, `provider_session_id` = source id when kept, `project_path` = recorded folder), one `turns` row per user message, `messages` for user/assistant text, `tool_calls` with a normalised title and a bounded summary, and a ledger row `(source, conversation_id) → sha256, session_id` in the same transaction. `data.import` carries `{source, importedAt, messageCount, sha256, conversationId, sourcePath, canResumeWith, truncated}`.

**The read-only guard** (`import.rs:1198-1219`) allows a turn only when the assigned employee's runtime provider equals the source **and** the source file still exists; otherwise `imported_read_only` (403), refused by both `session.prompt` and `session.queue.add` (`import_tests.rs:276-287`).

### 3.2 What the employee model gives us already

| Need | Already exists | Evidence |
|---|---|---|
| An employee is a bot with name, role, runtime/provider, model, instructions, project default | `Agent` fields | `apps/desktop/src/types.ts:189-232` |
| Project membership | `project.members`, `project.hire`, `project.release` | `crates/retinue-storage/src/phase9.rs:261-299`; `main.rs:1244` |
| A new conversation for a bot, idle before its first message | `agent.conversation.create` | `crates/retinue-daemon/src/team.rs:1662-1691` |
| A bounded "continue from" draft, nothing sent | `session.handoff.draft` (8 messages × 600 chars, 4,000-char excerpt) | `main.rs:4270-4327` |
| A picker for that handoff | `HandoffDialog`, reachable from the "More" menu on any conversation when another employee exists | `apps/desktop/src/ui/TeamMessages.tsx:85-123`; `App.tsx:3142` |
| An employee reading another conversation, paged and capped | MCP `list_conversations`, `read_conversation` (30,000 chars per message, 128 KiB per reply) | `crates/retinue-daemon/src/external.rs:398-399,793-794` |
| Routing vocabulary | project tags, bot tags, `preset.list`, `propose_hires` | `crates/retinue-daemon/src/presets.rs:99-190`; `main.rs:1240` |
| Owner approval before a hire | the CEO's rule ("Never hire anyone without a proposal the owner approves") and `propose_hires`, which makes one approval card per candidate ("Nothing is hired until the owner approves") | `main.rs:1240`; `crates/retinue-daemon/src/proposals.rs:68,379` |

### 3.3 Nine gaps, with consequences

**G1. The picker shows the newest 50 per source and never pages.** `loadRows` asks every source for `offset: 0, limit: 50` and ignores the `next` it is given (`ImportDialog.tsx:63`; the daemon supports `next` at `import.rs:763`). An older project conversation cannot be reached at all, and `import.apply` takes at most 50 anyway (`import.rs:37`). *Consequence:* a whole project cannot be imported today even though the daemon can describe it.

**G2. There is no project-first grouping or whole-project action.** The window groups by `row.projectPath` and shows a folder name (`ImportDialog.tsx:96-100,140-150`), but it cannot say "this folder is already a project", "this folder does not exist any more", or import a project in one go. The daemon can match a path to a project (`import.rs:1045-1047`; `project_id_for_path`, `lib.rs:4331`). *Consequence:* the strongest routing signal (the project) is computed after import and never written to the session.

**G3. Suggestions come after the writing, and there is no hire.** `ImportDialog.start` calls `import.apply` first, then `import.distribute` on the new ids (`ImportDialog.tsx:76-83`); the "who" step can only pick an existing employee (`:155-161`). The assistant rule is project → same agent → role → balance (`import.rs:1051-1083`). *Consequence:* an import with no suitable employee lands unassigned or by "balance", which is the one reason nobody trusts.

**G4. Read-only is invisible until a prompt fails.** The session summary carries no import field (`lib.rs:8563` builds it; `session_brief` at `:4212` also omits it), so the sidebar, header and companion show an imported conversation exactly like a live one; the only signal is the 403 text (`import.rs:1214-1218`). The contract even names the missing action ("'Give this to a bot' starts a new conversation", `ipc-contract.md:914`) but no button exists. *Consequence:* the first thing an owner does with an import is hit an error.

**G5. "Can continue in Claude Code" ignores whether that CLI is installed.** `canResumeWith` is set from file existence alone (`import.rs:794,827`); `import.assign` then keeps the provider id only if the receiving bot's provider matches (`:1171-1179`). *Consequence:* the picker promises something the machine may not be able to do, and the honest sentence differs from the badge.

**G6. There is no per-conversation size budget, only per-field caps.** The caps are 2,000 messages, 20,000 characters each, 2,000 tool rows, 2,000 characters of detail (`import.rs:44-49`), so one conversation can be ~40,000,000 characters of message text (40 MB of ASCII, up to 160 MB of UTF-8) plus 4,000,000 of tool detail, and one `import.apply` of 50 can reach roughly 2 GB of ASCII text (up to 8 GB at the UTF-8 worst case). That is the shape of the 20 November lesson: `tool_calls.data.raw` alone had grown to 982 MB in 2,410 rows and was the whole database (`.claude/skills/retinue-development/references/lessons.md:5-11`). *Consequence:* the import is bounded per field but not per conversation or per call, and `truncated` only records the message cap (`import.rs:368-372`).

**G7. Three OpenCode rows are wrong, and OpenCode is copied per conversation.** The scan's `bytes` for OpenCode is the database size repeated for every session (`import.rs:212,224-229` summed at `:682`), which can read as gigabytes; the preview always says `canResumeWith: "opencode"` and `alreadyImported: null` (`:827-828`), so an already-imported OpenCode conversation is offered again and looks new; and reading one conversation copies the whole database (`read_opencode` at `:555-557`, through `TempCopy`), so 50 conversations mean 50 copies. *Consequence:* a slow, misleading OpenCode experience — the one source most likely to be large.

**G8. An import and its continuation have no link.** The handoff draft stores nothing and the new conversation records nothing about where it came from (`main.rs:4274-4297`). *Consequence:* after "continue with Ada", nobody — owner or employee — can trace the conversation back, and the imported one cannot show "continued in Ada's conversation".

**G9. The "share them out" path writes before it decides.** With `who === 'distribute'` the apply runs with `assign: {}`, so every conversation is written unowned and only then assigned item by item (`ImportDialog.tsx:76-91`); closing the dialog on the review step leaves them unowned with no surface that lists them. *Consequence:* a cancelled review leaves a partial state nobody sees.

Two smaller facts worth recording: the window fixes the window at 30 days with no control (`ImportDialog.tsx:44`), and `runtime_for` falls back to the bare provider name as a runtime id when none is installed (`import.rs:1010-1016`), which no runtime lookup will match.

---

## 4. The proposal: import to employees

### 4.1 The rule

**An import is not a conversation until an employee owns it.** The picker's job is to say, for each source conversation, which employee it should belong to and how far it can go; the daemon's job is to write nothing until the owner confirms, and to keep the read-only floor for everything else.

### 4.2 Three outcomes per conversation

| Outcome | When | What happens | Owner sees |
|---|---|---|---|
| **Continue** | the employee's provider is the source's, and the source file is still there | the imported conversation becomes live: prompts allowed, `resumable`, resumed natively by the CLI (today's guard, `import.rs:1198-1219`) | "Continues in Claude Code with Claude" |
| **Carry over** | any employee whose provider differs | the import stays read-only; a **new** conversation is created for that employee in the same project, with a bounded, editable first message that summarises the last exchanges, names the imported conversation id, and says the rest can be read with `read_conversation` | "Claude read the history; Ada starts from a summary" |
| **Keep as history** | no employee chosen yet | read-only; the conversation is readable as context by anyone assigned later | "Imported — read only" |

This is T3's two-mode honesty (bind and resume, or fresh session from a bounded handoff, `thread-migration.md:22-33`) applied per conversation, with Synara's "never touch the original history" (`project-import.md:21`). The bounded draft already exists (`session.handoff.draft`: 4,000 characters, 8 messages); the change is to include the source conversation id and the employee's provider, and to offer it as one button rather than in the More menu.

**Why not copy the provider's session state ourselves (Synara's model)?** It needs, per adapter, a way to fork a foreign conversation into a new native one, and a check that the new id differs — `ProviderService.ts:4021-4063` shows how much that entails. Retinue's adapters resume their own sessions; none forks a foreign one today. Unverified for all three, so it is option C, not the first version.

### 4.3 Grouping and suggestion

1. **Group by project.** Canonicalise each conversation's `projectPath` (case, separators, symlinks, and the git root when the folder is a worktree), then match it against existing projects by `repoIdentity` first and path second (Retinue already normalises `repoIdentity` per project, `types.ts:245-246`). A group that matches no project offers "Create the project" with the folder name; a group whose folder is gone says so and offers the same relink control Synara has (`ProjectImportProjectCard.tsx:106-138`).
2. **Group inside a project by agent**, with a toggle Group by project / folder / agent, because an owner thinks both ways and Orca proves the control is cheap (`session-history`: "Group — Project, Folder (Agent)").
3. **Suggest an employee per conversation, before writing.** Extend the existing four-step rule (`import.rs:1051-1083`) and run it on preview rows:
   - **Project** — the folder is a project and the employee is hired into it; strongest, exact.
   - **Provider** — the employee's runtime provider equals the source; the only case where Continue is possible.
   - **Role** — a word from the title or first message appears in the employee's role, instructions or tags (today's rule, kept as a suggestion, never silent).
   - **Balance** — the least loaded employee, last.
   - **Hire** — when no employee speaks the source provider: propose one, with that provider, the project's model override or the provider default, and optionally a preset persona (`preset.list`, `presets.rs:174-190`). Hiring is shown as an approval card in the shape `propose_hires` already uses (one card per candidate, a group "Hire all", nothing hired until approved, `proposals.rs:68,379`), and happens only on confirm.
   Show the reason in plain words (the current `detail` string is already good, `ImportDialog.tsx:174`).
4. **Nothing is written until the owner confirms the "who" step.** That removes G3 and G9: the review is of intentions, not of rows already in the database.

### 4.4 Bulk import of a whole project

One action per project group: find or create the project by path (the daemon's `project.create`, `team.rs:1743-1757`), import every conversation in the window in batches of at most 50 (`import.rs:37`), assign each to its suggested employee, and hire only if the owner ticked the hire card. The daemon needs a project-scoped preview so the window never has to page through 200-row limits by hand (G1), and `import.apply` needs a `projectKey` mode so the 50-per-call cap is the daemon's batching, not the window's.

### 4.5 The CEO

The CEO already plans and staffs projects and must ask before hiring (`main.rs:1240`). Two options:

- **v1 (recommended):** the window's review step offers "Ask the CEO to divide these". It opens a message to the CEO with the imported conversation ids, titles, sources and project names (metadata only, no transcript), and the CEO replies with a proposal the owner applies with one click. Nothing new in the daemon beyond the existing `message_blob`/conversation path.
- **v2 (owner decision):** a metadata-only MCP tool that lists imported conversations and assigns one. It must **not** expose scanning or applying: those read the owner's filesystem. Treat it as a new reach for bots (privacy, and a bot spending the owner's history), and add its rows to the threat model.

### 4.6 Privacy: nothing leaves the machine

- **Reading is local.** The daemon reads the three stores and writes Retinue's own database; the scan parses nothing and touches no credential file (`ipc-contract.md:906`). No network call is made on this path.
- **Copying is selective and already so.** Text and tool *labels*, never credentials, thinking, tool output, file snapshots, environment or account ids (`import.rs:11-18`; tests `import_tests.rs:152-160,191-199,221-226`).
- **The only egress is a deliberate Continue.** A bot's turn goes to its provider as any turn does; the draft carries a bounded excerpt, and the rest stays on disk and is read on demand (`read_conversation` is paged at 128 KiB, `external.rs:793-794`). A raw foreign transcript must never be pasted into a prompt.
- **Remote.** `import.*` is already `never` (`tiers.json:1846-1851`) and its events are `drop` (`:2212-2213`); every new `import.*` method gets the same class and a parity test. **Never import from a phone**: no method is reachable.
- **But imported history becomes ordinary history.** Once written, a paired viewer may read the whole conversation (`session.get` is `viewer`, `tiers.json:40-45`). That is coherent — it is Retinue's history now — but it is a decision, not an accident; see open decision 1.
- **Threat model.** Add a section 16 row: "a bot or a paired device reads a transcript that was written by another tool and imported", with the class `never` for import and the redaction that already applies.

### 4.7 Size: bounded per conversation, not just per field

- Keep what is stored: text, a tool title (`Bash: npm test`), and at most a 200-character detail; never the provider's raw payload (today's rule, `import.rs:625-633`).
- Add a **per-conversation text budget** (proposed 2 MiB) and a **per-call batch budget** (proposed 50 MB), measured before writing, with the existing `truncated` flag and notice extended to say *what* was dropped ("later messages were not imported"). Two MiB is roughly 500 pages: enough for any real conversation, and 20 such imports are 40 MB, not 80 GB.
- Keep the message cap at 2,000 and consider reducing the per-message text cap for imports from 20,000 to about 8,000 characters; the tail of a huge message is rarely why the owner imported it.
- OpenCode: copy the database **once per batch**, not once per conversation (G7).
- `import.distribute` currently reads every session row and filters in Rust (`import_sessions`, `phase10.rs:59-85`), so it grows with the database. Measure with `EXPLAIN QUERY PLAN` before adding an expression index; the 20 November lessons show how easily a partial index is ignored (`.claude/skills/retinue-development/references/lessons.md:564-571`).

### 4.8 Remote devices

| Action | Phone / browser | Another machine (link) |
|---|---|---|
| Scan, preview, apply, assign, hire, continue | never | never |
| Read an imported conversation after it is imported | viewer, as any conversation | viewer, through the link |
| Start the import | only at the computer that holds the files | run it on that machine; nothing new needed |

The first row is today's `never` row; state it in the dialog's copy ("Imports run on this computer only") so nobody expects it from the phone.

### 4.9 Copy

- Import dialog, step 1: "Found on this computer. Imports run here, only."
- A project card: "4 conversations · newest 18 Nov · already a project" or "New project".
- The read-only banner: "Imported from Claude Code on 14 Nov. This is read-only history."
- Buttons: "Continue with Claude" (Continue), "Give this to another employee…" (Carry over), and for the latter the honest line: "Ada gets a short summary and the latest messages. The full history stays here for her to read."
- Never "synced", never "resumed" for a read-only import.

---

## 5. What it needs: backend and frontend

### 5.1 Backend (daemon, `crates/retinue-daemon/src/import.rs`)

| Step | Change | Size |
|---|---|---|
| B1 | `import.scan`: add per source `installed` (a runtime of that provider exists), `runtimeId`, `providerStatus`, and fix OpenCode `bytes` (sum of message lengths once per batch, not database size per session) | S |
| B2 | `import.preview`: fix OpenCode `alreadyImported` from the ledger and `canResumeWith` from file **and** a matching runtime; add `archived` where the source knows it; keep `next` | S |
| B3 | `import.projects {sources, sinceDays}` — the same scan rolled up: `{projects: [{key, path, exists, existingProjectId, total, bytes, newest, conversations: [preview row]}]}`, bounded (say 1,000 conversations, `truncated` when cut). This is what makes G1 and G2 fixable | S–M |
| B4 | `import.distribute` accepts `items` (preview rows) as well as `ids`, returns `sessionId: null` for a preview row, and adds `hire: {runtimeId, model, name, role, presetId?}` when no employee speaks the source provider. Reuse the four-step rule; keep the 5-per-employee cap | M |
| B5 | `import.apply`: accept `projectKey` (every conversation in the window for that project, batched at 50, returning `next`), and `assign` with a `mode` of `ask`, `project`, `agent` or `ceo` (plus `agentId` or `projectPath`). In `project` mode find-or-create the project for that path before the first write | M |
| B6 | `import.assign`: accept `projectPath` to find or create the project, instead of only an existing `projectId` | S |
| B7 | Session summary: add `imported: {source, canResumeWith, readOnly}` to `session_event_summary_locked` (`lib.rs:8563`) and `session_brief` (`:4212`), derived from `data.import`; the UI needs it for G4. One `json_extract`, no new column | S |
| B8 | `import.continue {sessionId, agentId, note?}`: if Continue is possible, assign and say so; otherwise create the conversation for the employee (`agent.conversation.create` semantics) with the handoff draft extended by "The full history is conversation <id>; read it with read_conversation", and record `data.origin.importedFrom` on the new row and `data.import.continuedAs` on the old one (G8) | M |
| B9 | Per-conversation and per-call size budgets (G6), plus the OpenCode batch copy (G7) | S–M |
| B10 | `tiers.json`: rows for any new method at `never`; event rows `drop`; extend the parity test | S |

### 5.2 Storage

No new table is needed for v1:

- the ledger (`phase10.rs:18-23`) already makes a repeat import harmless;
- `sessions.agent_id` and `data.projectId` already express ownership;
- `data.import` and `data.origin` take the two new links (`continuedAs`, `importedFrom`) as JSON patches, through the existing `merge_session_data` (`lib.rs:4247-4251`).

Two later additions, only if v1 proves the need:

- **An import job row** (id, source, project key, total, done, failed, created, state) so a 300-conversation batch survives a restart and the window can show it after reopening. Synara does this per item (`ProjectImportOrigin`), not per batch; T3 relies on the ledger. Retinue's ledger + idempotent apply covers the safety need; a job row is about *visibility*, so it comes after the UX.
- **An index for the import lookups** if `import_sessions`' full scan shows up in measurement (`phase10.rs:59-85`). Follow the lessons' rule: `EXPLAIN QUERY PLAN` first, then a partial expression index whose `WHERE` repeats every condition.

### 5.3 Frontend (`apps/desktop/src`)

| Screen | What changes | Size |
|---|---|---|
| `ImportDialog` step 1 | Per source: data found, newest/oldest, whether that CLI is installed, and the "runs on this computer only" line. Add "since 30 days / 90 days / everything" with the count each would add (G5, window) | S |
| Step 2 | Project cards (title, path, count, providers, "already a project" / "new project" / "folder missing"), a search that also matches conversation titles and first prompts, group toggle project/folder/agent, "Import this project", "Show older" paging, include archived (G1, G2) | M |
| Step 3 (new order: who, then import) | One row per conversation with the suggested employee card and reason, a change picker, "Hire …" cards where no provider matches, then "One employee", "the CEO", "Share them out as suggested"; the import starts from here (G3, G9) | M |
| Step 4 | Progress per conversation, Stop after this one, Retry failed, and a summary ("Imported 23: 19 to Ada, 4 to the CEO; 4 can continue in Claude Code") | S |
| Session list and header | An "Imported" mark with the source logo and a read-only banner; buttons "Continue with <name>" and "Give this to another employee…" (the existing `HandoffDialog`, prefilled to the project's members); `imported_read_only` gets a card with those actions instead of a bare error (G4, G8) | M |
| Demo and fixtures | `preview/fixtureBridge.ts` (the import rows at `:488-491` and the `import.scan` reply at `:1341`) gains the new RPC replies for every state: source installed / not installed, folder missing, already imported, read-only, continued, hire needed, error. Each new screen state and error gets its demo entry, per the 21 November rule (`AGENTS.md:5-7`) | S–M |
| Copy and CSS | `importDialog.css`, the banner, and the sentences in 4.9 | S |

### 5.4 Tests and gates

- Daemon: a project-grouping test on synthetic fixtures (two providers, one folder, one worktree), an idempotency test for project mode, a suggestion-before-write test (`sessionId: null`), a hire suggestion test (no runtime of the source provider), a budget test (a 3 MiB conversation is cut with a notice), an OpenCode `alreadyImported` test, an `import.continue` test (Continue vs Carry over), and the existing never-copied secrets assertions (`import_tests.rs:140-320`).
- `tiers.json` parity: every new method and event has a row.
- Window: mounted tests for the banner and the two actions; the import dialog's steps; the preview fixture paths.
- The full gates from `.claude/skills/retinue-development/references/verification.md`, in the lane's worktree; the Director runs them on the merged result.

### 5.5 The smallest useful first version

Order matters: the backend can land and be tested before any UI.

| # | Item | Why this first |
|---|---|---|
| 1 | B1, B2, B7 (source health, honest `canResumeWith`, OpenCode rows, `imported` on the summary) | Three small fixes remove three of the nine gaps (G4, G5, G7) and make the rest possible |
| 2 | B4 with preview rows and a `hire` suggestion | The "smart" half: a suggestion before writing, and a route when no employee fits |
| 3 | B3 + B5 + B6 (project roll-up, project mode, project creation) | Whole-project import, the owner's most likely first action |
| 4 | Frontend: project cards, suggestion step before import, paging and search | The dialog that makes 1–3 usable |
| 5 | Frontend: the imported banner and "Continue with …" / "Give this to another employee…" | Closes G4 and the contract's missing action |
| 6 | B8 `import.continue` and the two links | Makes "continue" one click and traceable; may be deferred if 5 ships with the handoff dialog |

Estimate: **backend S–M** (roughly two to four lane-days with tests), **frontend M** (three to five days), **B8 M** on top. Nothing here needs a new listener, a new credential, or a new egress.

### 5.6 Deliberately not in v1

- Synara-style frozen native copies (option C).
- A Cursor source. It is a real fourth source (`~/.cursor/projects/<slug>/agent-transcripts/<id>/<id>.jsonl`, owner-requests `:466`) and Codex's own importer covers it (`:109-111`), but it is a new parser and the ACP runtime's resume behaviour is unverified here.
- Importing from a linked machine, or any remote trigger.
- A durable import job row and a background watcher (`docs/research/synara-2026-11.md` already advises against a background sync for other reasons).
- Unbounded tool output or attachments.

---

## 6. Options, cost and risk

| | **A. Polish what exists** | **B. Project-first smart import (recommended)** | **C. Synara-style frozen native copies** |
|---|---|---|---|
| What the owner gets | paging, a badge, correct OpenCode rows | grouping by project, a suggestion and a hire before writing, whole-project import, a visible read-only state, continue in one click | every import can continue in its own conversation, on its own copy |
| Cost | S | S–M backend, M frontend | M–L, plus a fork primitive per adapter and cleanup of native copies |
| Risk | low | medium: path identity (symlinks, worktrees, moved folders), a first write before assignment if the order is not changed, a hire is a spend | high: unverified adapter support; two histories per conversation; a copy can break the original CLI's expectations |
| Fits Retinue's rules | yes | yes: nothing leaves the machine, import stays desktop-only | only if copying is done locally and never writes the owner's stores |
| Evidence it can work | the existing five methods | the same methods plus `project.*`, `agent.conversation.create`, `session.handoff.draft`, `presets` | `ProviderService.ts:4021-4063` shows the shape; Retinue has no equivalent |

**Recommendation: B.** It uses machinery Retinue already has, keeps the read-only floor that makes the feature safe with no CLI installed, and delivers the employee fit the brief asks for without new native-copy risk. Option A leaves the two complaints an owner will actually have (it cannot import a project; it cannot see what is read-only) and option C spends the most on the one thing that is already honest.

---

## 7. Open decisions for the owner

Each has a recommendation; none blocks the first four backend items.

1. **Should imported history be readable from a paired phone?** Today it would be, at viewer, like any conversation (`tiers.json:40-45`). *Recommend: yes*, because after import it is Retinue's own history, with an "Imported from …" mark in the phone list so nobody confuses it with work done by a bot.
2. **When a whole project is imported, may Retinue create the project automatically?** *Recommend: yes* (it is a folder the owner chose); **hiring** stays a confirm, because it spends money and the CEO's own rule is to ask first (`main.rs:1240`).
3. **Should "Continue with another employee" be offered for every import, or only when the owner opens it?** *Recommend: offered in the banner, never automatic*, and the draft is editable before it is sent.
4. **Should the CEO be able to list and assign imports through MCP?** *Recommend v2: no scanning or applying for any bot; at most a metadata-only "assign an imported conversation" at Operator, with the window's "Ask the CEO to divide these" as the v1 path.*
5. **Add Cursor as a fourth source?** *Recommend: after v1*, as its own small lane; the parser reference is Codex's `cur.rs`, the path is known, and the runtime question is the open part.
6. **Should the 30-day window become a choice?** *Recommend: yes*, default 30 days, with "N older conversations are not shown" so nothing vanishes silently — T3's issue #27585 is exactly this complaint.
7. **Import on another machine, from this one?** *Recommend: no.* Run the import on the machine that holds the files; the sessions then appear through the link as any others do.

---

## 8. What I could not verify

- **Any of the three products running.** Synara and T3 Code were read from source and docs; Orca from its documentation only. I did not install, build or run any of them.
- **Synara's copy working end to end.** The contracts, route and provider service show the shape and the checks, but I did not follow `forkThread` into the Codex and Claude adapters to see what a copy costs or whether every historical turn survives.
- **T3 Code's picker as shipped.** The wizard page in this week's clone is `WelcomePage.tsx`; the "Import your projects" documentation page quoted here is a mirror at an older commit and is not in the local clone.
- **Retinue at runtime.** Every Retinue statement is source-level at `cb8d736`. I did not run the app, the daemon, the import, or a resume. In particular I did not prove that a Claude or Codex conversation imported and assigned to a matching employee actually resumes with `--resume`; `import_tests.rs:252-274` proves only the guard's decisions.
- **Cursor's transcript format and resume.** Referenced from the earlier study (`owner-requests-2026-11-14.md:109-111,151-155`); not read this round.
- **The size numbers.** The 40,000,000-character worst case is arithmetic on the caps at `import.rs:44-49`, not a measured conversation; real imports are far smaller.
- **The demo coverage mechanism.** The 21 November rule is recorded in `AGENTS.md:5-7`; I did not run the coverage test, so I cannot name exactly which fixture the conversation import uses today.

---

## Sources

**Synara** (documentation fetched 21 November 2026; source clone at `c8c1ed4`, 18 November 2026):

- [Import Codex and Claude projects](https://www.trysynara.com/docs/features/project-import) (0.9.0, and the 1.0.0 account-aware section).
- [Documentation index](https://www.trysynara.com/llms.txt), for the page's canonical address.
- Clone: `packages/contracts/src/projectImport.ts`; `apps/server/src/orchestration/projectImport{Catalog,Route,Sources}.ts`; `apps/server/src/provider/{codex,claude}ProjectImport.ts`, `Layers/ProviderService.ts`, `enabledProviderAdapter.ts`; `apps/web/src/projectImport/*`; `packages/shared/src/locales/en/workspace.json`; `CHANGELOG.md` (0.9.0 on 2026-09-21, 1.0.0 on 2026-10-05).
- Licence: MIT (`LICENSE`). No code copied.

**T3 Code** (clone `b387e3a`, 17 November 2026):

- `apps/server/src/project/AgentSessionScanner.ts`, `apps/server/src/project/AgentSessionImporter.ts`, `apps/web/src/components/onboarding/WelcomePage.tsx`.
- `docs/user/thread-migration.md`, `docs/user/portable-handoffs.md`, `docs/user/outside-agents.md`.
- Upstream, fetched 21 November 2026: issue [#10961](https://github.com/pingdotgg/t3code/issues/10961), discussion [#18025](https://github.com/pingdotgg/t3code/discussions/18025), discussion [#27585](https://github.com/pingdotgg/t3code/discussions/27585), the welcome-wizard page quoted from a mirror of commit `44215da` (`git.cafe/pingdotgg/t3code`).

**Orca** (fetched 21 November 2026): [Agent session history](https://orca.dev/docs/agents/session-history), [Chat UI (native chat)](https://orca.dev/docs/agents/native-chat).

**Retinue**, this worktree at `cb8d736`:

- `crates/retinue-daemon/src/import.rs`, `import_tests.rs`, `team.rs`, `main.rs`, `external.rs`, `presets.rs`, `queue.rs`.
- `crates/retinue-storage/src/phase10.rs`, `phase9.rs`, `lib.rs`.
- `apps/desktop/src/ui/ImportDialog.tsx`, `TeamMessages.tsx`, `App.tsx`; `apps/desktop/src/types.ts`; `apps/desktop/src/preview/fixtureBridge.ts`; `apps/desktop/src/remote/tiers.json`.
- `docs/ipc-contract.md:900-914`; `docs/releases/0.2.0-beta.4.md:10`; `docs/reference/invariants.md`; `docs/research/owner-requests-2026-11-14.md:93-240,466`; `docs/research/synara-2026-11.md` (18 November; it covers no conversation import — its only "import" mentions are browser cookies and skills); `docs/research/competitor-adaptation-2026-11.md`.
- `.claude/skills/retinue-development/references/lessons.md:5-11,537,564-571`; `AGENTS.md:5-7`.

No code was changed, no test was run, the installed app was not started, and no provider history on this computer was opened. The only data read from outside the repository were the two read-only clones and the documentation and upstream pages above.

---

## Appendix: proposed skill update (for the Director to apply)

`references/codebase-map.md`, in the import entry: "Conversation import is five methods in `crates/retinue-daemon/src/import.rs` (scan, preview, apply, distribute, assign) writing through `crates/retinue-storage/src/phase10.rs`; the window is `ui/ImportDialog.tsx`; `import.*` is `never` remote. An import is read-only unless the assigned bot's provider matches the source and the source file still exists (`refuse_read_only`). The UI cannot see that state: `session_event_summary_locked` carries no import field (2026-11-21)." And one lesson for `references/lessons.md` when the first version lands: "An import writes `data.import` but the sidebar summary did not carry it, so read-only was invisible until a prompt failed; surface state on the row, not only in the error."
