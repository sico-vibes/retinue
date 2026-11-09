# Brief: employees, hiring and the CEO (backend)

**Lane:** Gogo (OpenCode Go DeepSeek V4.1 Flash, high) in the owner's Retinue app, in its own git worktree, assigned through the Retinue MCP.

**Rules:**
- Never run `git commit`, `git push`, `git merge`, `git stash` or `git reset`, and never switch branches.
- Change nothing outside your worktree.
- Never print tokens, capabilities or credentials.
- Windows: use `RUST_MIN_STACK=33554432` for cargo tests. `crates/retinue-storage/src/lib.rs` contains one NUL byte, so edit it with your edit tool, never shell rewrites.
- The Director (Claude) builds the UI against the contract below at the same time. Keep the contract exactly. If something can't be done as written, choose the smallest change and say so in your report.

**Read first:**
- `AGENTS.md` (the top amendments and the Remote rules);
- `.claude/skills/retinue-development/SKILL.md`, then `references/codebase-map.md` and `references/lessons.md`.

**Set-up:** copy these two files from the owner's main checkout (`D:\Basem's\Coding\Blolblex`) into the same paths in your worktree. They are not committed yet.
- `docs/plans/hiring-brief.md` (this file)
- `apps/desktop/src/team/presets.json`

## The decision (owner, 8 November 2026)

Bots become **employees**, and projects become the main view.
- **Presets.** Retinue ships 28 preset employees in `apps/desktop/src/team/presets.json`. Each preset is a nickname, a Haze portrait, a role, a department, tags, a description and instructions.
- **Hiring.** The owner **hires** an employee into a project. One employee can be hired into many projects; hiring a preset again reuses the same employee.
- **The CEO.** The Manager becomes the **CEO**. When a project starts, the CEO can interview the owner and propose whom to hire. If no preset fits, it proposes a new employee with the existing `propose_bot`.
- Existing bots are employees too.

## What exists today (build on it)

- **`agents.project_id`** is the bot's default project. A bot's projects are worked out from its conversations plus that default (`Storage::agent_projects`, `crates/retinue-storage/src/lib.rs:2875`; `agent_last_project_id`, :2909).
- **Default project for a new conversation:** `team::default_conversation_project` (`team.rs:1488`).
- **Tags:** `agents.tags` (JSON, at most 8 lower-case tags of up to 24 characters; check at lib.rs:1571).
- **Portraits:** the allowlist is `valid_look` (lib.rs:7987). The portrait list is repeated in the tool schemas at `team.rs:1676` and `proposals.rs:72`.
- **The Manager:**
  - It is the row with `system_role='manager'`. A partial unique index allows only one (lib.rs:524).
  - Its defaults are `MANAGER_DESCRIPTION` and `MANAGER_INSTRUCTIONS` (`main.rs:964-966`), created by `manager.ensure` (`main.rs:1023`).
  - Its extra prompt lines are at `team.rs:470-482`, and `DEFAULT_PLAYBOOK` is at lib.rs:894.
- **Proposals:**
  - `create_proposal`, `propose_bot` and `apply_bot` are in `crates/retinue-daemon/src/proposals.rs`.
  - The owner decides with `proposal.decide {id, decision, edits?}` (`main.rs:1528`).
- **Remote policy:** `apps/desktop/src/remote/tiers.json`. A parity test fails when a daemon method has no row.

## Contract (the UI is built against exactly this)

### 1. Storage

**`project_members`**, a new table:

```
project_members(project_id TEXT NOT NULL, agent_id TEXT NOT NULL,
                hired_at INTEGER NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(project_id, agent_id))
```

- Deleting a project or purging an agent deletes its rows, either in code or with foreign keys, whichever matches the file.
- An archived agent keeps its rows.

**`agents.preset_id TEXT NULL`**, a new column, unique where it isn't null (partial unique index).

**One-time migration:**
- For every non-Manager agent, insert a row for its `project_id` (when set) and for every project it has a non-archived conversation in. `hired_at` is the earliest such conversation, or now.
- The Manager/CEO gets **no** rows: it belongs to every project implicitly.
- Running the migration twice must change nothing.

**New files:** put this in a new storage module, for example `crates/retinue-storage/src/phase9.rs`, following the existing `phase*.rs` pattern, so `lib.rs` changes stay small.

### 2. Preset catalogue

**Reading it:**
- The daemon reads `apps/desktop/src/team/presets.json` with `include_str!` and parses it once.
- Each preset's `instructions` is an array of lines; join them with `\n`.
- Add a test that every preset's `portrait` passes `valid_look`, every `color` is a known swatch key, `role` is at most 40 characters, `description` at most 255, tags are valid, and ids and nicknames are unique.

**`preset.list {}`** returns:

```
{ departments: [{id, name}],
  presets: [{id, nickname, portrait, department, role, color, tags, description,
             hiredAgentId: string|null}] }
```

`hiredAgentId` is the agent made from this preset (`agents.preset_id = id`, not archived), or null.

### 3. Members, hiring and releasing

**`project.members {projectId}`** returns:

```
{ members: [{agentId, hiredAt, sortOrder}] }
```

- Order by `sort_order`, then `hired_at`.
- `not_found` if there is no such project.
- The UI joins these rows with its own agent list, so return no other agent fields.

**`project.hire {projectId, agentId?, presetId?, runtimeId?, model?}`**
- Exactly one of `agentId` and `presetId` is given. Otherwise refuse with `invalid_argument`.
- With `agentId`: add the membership. Refuse with `not_found` for an unknown or archived agent. Refuse with `invalid_argument` for the CEO.
- With `presetId`:
  - If an agent with that `preset_id` exists and isn't archived, reuse it.
  - Otherwise create one from the preset:
    - name = nickname; if the name is taken (`name_key`), use "Ada 2", "Ada 3" and so on;
    - role, description, instructions, color and tags come from the preset;
    - `look` = `{"style":"haze","icon":"<portrait>"}`, using whatever `look` shape the existing Haze portrait path stores (check `LookEditor` and `valid_look`, and match it);
    - `preset_id` = the preset's id;
    - `runtime_id` and `model` = the given values, else the CEO's runtime and model, else the first available runtime;
    - `approval_mode` = `ask`.
  - Then add the membership.
- Hiring is idempotent: hiring someone already in the project returns the same result and changes nothing.
- Returns `{ agentId, created: bool, member: {agentId, hiredAt, sortOrder} }`.
- Emits the same change event agent updates already emit, so open windows refresh, plus a `project.members.changed {projectId}` event.

**`project.release {projectId, agentId}`**
- Removes the membership only. Conversations, the agent and its default project are untouched.
- If the agent's `project_id` equals this project, set it to null.
- Returns `{ released: bool }` and emits `project.members.changed`.

**Automatic hiring:**
- Creating a conversation for a non-CEO agent in a project it isn't a member of hires it into that project first.
- This covers `agent.conversation.create`, `session.new` with `agentId`, tasks, `assign_task` approval and side conversations: put the call where the conversation's project is decided.
- Setting `agents.project_id` through `agent.team.update`, `agent.create` or `agent.update` also hires the agent into that project.

**`agent.projects`** now includes every project the agent is a member of, with its current fields. Members with no conversations show `conversationCount: 0`.

**Delete and purge:**
- `project.delete` removes that project's memberships.
- `agent.purge` removes that agent's memberships.

**Remote** (`tiers.json`): add rows.
- `preset.list` and `project.members` are `read`, `minTier: "viewer"`.
- `project.hire` and `project.release` are `never`, with a `why` saying hiring can create an agent and agent creation is never remote.
- The parity test must pass, and the TypeScript side reads the same file.
- Add `project.members.changed` to the event policy the same way existing project events are listed.

### 4. The Manager becomes the CEO

**Naming:**
- New installs: `manager.ensure` defaults to name **"CEO"** and role **"CEO"**.
- One-time migration: if the Manager's name is exactly `Manager`, rename it to `CEO`. If its role is `Manager`, set the role to `CEO`.
- `system_role` stays `manager`, and the RPC names stay as they are.
- In user-visible strings the daemon writes (errors, prompt text, tool descriptions), say "CEO" where they say "the Manager". Don't rename code identifiers.

**New CEO tools** (Manager-only, next to `propose_bot` in `proposals.rs`):
- **`list_presets {department?, tag?}`** (read-only) returns the presets with id, nickname, role, department, tags, description and whether each is hired, plus the departments.
- **`propose_hires {project, candidates: [{preset?: string, bot?: string, reason: string}]}`**
  - 1 to 8 candidates. Each has exactly one of `preset` (a preset id) or `bot` (an existing employee's name or id).
  - It creates **one proposal per candidate**, of kind `hire`, with the payload `{projectId, projectName, presetId|agentId, nickname, role, portrait, reason, groupId}`. All candidates from one call share a `groupId` (a new UUID) so the UI can show them as one card with "Hire all".
  - The title is `Hire <nickname> into <project>`, and the detail is `<role> · <reason>`.
  - Refuse a candidate already in the project: name it in the tool result and skip it.
  - Return a short text listing what is waiting for approval.
- **Approving a `hire` proposal** runs `project.hire` with that payload, and `edits` may set `runtimeId` and `model`. Rejecting it does nothing.

**Prompts:**
- Update `MANAGER_INSTRUCTIONS`, the Manager prompt lines at `team.rs:470-482` and `DEFAULT_PLAYBOOK` so they say:
  - You are the CEO. Your team works in projects.
  - When a project starts, or the owner asks you to staff one, interview them briefly: the goal, the stack, what done looks like, and what matters most.
  - Then call `list_presets` and `propose_hires` with the smallest team that fits (2 to 6), each with a one-line reason.
  - Use `propose_bot` only for a need no preset covers, and say so.
  - Never hire without a proposal the owner approves.
- Replace a stored playbook only when it is still exactly the old default.

**Retinue MCP:** add a read-only `list_presets` tool next to `list_bots`, in the Retinue MCP server code, with the same output as `preset.list`.

## Tests (required)

- The migration seeds memberships from `project_id` and from conversations, skips the CEO, and running it twice changes nothing.
- `project.hire` with a preset creates one agent. Hiring the same preset into a second project reuses that agent, giving two memberships and one agent. Hiring twice into the same project changes nothing. A taken name gets " 2".
- `project.hire` refuses the CEO, an unknown agent, and a call with both or neither of `agentId` and `presetId`.
- `project.release` keeps conversations and clears a matching `project_id`.
- `agent.conversation.create` in a new project hires the agent automatically.
- `preset.list` reports `hiredAgentId`.
- The catalogue validation test passes.
- The CEO rename happens only for the default name.
- The `propose_hires` round trip works: proposals are created with a shared `groupId`, approval hires, and a candidate already in the project is skipped.
- The tier parity test passes.

## Gates

Run these in the foreground and report in the same turn:

- `cargo test -p retinue-storage` and `cargo test -p retinue-daemon` (with `RUST_MIN_STACK=33554432`)
- `cargo clippy -p retinue-daemon -p retinue-storage --all-targets -- -D warnings`
- `npm run typecheck` (the tiers JSON is shared with TypeScript)

The first cargo build is cold and takes several minutes.

## Report

For each section (1 to 4):
- what you did, with `path:line`;
- every file you changed;
- the last lines of each gate's output;
- every decision the brief didn't specify;
- anything unverified;
- proposed text for the skill (`references/codebase-map.md`: presets, `project_members`, the hire RPCs, the CEO tools).
