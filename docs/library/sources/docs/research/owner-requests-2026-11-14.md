# Owner requests, 14 November 2026

Research for four requests the owner made through the Director. Research only: no code was changed. Every claim below cites either a place in this repository (`file:line`) or an outside source (URL). Claims I could not verify are marked **unverified**.

Order of work: section 1, then 2, 3, 4. Each section was written as it was finished.

---

## 1. Showing that an image is being generated

### What the owner wants

A placeholder in the chat that shows an image is being made, the way ChatGPT shows its generated-image tile filling in. Today Retinue has two separate pieces: a tool row that says "Generating an image", and (for one provider only) a "Generating an image…" card.

### What the Codex app-server actually sends

The adapter's schema copy is the authority here (`crates/retinue-adapter-codex/schema/`, version in `schema/VERSION`).

**The item type.** `ImageGenerationThreadItem` is one variant of `ThreadItem` (`crates/retinue-adapter-codex/schema/ServerNotification.json:5513`). Its whole payload is:

```json
"properties": {
  "failure": { "anyOf": [{ "$ref": "#/definitions/ImageGenerationFailure" }, { "type": "null" }], "default": null },
  "id": { "type": "string" },
  "inputImages": { "description": "Observed local inputs for a completed generation, absent on older items.", ... },
  "revisedPrompt": { "type": ["string", "null"] },
  "savedPath": { "anyOf": [{ "$ref": "#/definitions/AbsolutePathBuf" }, { "type": "null" }] },
  "status": { "type": "string" },
  "transparentBackground": { "default": null, "type": ["boolean", "null"] },
  "type": { "enum": ["imageGeneration"], ... }
},
"required": ["id", "status", "type"]
```

(`ServerNotification.json:5445-5514`.) The `result`/`fileId` half of the item (`ServerNotification.json:5440-5462`) is the image bytes or a file id; the description at `:5451` reads "Either inline generated-image data or its durable attachment-store identity."

So, at the start of the item, the wire gives us: `id`, `type`, `status` (a free string, not an enum), and optionally `revisedPrompt`, `savedPath`, `failure`, `transparentBackground`, `inputImages`.

**There is no progress channel.** The full notification list in `ServerNotification.json` (`oneOf`, `:1-…`) contains:

- `item/started` → `ItemStartedNotification` (`:7762`, definition carries `item`, `startedAtMs`, `threadId`, `turnId`; `startedAtMs` at `:2660`).
- `item/completed` → `ItemCompletedNotification` (`:7842`, carries `item`, `completedAtMs`), and
- `item/mcpToolCall/progress` — for MCP tool calls only, not for image generation.

There is **no** `item/imageGeneration/progress`, no partial-image frame, no percentage, no width/height or aspect-ratio field, and no separate "revised prompt" notification. The only image-related notification besides started/completed is `item/mcpToolCall/progress`, which belongs to MCP tools.

**What `status` holds.** The field is declared as a bare `string` (`:5490-5492`) with no enum, so the schema does not promise a value set. Two values are evidenced from Retinue's own code: the adapter treats `"failed"` as a failure (`crates/retinue-adapter-codex/src/lib.rs:1002`) and a test feeds `"inProgress"` as a non-final state (`lib.rs:1977`). A third, `"completed"`, appears in a fixture (`lib.rs:1948`). Whether the real app-server sends exactly those three strings is **unverified** — the schema does not constrain them, and I did not run a live Codex session.

**What Retinue does today with those two events.**

- `item/started` with `item.type == "imageGeneration"` becomes `ToolStarted { kind: "imageGeneration", title: "Generating an image", raw }`, where the base64 is stripped (`crates/retinue-adapter-codex/src/lib.rs:1040-1046`, stripper at `:975-983`).
- `item/completed` becomes `ToolCompleted` (leaving the row with no bytes, `lib.rs:442-451`) plus an `ImageRequested` event with `savedPath`, inline base64, or the failure (`lib.rs:997-1013`).

**Do Claude Code and OpenCode give the same signal?** The claim in the brief is half right, and one half is wrong.

- **Claude Code: correct, there is no image-generation tool.** The Claude adapter's only image code is *input*: an attached image becomes a base64 `image` content block (`crates/retinue-adapter-claude/src/lib.rs:225-231`, test at `:1512-1517`). No generation tool, no hook, no event to map. The owner can confirm from Claude Code's own tool list at runtime, but nothing in Retinue's Claude path has a place to put one.
- **OpenCode: the claim that it exposes no signal is wrong.** OpenCode does generate images and does report them, through ACP:
  - the tool is named `experimental_image_generate` (`crates/retinue-adapter-acp/src/lib.rs:1104`), and the adapter recognises it by `name`, `toolName`, `title`, `rawInput.tool`, `rawInput.name` or `_meta["opencode/tool"]` (`lib.rs:1146-1155`);
  - the *finished* tool update carries `rawOutput.metadata = {path, model, jobID}`, which the adapter turns into a stem for the pending card (`lib.rs:1161-1165`);
  - a completion `notice` titled "Image generation" carries `{source, jobID, state, path}` plus the error text on failure (`lib.rs:1198-1214`).

  In OpenCode's own source, the tool description says it "runs in the background: the call starts a job and returns right away, and you are notified automatically when the image is saved" (`packages/core/src/tool/plugin/image-generate.ts`, `description`, fetched from `https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/core/src/tool/plugin/image-generate.ts`). Its output is `{id, path, model}` — an id, a path *without extension*, and the model. The completion notice is `<image id="…" state="…">…</image>` (`packages/core/src/image/notice.ts`). **No progress percentage, no partial frames.** The internal job does have a `running` state (`jobs.wait(...).info.status === "running"` in `image-generate.ts`), but it is not pushed to the client; the client learns "done" or "failed".

  OpenCode's AI layer *does* have a recording for `streams-a-partial-image-before-the-final-image.json` (`packages/ai/test/fixtures/recordings/openai-images/`), but that is a provider-protocol fixture inside `packages/ai`, not something the image tool forwards to a session. Forwarding it would be new work in OpenCode, not a signal Retinue can read today.

### Where the two signals currently diverge in Retinue

- **OpenCode** produces a real pending card. The daemon's media layer takes a `stem` (an absolute path without extension) and inserts a `pending` media row keyed by `tool_call_id` (`crates/retinue-daemon/src/media.rs:509-519`), then publishes a message with `meta.kind = "generated_image"` and `meta.status = "pending"` (`media.rs:605-634`). The card is `GeneratedImage` in the desktop app: a spinner and "Generating an image…" (`apps/desktop/src/ui/GeneratedImage.tsx:104-106`), covered by a mounted test (`GeneratedImage.mounted.test.tsx:98`).
- **Codex** produces only the tool row. The adapter emits `ToolStarted` at `item/started`, but the daemon creates a media row only from `ImageRequested` (`crates/retinue-daemon/src/main.rs:5434-5435`), which for Codex arrives at completion. So a Codex image shows a spinnerless activity row, then the picture appears.
- **The row survives the wait.** While any tool row is open the daemon widens its silence window to `RUNNING_TOOL_FACTOR = 6` × `TURN_SILENCE_LIMIT = 300 s` (`main.rs:5086`, `:5158`, logic at `:5205-5211`), i.e. up to 30 minutes. That matches the media layer's own 35-minute deadline (`media.rs:456`, `MEDIA_DEADLINE_SECS`). So a long image generation will not be killed as "silent" — worth knowing before anyone "fixes" a slow image by shortening timeouts.

### Recommendation: what the placeholder can honestly show

Show four things, all of them real:

1. **A spinner and the words "Generating an image…"** — already built (`GeneratedImage.tsx:105`). The important fix is to give Codex the same card: on `ToolStarted` with `kind == "imageGeneration"`, the daemon should insert a `pending` media row with that `tool_call_id` and no stem. `media::requested` already updates an existing row by call id (`media.rs:483-487`, `finish_ready` at `:565-583`, `finish_failed` at `:585-600`), so the completion event will move the same card from pending to the picture with no other change.
2. **Elapsed time, ticking.** The daemon already stores `startedAt` on the tool row and sends it in `tool.changed` (`main.rs:5385`). A card that counts up ("12 s", "1 min 4 s") states a fact, and is the honest stand-in for a progress bar. Do not draw a progress bar: nothing on the wire supports a percentage, and an invented one breaks the "no fabricated success/usage" rule in `AGENTS.md`.
3. **Nothing about shape until the bytes arrive.** Aspect ratio is not on the wire before completion (`ServerNotification.json:5445-5514` has no size field). Once the image is saved, the daemon knows the real width and height from the file — that is the moment to size the frame, not before. A fixed 1:1 (or 4:3) placeholder box with a neutral pattern is honest as a *layout* choice as long as nothing claims it is the image's shape.
4. **The revised prompt, after it arrives.** Codex sends `revisedPrompt` on the completed item (`:5474-5479`); OpenCode sends the model's output text in the notice body. Today the Codex adapter ignores `revisedPrompt` entirely (`lib.rs:997-1013` reads only `savedPath`, `result`, `failure`, `status`). Using it as a caption under the finished image ("Prompt: …") is real, provider-supplied text and a genuine improvement — but it is not available *during* generation, so it cannot feed the placeholder.

What the placeholder must not show, because no provider sends it: a percentage, a "remaining" estimate, a blurry preview or blurred-up version of the final image, or the model's prompt at the start (the prompt is the model's own text; at `item/started` the only fields we know about are `id`, `status`, `type`).

Small optional extra, still honest: when the request fails, the card already shows the reason (`GeneratedImage.tsx:107-109`), which the daemon bounds to one line and strips paths from (`media.rs:536-562`). Keep that.

### Assumptions I did not verify

- That the real Codex app-server emits `item/started` for every `imageGeneration` item (the schema allows it; only `item/started` exists as a lifecycle notification, but a live run was not made).
- The exact `status` strings the server sends.
- Whether Codex fills `revisedPrompt` at start as well as at completion.

---

## 2. Importing conversations from other coding agents

### First, a correction about capy.ai

capy.ai is real and it really does this: its FAQ says **"The desktop app can also import your local Codex and Claude Code sessions."** (`https://docs.capy.ai/faq`, section "Skills, MCP, and plugins").

**Its source is not public as far as I can find. The "source is on GitHub" part of the brief is unverified and looks false.** What I checked:

- The site links to `https://github.com/scrapybara` (its owner). Everything public there is SDKs, the `codecapy` PR bot, computer-use samples and `scrapybara-oss`; no app source and no importer (`https://api.github.com/orgs/scrapybara/repos?per_page=100`).
- `https://capy.ai/open-source` is a credits programme for *other* people's repositories, not a source release.
- GitHub searches for the product name and for its features turned up clones and third-party CLIs, no official repo.

That does not block the work, because there is a **better** precedent with public source: **Codex itself imports other agents' sessions**, in `openai/codex`, crate `codex-rs/external-agent-migration`. It is the closest thing to a reference implementation, it is MIT/Apache-licensed Rust in the same language as Retinue's daemon, and it covers exactly the two formats that matter most. I use it as the model below.

### How the reference implementation does it (`openai/codex`)

- **Sources it supports: Claude Code and Cursor, nothing else.** `codex-rs/external-agent-migration/src/detect/sessions/mod.rs` exports only `detect_recent_cla_sessions` and `detect_recent_cur_sessions`.
- **Claude Code**: it walks `<home>/.claude/projects/*/*.jsonl` (`.../detect/sessions/cla.rs`, `detect_recent_cla_sessions_with_limits`: `let projects_root = external_agent_home.join("projects")`, then each project directory, then every `*.jsonl`). It requires the recorded `cwd` to still exist (`require_existing_cwd: true`).
- **Cursor**: it walks `<home>/.cursor/projects/*/agent-transcripts/**/*.jsonl`, skipping a directory named `subagents` (`.../detect/sessions/cur.rs`, `cur_transcript_files`). It decodes the project folder name back into a path, and treats the reserved name `empty-window` as "no project" (`cur_project_cwd`).
- **Bounds**: 30 days maximum age and 50 sessions maximum (`codex-rs/external-agent-migration/src/model.rs`: `DEFAULT_SESSION_IMPORT_MAX_AGE` = 30 days, `DEFAULT_SESSION_IMPORT_MAX_COUNT = 50`).
- **Deduplication**: each import records the source file's SHA-256 and a ledger entry, and an import is skipped when the hash is unchanged (`codex-rs/external-agent-migration/src/sessions/ledger.rs`, `has_current_session_been_imported`; `SessionImportTarget::Existing { expected_source_content_sha256 }` in `.../sessions/mod.rs`).
- **Shape**: it parses into its own `RolloutItem`s (`.../sessions/records_cla.rs`, `records_cur.rs`) and writes them into its own rollout files (`.../sessions/export.rs`, `append.rs`), with a title derived and capped at 120 characters (`SESSION_TITLE_MAX_LEN`, `.../sessions/mod.rs`).
- **Rewriting**: it also rewrites instruction and config text that names the other agent (`RewriteProfile` in `.../source/cla.rs`, `.../source/cur.rs`). Retinue should **not** rewrite history text; see "what not to copy" below.

The app-server side is where the user-facing flow lives: `ExternalAgentConfig/detect` then `ExternalAgentConfig/import`, with `progress` and `completed` notifications. Those notifications are in the schema Retinue already vendors: `crates/retinue-adapter-codex/schema/ServerNotification.json` declares `externalAgentConfig/import/progress` (`:8247`) and `externalAgentConfig/import/completed`, and `ExternalAgentConfigMigrationItemType` (`:1425`) lists `AGENTS_MD, CONFIG, SKILLS, PLUGINS, MCP_SERVER_CONFIG, SUBAGENTS, HOOKS, COMMANDS, MEMORY, SESSIONS` (see also `openai/codex`, `codex-rs/app-server-protocol/schema/json/v2/ExternalAgentConfigImportParams.json`). Two lessons worth copying: **a scan phase separate from the import phase**, and **a progress stream** so a 100 MB import is not a frozen dialog.

### Where each agent's history actually lives — checked on this machine

These paths are not guesses: I listed and parsed the files on the owner's Windows machine today. Counts are from that machine and are given so the size of the problem is concrete.

**Claude Code** — `C:\Users\Sico Vibes\.claude\projects\<project>\<session-uuid>.jsonl`

- `<project>` is the working directory with every separator turned into `-` (for example `C--Users-Sico-Vibes-Desktop-DevCN-Retinue`). One folder per project, one `.jsonl` per conversation; the file name is the session id used by `--resume`.
- 62 project folders, 111 session files here; the largest is 89 MB.
- Each line is one JSON object with a `type`: `user`, `assistant`, `attachment`, `system`, plus bookkeeping types `custom-title`, `agent-name`, `mode`, `permission-mode`, `last-prompt`, `file-history-snapshot`, `file-history-delta`, `cost-state`, `queue-operation`, `atis-latch`.
- Message lines are `{"type":"user"|"assistant","message":{"role":…,"content":[…]}, "uuid","parentUuid","isSidechain","timestamp","cwd","sessionId","version","gitBranch"}`. Content blocks are `text`, `thinking` (with a signature), `tool_use`, `tool_result`, and `image` with **inline base64**.
- Sub-agent rows live in the same file with `isSidechain: true`.
- `custom-title` / `agent-name` hold the conversation title when the user renamed it; otherwise the first user message is the title.

**Codex** — `C:\Users\Sico Vibes\.codex\sessions\<YYYY>\<MM>\<DD>\rollout-<timestamp>-<uuid>.jsonl`

- 44 rollout files here; the largest is 169 MB.
- Every line is `{timestamp, ordinal, type, payload}`. Types seen: `session_meta`, `turn_context`, `response_item` (payload types `message`, `reasoning`, `function_call`, `function_call_output`, `custom_tool_call`, `custom_tool_call_output`, `additional_tools`, `configuration_update`), `event_msg` (`task_started`, `task_complete`, `token_count`, `thread_settings_applied`, `item_completed`), `token_usage_record`, `world_state`.
- `session_meta.payload` holds `session_id`, `id`, `cwd`, `originator`, `cli_version`, `source`, `model_provider`, `base_instructions`, and also **`creator_user_id` and `creator_account_id`** — account identifiers, which must not be copied.
- `world_state` embeds the whole `AGENTS.md` text and the instruction set. That is project instruction text, not conversation, and importing it would replay the wrong project's rules into a bot.

**OpenCode** — `C:\Users\Sico Vibes\.local\share\opencode\opencode.db` (SQLite, 164 MB here)

This is the one that breaks the "history is JSON files" assumption. OpenCode 2 stores everything in one database:

- `session_v2` (465 rows): `id` (`ses_…`), `project_id`, `workspace_id`, `parent_id`, `fork_session_id`, `fork_boundary`, `slug`, `directory`, `path`, `title`, `model` (JSON like `{"id":"deepseek-v4.1-flash","providerID":"opencode-go","variant":"high"}`), `cost`, `tokens_*`, `agent`, `permission`, timestamps, `share_url`.
- `session_message` (39,427 rows): `id`, `session_id`, `type`, `seq`, `time_created`, `data` (JSON). Types by count here: `model-switched` 26,036, `assistant` 12,673, `user` 255, `idle` 245, `agent-switched` 127, `synthetic` 72, `system` 19. An `assistant` row's `data` holds `content: [{type:"tool", id, name, state:{status, input, content}}]` and text parts.
- `session_part` (empty on this machine), `blob` (`hash, mime, bytes, width, height, data`), `blob_ref`.
- **`credential`** (`id, integration_id, label, value, connector_id, method_id, active`) — one row here. This is a secret store and must **never** be read by an importer. Same for `account` and `control_account`, which carry `access_token` and `refresh_token` columns.
- `kv` (2 rows) and `project` (345 rows) are config and project registry.

Note the size of `model-switched` rows: an importer must filter by `type` or it will import 26,000 bookkeeping rows as "messages" for every 12,000 real ones.

**Cursor** — `C:\Users\Sico Vibes\.cursor\projects\<slug>\agent-transcripts\<id>\<id>.jsonl`

- Line `role`s: `user`, `assistant`, `turn_ended`. Same message/content shape as Claude (`{role, message:{content:[{type:"text"|"tool_use"}]}}`), with `<timestamp>…</timestamp><user_query>…</user_query>` wrappers in user text.
- Sibling folders: `assets/` (images), `agent-tools/` (tool output dumps), `mcps/`, `terminals/`, `canvases/`.
- Cursor's own chat database is separate, at `%APPDATA%\Cursor\User\workspaceStorage\<hash>\state.vscdb`; I did not read its contents, so its schema is **unverified**.

**Gemini CLI** — `~/.gemini/tmp/<project_hash>/chats/`, with `gemini --resume <uuid>` (`https://github.com/google-gemini/gemini-cli`, `docs/cli/session-management.md`). Not installed on this machine, so this path is **documented, not observed**.

**Grok CLI** exists on this machine (`C:\Users\Sico Vibes\.grok\sessions`), but Retinue does not run Grok, so it is out of scope.

### Can an imported conversation be resumed by the CLI?

Short answer: **only by the agent that wrote it, and only on the machine that has it.** Retinue's adapters already resume all three, and each resumes through the provider's own session id:

| Agent | Retinue's resume call today | Evidence |
|---|---|---|
| Claude Code | spawn `claude … --resume <provider session id>`, then probe with a `get_context_usage` control request; a rejected resume is detected (`resume_rejected_result`) | `crates/retinue-adapter-claude/src/lib.rs:819`, `:863-1000`, `:743` |
| Codex | app-server `thread/resume` with the previous thread id | `crates/retinue-adapter-codex/src/lib.rs:655`, `:775`, `:1581` |
| OpenCode (ACP) | `session/load` when the agent advertises `loadSession`, else a new session | `crates/retinue-adapter-acp/src/lib.rs:508`, `:624`, `:636` |

Consequences, stated plainly:

- A conversation **imported from Claude Code** can be continued by Claude Code later *if Retinue never moves the file*: the id is the file name, and Claude rebuilds context from that JSONL. Retinue can simply point a bot at it (`provider_session_id` = the uuid) and the first turn will resume it.
- A conversation **imported from Codex** can be continued by Codex the same way (`thread/resume`), because the rollout file is still in `~/.codex/sessions`.
- A conversation **imported from OpenCode** can be continued by OpenCode through `session/load`, because the row is still in `opencode.db`.
- A conversation **crossed between agents** (say, a Codex conversation given to a Claude bot) **cannot** be resumed by the receiving CLI. There is no supported way to hand Claude Code a Codex transcript and get a resumable Claude session. The only honest options are: (a) show it **read-only** in Retinue, and (b) let the bot **read it as context** — the bot starts a normal new turn whose prompt carries a summary or an excerpt, exactly like a brief. Codex's own importer takes route (b) internally: it converts foreign records into *its* rollout format. Retinue should not fake a provider history by writing into `~/.claude/projects` or `opencode.db`: those are private formats, and a malformed file can break the user's real CLI.

**Recommendation: import as a Retinue-owned session that is read-only, with the provider id kept for the same-agent case.** Show "Continue in Claude Code" only when `imported.from == "claude"` and the source file still exists. For every other case the honest label is "Imported — read only", plus a "Give this to a bot" action that starts a new conversation with a summary.

### What Retinue stores, and what it must never copy

Retinue's tables already fit an imported conversation without new schema: `sessions`, `turns`, `messages`, `tool_calls`, `file_changes`, `usage_events` (`crates/retinue-storage/src/lib.rs:2569-2575`). An import writes:

- one `sessions` row per conversation: `provider` = the source agent, `provider_session_id` = the source id (kept, so same-agent resume works), `project_path` = the recorded `cwd`, `title`, `state = 'completed'`, `data.import = {source, sourcePath, sha256, importedAt, messageCount}`;
- one `turns` row per user→assistant exchange, so the transcript groups the way Retinue already renders it;
- `messages` rows for user and assistant **text**;
- `tool_calls` rows with `state = 'completed'`, a `title` and a bounded `detail`, the same normalised shape the adapters produce — not the provider's raw records.

**Never copied, per the project rules** (`.claude/skills/retinue-development/SKILL.md:20`: "No copied provider authentication, no scraping the provider's terminal, no raw provider JSON in React… Never log or print credentials, tokens, capability secrets, audio, transcripts or message content"; `AGENTS.md` repeat the same invariants and add that a handoff "never carries host paths, environment values, thinking or tool output"):

- `~/.claude/.credentials.json`, `~/.codex/auth.json`, OpenCode's `credential` table and `auth.json`, and any OAuth token or cookie found in the files. Skip the line, do not parse around it.
- `creator_user_id` / `creator_account_id` from Codex's `session_meta`, and any account, organisation or share identifiers.
- Environment values: Claude's `attachment` lines include an environment snapshot (`workingDirectory`, `platform`, `osVersion`, shell, scratchpad path); Codex's `turn_context` includes `sandbox_policy` and permission profiles. Neither belongs in a bot's transcript.
- The `world_state` / `base_instructions` blocks (whole instruction texts) — importing them would replay another project's rules.
- Base64 image data and `blob` rows: keep the *reference* (a file path inside the project, or an attachment Retinue copies into its own media folder after asking) or drop it. A 89–169 MB file is mostly images and tool output.
- `thinking` blocks and signatures: the agent's private reasoning is not transcript.
- Raw provider JSON into React (golden rule 3). The importer must normalise before the UI sees anything.

Two more practical rules: **cap what is read** (Codex's own 30 days / 50 sessions is a sane default), and **stream progress**. A scan of this machine's three stores is ~250 MB and 600+ conversations.

### Proposed design

**RPCs** (new, daemon-owned, same naming style as `session.new` in `crates/retinue-daemon/src/main.rs`):

| RPC | What it does | Who may call it |
|---|---|---|
| `import.scan` | Returns the sources found on this machine: agent, folder, conversation count, newest and oldest date, total bytes. Reads only the paths above, never credential files. | desktop only (never remote) |
| `import.preview` | For one source (or a list of conversation ids): title, project folder, agent, first user line, message count, size, last update, and a `canResumeWith` field (`claude` / `codex` / `opencode` / `null`). Paged. | desktop only |
| `import.apply` | Imports the chosen conversations, one at a time, emitting `import.progress` and `import.completed`. Idempotent by content SHA-256. | desktop only |
| `import.distribute` | Given the imported ids (or all unassigned imports), returns one suggestion per conversation with a reason code. | desktop only |
| `import.assign` | Files a conversation under a bot: sets the employee, the project, and whether it is read-only or resumable. | desktop only |

Nothing here is a `remote.*`-reachable method: importing reads the owner's filesystem, and the "Never remote" list in `AGENTS.md` already covers file-level operations.

**The flow the owner sees.** Five steps, each a screen or a sheet, no hidden work:

1. **Where from.** "We found 111 Claude Code conversations, 44 Codex conversations and 465 OpenCode conversations on this computer." Each source has a switch and a "choose folders" filter, defaulting to the newest 30 days. (A source that is absent is not shown at all; a source whose files are unreadable says why.)
2. **Which ones.** A list grouped by project folder, newest first, with checkboxes and a "select all in this folder" control. Each row shows the title, the project, the agent, the date, the size, and one honest label: **"Can continue in Claude Code"** or **"Read only"**. A warning line sits above the list: "Imported conversations stay read-only unless the agent that wrote them is still installed and still has the file."
3. **Who gets them.** Three choices, in the owner's words:
   - **One employee** — pick a bot, all selected conversations go to it.
   - **The CEO** — everything goes to the Manager/CEO conversation list, unassigned to a project.
   - **Distribute** — the daemon proposes an employee per conversation, each with its reason on screen ("same folder as *Billing*", "names the same service", "came from Claude, and *Ada* is on Claude"), and the owner can change any of them with a dropdown.
4. **Import.** A progress row per conversation, a count, and a running total. Cancel stops cleanly; what is already imported stays.
5. **Done.** "Imported 23 conversations: 19 to *Ada*, 4 to the CEO. 19 are read-only; 4 can continue in Claude Code." A link to the inbox the conversations landed in.

**How "distribute" should decide** — only from signals that exist:

- **Project folder** first. The conversation's recorded `cwd` is compared with Retinue's `projects` rows (`crates/retinue-storage/src/lib.rs:484`) by path, and a project's hired bots (`team::projects_of`, `crates/retinue-daemon/src/team.rs`) are candidates. This is the strongest signal and it is exact.
- **The agent it came from** second, as a tie-breaker: a Claude conversation is more useful to a bot already on Claude, because that is the only case where "continue" works.
- **Role match** third: keyword overlap between the conversation's title and first two user messages, and the bot's `role` and instructions. This is a suggestion with a visible reason ("mentions invoices; *Billing* handles invoices"), never a silent decision.
- **Volume balance**: cap how many conversations one bot gets per import so a single bot is not buried.
- **Always reversible**: every suggestion is a dropdown, and the reason is shown so the owner can judge it.

**Where a bot's context comes from after import.** The bot does not get the foreign transcript as provider history. Two honest options, both already supported by Retinue's shapes: (a) the conversation is pinned in the bot's list and the bot is told "read conversation X" through the MCP `list_conversations`/read path (`crates/retinue-daemon/src/external.rs:393-403`), or (b) the first turn's prompt carries a bounded summary the daemon generated with the conversation's own text. Both keep the rule that a bot never inherits another tool's hidden state.

**Risks worth stating in the plan.** (1) Reading `opencode.db` while OpenCode is running: open it read-only and copy it first, or a WAL write can fail; Retinue already opens SQLite with a busy timeout elsewhere but a foreign DB deserves a copy. (2) A 169 MB rollout can take seconds to parse; stream and cap. (3) Claude's `file-history-snapshot` lines contain file *contents* of the project; skip them entirely. (4) Retinue must not write to any provider's store.

### Unverified in this section

- capy.ai's importer internals (closed source).
- Cursor's `state.vscdb` schema.
- Gemini CLI's on-disk JSON shape (path documented, not observed).
- Whether an older OpenCode writes `storage/session/**` JSON files as well; this machine's OpenCode 2 keeps everything in `opencode.db`.

---

## 3. Coloured, highlighted replies (OpenCode's look)

### The answer to the question: it is the terminal renderer, not the model

The model emits ordinary GitHub-flavoured Markdown. OpenCode's TUI colours it by mapping each Markdown token to a colour from the active theme. There is no colour markup in the text.

**The token list is explicit.** `packages/theme/src/tui/schema.ts` defines:

```ts
export const MarkdownToken = Schema.Literals([
  "text", "heading", "link", "linkText", "code", "blockQuote", "emphasis", "strong",
  "horizontalRule", "listItem", "listEnumeration", "image", "imageText", "codeBlock",
])
export const MarkdownDefinition = Schema.Record(MarkdownToken, Schema.optionalKey(HueColorValue))
```

Those 14 tokens are the whole vocabulary. Every theme supplies them, and `packages/tui/src/theme/component.ts` exposes the resolved set as `theme.markdown` for the TUI's Markdown component (which comes from OpenTUI, `@opentui/core`; OpenCode's `packages/tui/src/plugin/markdown.ts` only swaps *code block* renderers per language).

**The colours, in the shipped default theme** (`packages/tui/src/theme/assets/v2/opencode.json`, `base.markdown`):

```json
"markdown": {
  "text": "$hue.neutral.200",
  "heading": "$hue.accent.200",
  "link": "$hue.interactive.200",
  "linkText": "$hue.cyan.200",
  "code": "$hue.green.200",
  "blockQuote": "#e5c07b",
  "emphasis": "#e5c07b",
  "strong": "#f5a742",
  "horizontalRule": "$hue.neutral.400",
  "listItem": "$hue.interactive.200",
  "listEnumeration": "$hue.cyan.200",
  "image": "$hue.interactive.200",
  "imageText": "$hue.cyan.200",
  "codeBlock": "$hue.neutral.200"
}
```

The earlier (v1) theme file resolves the same idea to concrete dark-mode hexes (`packages/tui/src/theme/assets/opencode.json`): `markdownHeading` `#9d7cd8` (purple), `markdownLink` `#fab283` (peach), `markdownLinkText` `#56b6c2` (cyan), `markdownCode` `#7fd88f` (green), `markdownEmph` `#e5c07b` (yellow), `markdownStrong` `#f5a742` (orange), `markdownHorizontalRule` `#808080`, `markdownListItem` `#fab283`, `markdownListEnumeration` `#56b6c2`, `markdownCodeBlock` `#eeeeee`.

So the "DeepSeek reply" the owner sees is: purple headings, green inline code, orange bold, yellow italic and quotes, peach and cyan links and list markers. All of it is applied *to standard Markdown*.

**What the system prompt says about formatting.** In `packages/core/src/plugin/system-prompt/gpt.txt`:

> `# Harness`
> `- Responses are rendered as GitHub-flavored Markdown.`
> …
> `## Final Answer`
> `In the final answer, lead with the outcome, not the steps you took to reach it. Cover the most important information, use only as much structure as the answer needs, and avoid long-winded explanations unless necessary. Include technical detail only where it helps.`

and the shorter `packages/core/src/session/runner/prompt/system.txt` carries the same first line. There is **no instruction to add colour, and no colour syntax OpenCode would understand**. The only formatting instruction is "GitHub-flavoured Markdown" plus "use only as much structure as the answer needs". That last line is the lesson: the colour is legible because the *structure* is restrained. The model is model-agnostically nudged towards headings, bold and inline code by its own habits, and the theme makes them visible.

### Where Retinue stands today

Retinue has its own Markdown renderer (`apps/desktop/src/ui/SafeMarkdown.tsx`), not a third-party one, and it already does some of this:

- It tokenises inline `code`, `**strong**`, `*em*`, links, `:lucide-…:` icons, and **`==mark==`** — the highlight syntax is already implemented (`SafeMarkdown.tsx:11`, `:23`, `:30`).
- It colours only the bot's (`agent`) rows: headings h2–h4 take `--md-heading` (dark `#f0a8c8`, light `#a12d68`), inline code takes `--md-code` (dark `#8fd6a4`, light `#1f7a3f`), strong takes `--md-strong`, and `mark` gets `--md-mark-bg` yellow with dark ink (`apps/desktop/src/ui/shell.css:677-682`).
- Links, emphasis and list markers are **not** coloured (`shell.css:209-230`), and a blockquote is already muted with a left rule (`:223`).

So the owner is asking for the last 20% of a feature that is 60% done. The `==highlight==` marker the owner proposes **already exists**, which is the best possible answer: no renderer work is needed for it, only a bot-side instruction to use it.

### Recommendation: which elements to colour

Extend the existing `--md-*` set; do not add a palette of arbitrary colours, and keep the rule that colour signals meaning, not decoration (`.claude/skills/retinue-development/references/design-language.md:7`: "Status colour only when something is happening"), and that text keeps text tokens except for these deliberate element roles (the chart rule at `design-language.md:164` says "text uses text tokens").

| Element | Today | Recommendation | Why |
|---|---|---|---|
| Heading (`##`, `###`) | coloured (pink) | keep | The single strongest navigational colour; already proves the idea in the product. |
| Inline code | coloured (green) | keep | Recovers the "this is a literal" reading at a glance: paths, commands, identifiers. |
| `**strong**` | coloured (white in dark) | keep, but make it an emphasis colour rather than "whiter white" in light mode (light mode `#000000` is invisible against text) | The one bug I would fix: in light mode `--md-strong: #000000` is the same as body text, so bold has no colour and only weight. A brand-neutral emphasis tone would fix it. |
| `==mark==` | yellow background, dark ink | keep, and teach bots to use it | Covers the owner's "the one or two most important points". A background is louder than coloured text, which is why it must stay rare. |
| Link text | plain | colour with the look's accent or a new `--md-link` | A wall of plain-coloured links is the one thing OpenCode's look does that Retinue does not. |
| List markers (`-`, `1.`) | plain | colour the bullet or number only, faintly | Cheap, and it makes a long list scannable without shouting. |
| `*emphasis*` | plain italic | leave as is | Emphasis is common; colouring it makes every sentence busy. Italic alone is enough. |
| Blockquote | muted text + rule | leave as is | Already reads as a quotation. |
| Horizontal rule | plain line | leave as is | Structural, not semantic. |

Two constraints worth writing down: the tokens must live in `theme.css`/the look stylesheets like the rest (`design-language.md:137`: "Component stylesheets use tokens, not literals"), and `mark` must stay inside the bot's rows only, as it is now, so user text is never highlighted.

### A formatting guide for bots

There is exactly one place for this: `crates/retinue-daemon/src/team.rs`, next to the chart/table and OpenUI instruction blocks that go into every bot's instructions (`team.rs:508-509`). A new `FORMATTING_INSTRUCTIONS` constant joining that `push` list would reach every bot, the CEO included, with no change to any adapter.

Proposed text (short, because it is added to every prompt):

> **Formatting.** Write GitHub-flavoured Markdown, and use structure only where it helps. A one-paragraph answer gets no heading. Use `##` for three or more sections or steps, never for a single section, and never `#`. Bold the outcome sentence and the few terms that matter (`**…**`). Put paths, commands, identifiers and short code in backticks. Use a list for three or more parallel items, and prose otherwise. Wrap at most one or two sentences in `==…==`: the thing the reader must not miss, never a whole paragraph, never a heading. No tables under three rows. No coloured emoji as decoration.

Why each line: headings only at three or more sections mirrors OpenCode's "use only as much structure as the answer needs" (`gpt.txt`, Final Answer) and stops the "one heading per paragraph" habit; bold for the outcome matches "lead with the outcome"; `==` is capped because a highlight loses meaning when repeated; backticks are already the product's mechanism for copyable literals.

### What I could not verify

- Which prompt file DeepSeek gets in OpenCode (there are per-model prompt files: `gpt.txt`, `anthropic.txt`, `kimi.txt`, `meta.txt`, `trinity.txt`, `gpt-astra.txt`), so the exact wording a given model sees may differ. The GFM line is in the shared prompt either way.
- Whether OpenTUI's Markdown component applies token colours exactly as the theme names them (inferred from the token names and theme file; I did not run the TUI).

---

## 4. Browser use and computer use, including the owner's own computer

The owner has decided: bots get (a) a browser, (b) the cloud computer with mouse, keyboard and screenshots, and (c) the owner's own Windows computer **only after the owner turns it on or consents, off by default**. This section designs it. It builds on `docs/research/browser-computer-use-and-ag-ui-2026-11.md`, and where that document argued against desktop control, this one takes the decision as given and spends the effort on the safety rails.

### 4.1 What exists today, precisely

- **The cloud computer has no input or screenshot tools, only a command line.** `crates/retinue-daemon/src/computer.rs` is an MCP server named `vps-computer` (`:49`) that proxies one linked Harbor server (`/harbor/api/computer`, `:51`). Its ten tools are `computer_status`, `computer_wake`, `computer_sleep`, `computer_restart`, `computer_logs`, `computer_exec`, `computer_list_routines`, `computer_create_routine`, `computer_run_routine`, `computer_desktop_url` (`computer.rs:791-800`). There is no click, no type, no screenshot.
- **`computer_exec` is already a full escape hatch.** It runs a shell command inside the container as the desktop user, in `/config`. So **a bot can already drive X11 today** by installing and calling `xdotool` and a screenshot tool, if the image has them. Nothing in this design has to wait for Harbor to add endpoints.
- **There is already a human view of the cloud desktop.** `computer.view` mints a 256-bit one-time ticket and returns a loopback URL the window may frame (`computer.rs:727-766`, `:1082-1096`; tickets: 64 hex, 10-minute TTL, at most 16, never logged, `:80-163`). `docs/ipc-contract.md:779-788` documents it. That is the "watch it happen" surface for the cloud computer, already built.
- **The safety gates that already exist and must be extended, not replaced:** the computer must be linked **and switched on when the call arrives** (`computer.rs:839-849`); command tools are refused when nobody is watching the conversation (`computer.rs:847-849`, `nobody_is_watching` at `:829-834`); and **every `computer.*` method is `tiers.json` class `never`** with the event class `drop` (`docs/ipc-contract.md:767`).
- **The browser side has nothing.** No browser is mounted into any session; what Claude Code, Codex and OpenCode give a bot is their own web search and fetch.

### 4.2 Cloud computer: screenshots, mouse, keyboard

**Design: three new MCP tools that compose the existing `computer_exec`, plus one that returns an image.** No new Harbor endpoint is required, which means this ships without waiting on another team.

| Tool | What it does | Rules |
|---|---|---|
| `computer_screenshot {maxWidth?, region?}` | Runs a screenshot tool inside the container, downscales to at most 1280 px wide, and returns the PNG as **one Retinue media attachment** (the `generated_media` table, `crates/retinue-storage/src/phase2.rs:16`), not as inline base64 in the tool row. | One image per call. Never a stream or a recording. The tool row says `Caption: screenshot of the bot computer` and carries the media id; the bytes go through the same path as a generated image (`crates/retinue-daemon/src/media.rs`), so a phone never receives it (media meta carries no path, `media.rs:602-634`). |
| `computer_click {x, y, button?}` / `computer_move {x, y}` | `xdotool mousemove x y click n`. | Coordinates are clamped to the desktop's real size, read from `xdpyinfo` once per call. Refused while the computer is asleep. |
| `computer_type {text}` / `computer_key {key}` / `computer_scroll {direction, amount}` | `xdotool type --clearmodifiers --delay`, `xdotool key`, `xdotool click 4/5`. | `text` is capped (say 2,000 characters), recorded **as a length, never as the text**, in the tool row and in any log. |
| `computer_screen_size {}` | Optional: `xdpyinfo` size and the current pointer position. | Lets a model aim without a screenshot. |

**What has to be installed in the container.** `xdotool` (input), and one capture tool: `gnome-screenshot`/`import` from ImageMagick, or `scrot`. If the image lacks them, `computer_exec` can install them (`apt-get install -y xdotool imagemagick`), but that is a change to the user's computer and should be a one-line, visible setup step ("Your computer needs two small tools for screenshots and clicks. Install them? [Install]"). **Unverified:** what the current Harbor image contains; the view endpoint proves an X desktop exists, but not which utilities.

**Caveats that shape the tool contract.** A pixel click cannot be classified the way a file edit can, so these tools must not inherit `accept_edits`. They belong to a **new permission category `COMPUTER_INPUT`** handled in the same classifier as `EDIT`/`READ` (`crates/retinue-daemon/src/main.rs`, the permission classifier around `:5196`), denied under `ask` until approved with "allow for this conversation". And `COMMAND_TOOLS` (`computer.rs:103`) should grow to include every input tool, so unattended runs never drive a mouse.

**Why not coordinates first?** A screenshot plus a coordinate click is the expensive, blind version of the same task. Where the target is a browser, prefer the accessibility route in 4.3, and reserve pixels for things no tree describes (a game, a canvas, a native dialog with no accessible names).

### 4.3 Browser use

Two mechanisms, one shared contract.

1. **In the cloud computer: Playwright inside the container.** Install once (`npm i -g playwright && playwright install --with-deps chromium`), then a supervised local process exposing a small HTTP API on `127.0.0.1:<port>` inside the container, which the bot drives through `computer_exec` today. The pattern worth copying is OpenBot's `agent-computer` (MIT), already described in the earlier report (`docs/research/browser-computer-use-and-ag-ui-2026-11.md:51-67`): one long-lived Chromium per bot; screenshots via Playwright; **actions addressed by an element reference from an accessibility snapshot**, not by pixels; an explicit "stale ref" refusal; a cap on page text and element count.
2. **On the owner's machine: a browse-style driver** (`https://github.com/anomalyco/browse`, MIT, TypeScript: "an MCP server and a Chromium extension" that drives tabs with Playwright, screenshots, captures network traffic, and hands a tab back to the person for a login, 2FA or payment). Its **hand-back pattern** is the single best idea in this space and should be copied even for the container: when a page reaches a sign-in, a payment or a 2FA prompt, the bot **pauses**, the tab is shown to the owner, the owner acts, and the bot continues on an explicit "Continue".

**Recommendation for v1:** give bots a **Retinue-owned browser profile**, never the owner's Chrome profile — a separate `--user-data-dir` under the app's data folder, with no imported cookies. Reasons are the ones already recorded: a stolen profile is account takeover, and the owner's signed-in tabs are the highest-value thing on the machine. The owner's real browser becomes an **opt-in second mode** ("Use my browser for this conversation"), with a visible tab group, and every sensitive step handed back.

**What the bot gets** (keep it to seven verbs): `browser_open`, `browser_read` (text + element list), `browser_click`, `browser_type`, `browser_press`, `browser_screenshot`, `browser_stop`. Refusals that need no policy engine: no `file:`, `javascript:`, `data:` navigation; no non-loopback private ranges; no cloud metadata addresses; a hard cap on page text; never a download without a click the owner approves.

**Ordering.** Do 4.2 first (a day or two of daemon work over an API that already exists), then 4.3, because 4.2 produces the screenshot plumbing and the permission category that 4.3 reuses.

### 4.4 The owner's Windows computer

Three real options. All three need the same rails, so choose on cost and control.

| Option | What it is | Licence | Needs | Judgement |
|---|---|---|---|---|
| **A. Retinue's own native helper** | Rust in `apps/desktop/src-tauri`: `SendInput` for mouse and keyboard, `Windows.Graphics.Capture` or `BitBlt`/`PrintWindow` for frames, UI Automation (`IUIAutomation`) for the element tree | ours | The most native code Retinue has written; new COM work; a new always-on safety surface; every failure mode is ours | Best control and best fit, **largest**; the right long-term home |
| **B. Bundle `open-computer-use`'s Windows runtime** | `anomalyco/computer-use`, which is a mirror of `iFurySt/open-codex-computer-use`: an MIT-licensed computer-use MCP server. Windows runtime is `apps/OpenComputerUseWindows` — a Go binary (`main.go`) that embeds a PowerShell UI Automation bridge (`runtime.ps1`), shipped as `open-computer-use.exe` under the npm package's `dist/windows/<arch>/`. Nine tools: `list_apps`, `get_app_state`, `click`, `perform_secondary_action`, `scroll`, `drag`, `type_text`, `press_key`, `set_value` | **MIT** (`LICENSE`: "MIT License, Copyright (c) 2026 Leo") | Node/npm or the bundled `.exe`; accessibility permissions are only a macOS requirement ("Windows and Linux do not need this step", README) | **Recommended for v1.** Element-index addressing from an accessibility tree means most work needs **no screenshot at all**, which is the cheapest and safest possible start. Cost: a third-party binary in the trust path, and a PowerShell bridge to audit |
| **C. Point the bot at a third-party CLI's own computer use** | whatever Claude Code, Codex or OpenCode ships | — | — | Not available: none of the three exposes desktop control to the daemon |

**Why B is the right first step, in one paragraph.** Its default answer to "what is on screen" is a **text tree** (`get_app_state` returns an app's accessibility tree, 1,200 nodes and 64 levels by default, text capped at 500 characters), and its actions address an element index from that tree, re-read before each action (`skills/open-computer-use/references/usage.md`). Text is small, and text can be redacted and reviewed in a way a screenshot cannot. That gives the owner a working feature while the screenshot question is still a permission decision rather than a hard-coded assumption. Option A can replace B later behind the same Retinue tool names, so nothing in the product or the prompts changes.

**What Retinue must own, whichever option is used.** Retinue must not hand the raw server to the agents. It runs the server as its own child process, and exposes **its own MCP tools** (`owner_screen_state`, `owner_click`, `owner_type`, `owner_key`, `owner_scroll`, `owner_screenshot`, `owner_stop`) that gate every call, so the consent, the indicator, the log and the tier rules live in Retinue's daemon and not in a third-party binary's configuration.

### 4.5 The consent model

Six layers, each one cheap and each one independently sufficient to stop the worst case.

1. **A Settings switch, off by default, with the capability named plainly.** `experimental.ownerComputer` (settings keys of that shape already exist: `experimental.usageBar`, `experimental.runtimeLogos`, `experimental.activityDetails` in the preview bridge, `apps/desktop/src/preview/fixtureBridge.ts:539-542`). Copy: "Let bots control this computer. Off: no bot can see or use your screen or keyboard. When on, a conversation still has to ask you first." Turning it off releases any running grant immediately.
2. **Per-conversation consent, as an approval card, not a setting.** The first `owner_*` call puts a card in the conversation: *"Ada wants to control this computer: read the screen of Chrome and click. Allow for this conversation / Once / No."* Grant is stored in memory per session and dies with the session. Never a permanent "always allow" rule — the rule store must refuse this category explicitly.
3. **A visible indicator with a stop button, on screen the whole time.** A persistent bar at the top of the conversation ("Ada is controlling this computer — Stop") **and** a floating pill in the companion, which is already always-on-top. The pill is the emergency brake for when the bot has moved a window over the conversation. Two ways to stop, both immediate: the button, and a global hotkey (Ctrl+Alt+. or three quick Escapes) registered while a grant is live, never otherwise. Stop also cancels the turn (`session.cancel` exists today).
4. **A time limit, in three parts.** A grant lasts at most **5 minutes** and is renewable by the bot asking again (not silently); a turn has an absolute cap (say 15 minutes) after which the grant is dead; and 60 seconds with no action ends the grant. On expiry the indicator says so and the conversation gets an ordinary "the computer session ended" line.
5. **Tiers: never remote, never unattended.** Keep the existing rule that every computer method is `never` (`tiers.json`, `docs/ipc-contract.md:767`) and add rows for every new method and event with `never`/`drop` until a streaming design exists. Refuse when nobody is watching, reusing `nobody_is_watching` (`computer.rs:829`), and refuse for a teammate's reply-woken turn. Refuse while Full access is on as well: Full access removes approval prompts, and this is precisely the capability that must keep asking.
6. **A kill switch that is not in the UI.** On lock (`Win+L`-style session lock), on sleep, and when the app loses focus for more than N minutes with a grant live, the grant ends. Retinue can observe these through the shell that already owns the window.

### 4.6 Logging, and keeping secrets out

- **What is logged:** one content-free activity row per action class — "read the screen of *Chrome*", "clicked in *Chrome*", "typed 12 characters", "pressed Enter" — with the app name, the time, the conversation and the bot. That is the shape `remote_activity` already has (`crates/retinue-storage/src/lib.rs:850`). Nothing else.
- **Never logged, never stored:** screenshots (they live as media attachments with the same lifetime as a generated image, and are never in a log, a diagnostics bundle, a backup or a push payload); typed text (store the length only); the accessibility tree (use it in the turn, do not persist it — it can contain a whole mail body); window titles that look like paths or URLs.
- **Secrets typed on screen.** Four controls, strongest first:
  1. **Prefer non-pixel reads with redaction.** UI Automation marks password fields (`IsPassword`); when a password field is the element being read or captured, the element returns `••••` and the screenshot of that region is masked.
  2. **Screenshots are opt-in per frame.** Default mode sends the text tree only. When the bot needs a picture, the frame is shown **to the owner first** with "Send this frame / Retake", and only an approved frame goes to the provider. This is the same one-use grant pattern as the one-use secret requests in `crates/retinue-daemon/src/secrets.rs` (`:345-407`).
  3. **A URL and app deny-list while a grant is live.** Refuse to read or capture a window whose title or URL matches a bank, a password manager, a mail app or an SSH terminal, unless the owner overrides for that frame. Ship a starter list, make it editable, and show the refusal in the conversation so the owner learns why.
  4. **The transcript shows actions, not outcomes.** A bot's tool row for a typed secret reads "typed 24 characters"; the model's own reason for typing is in the bot's reply, which the owner already sees.
- **One more rule worth writing into the amendment:** a screenshot the bot reads is *prompt content*, and the provider sees whatever is in it. So the product must never take one automatically, on a timer, or "just in case". That is not a technicality; it is the difference between a feature and a leak.

### 4.7 Draft `AGENTS.md` amendment

For the owner to approve. Wording matches the existing amendments' style: what is allowed, what is never, and what a release must check.

> # Amendment (14 November 2026): browsers and computer control
>
> Owner decision of 14 November 2026. Bots may use a browser, drive the linked cloud computer, and — only after the owner turns it on — control the owner's own Windows computer. This amends the "never remote", "no copied provider authentication" and loopback rules for this path only, and adds one egress path that the threat model must now carry.
>
> - **Cloud computer input is a tool, not a shell.** Screenshots, mouse, keyboard and scrolling are `computer_*` MCP tools executed inside the linked computer's container. They run only while the computer is linked and switched on at the moment of the call, and never in an unattended run, an automation, or a turn woken by a teammate's reply.
> - **A screenshot is a media attachment, never inline bytes and never a log line.** It follows the generated-image path: `generated_media`, one image per call, the tool row carries an id and a caption, and the meta carries no path.
> - **Bots browse in a Retinue profile.** The default browser is Retinue's own Chromium profile under the app's data folder, with no imported cookies or extensions. Using the owner's signed-in browser is a per-conversation opt-in, never a default, and every sign-in, 2FA, payment or post pauses for the owner.
> - **The owner's computer is off until three things happen:** the Settings switch `experimental.ownerComputer` is on (off by default, and off after an update that changes the feature's shape), the owner approves a card in that conversation, and the conversation is attended. Turning the switch off releases every live grant at once.
> - **A live grant is visible and stoppable.** A bar in the conversation and a floating pill in the companion both say which bot is controlling the computer; either stops it at once, as does a global hotkey, the app losing the foreground for too long, or the machine locking. A grant lasts at most 5 minutes, a turn at most 15, and 60 seconds of no action ends it.
> - **Screens are sent only when the owner sends them.** The default is the accessibility tree with password fields masked. A picture of the screen goes to a provider only after the owner sees that exact frame and approves it. Never on a timer, never on a loop, never for every step.
> - **What is recorded is content-free:** the app name, the action class and a count ("typed 12 characters"). Never a screenshot in a log, a backup, a diagnostics bundle or a push payload; never the typed text; never the accessibility tree.
> - **Never remote, never unattended, never Full access.** Every `owner_*` and new `computer_*` method stays `tiers.json` class `never` and its events `drop` until a streaming design exists. A remote tier, an automation or a `bypass` conversation never gets them. Full access does not cover them, because this is the capability that must keep asking.
> - **A third-party driver is a supervised child.** If Retinue runs an open-source computer-use server, it runs it as its own child process behind Retinue's own tools, so the consent, the indicator, the log and the tier rules stay in the daemon. The licence and the pinned version of that server are recorded in `docs/operations/`.
> - **Release gate.** `docs/remote/THREAT_MODEL.md` gains a row for "the model receives a picture of the owner's screen" and one for "a bot can act as the owner on a signed-in site", and a release that changes these defaults needs the owner's live check before it ships.

### 4.8 Phased plan

**Phase A — the cloud computer moves (small, ships first).** Backend (Ada): the four input tools and the screenshot tool over `computer_exec`; `COMPUTER_INPUT` permission category and classifier arm; input tools added to `COMMAND_TOOLS`; screenshot through `media.rs` reusing `generated_media`; `xdotool`/capture-tool detection with a one-line setup card; tests that a call is refused when the computer is off, unattended, or the coordinates are out of bounds. UI: an activity row per action ("Clicked in the bot's screen"), the screenshot rendered as a normal image card, the setup card, and the existing `computer.view` frame offered beside it ("Watch").

**Phase B — browsers (medium).** Backend: a supervised browser process for the cloud computer (Playwright, one Chromium per conversation, element-ref actions from an accessibility snapshot, text and element caps); a `BROWSER` permission category; the seven `browser_*` tools; refusals for `file:`/`javascript:`/`data:` and private ranges. UI: a live "Browsing: *site*" step line drawn from the tool rows (the existing `doingNow.ts` pattern), a thumbnail that can be opened full-size, and Settings › Approvals copy for the new category.

**Phase C — the owner's computer (large, and the only one that touches the trust model).** Backend: supervise the chosen driver as a child process; the `owner_*` tools and their gate; grants in memory with the 5/15/60-second limits; the deny-list; the password-field and frame-approval flow; content-free activity rows; `tiers.json` rows as `never`. UI: the Settings switch with its warning copy; the consent card; the conversation bar; the companion pill with Stop; the global hotkey; the frame approval card ("Send this frame / Retake / Never for this app"); a Settings › Computer page that lists what is allowed and shows the last ten action rows.

**What I would build first inside Phase C, if it has to be split:** the accessibility-tree path only, with screenshots refused outright at first. That ships something genuinely useful (read a form, fill it, click a button) with no picture of the owner's screen leaving the machine, and it leaves the screenshot decision as a separate, explicit switch the owner can turn on later.

### Unverified in this section

- The cloud image's installed tools (`xdotool`, a screenshot utility, Playwright) — the view endpoint proves a desktop exists, but not what is in the image.
- Whether `open-computer-use`'s **Windows** runtime supports screenshots at all, or only the accessibility tree: its docs promise "the same 9 tools" on every platform, but the Windows runtime is described as experimental and no Windows screenshot tool is documented. **Assume text-only on Windows until proven otherwise**, and design Phase C so that is enough.
- Whether the container's Playwright can run headed against the existing X display, and the display number.
- The exact latency of a screenshot round trip through Harbor (matters for a 60-second idle limit).

---

## Proposed skill updates (not applied: this run changed no code and no docs outside this file)

Four things here that a future agent would otherwise re-learn, for the Director to put into `.claude/skills/retinue-development/`:

1. **`references/lessons.md`**: "OpenCode does generate images, and Retinue already has a pending card for it." The adapter reads `experimental_image_generate` over ACP (`crates/retinue-adapter-acp/src/lib.rs:1104`) and the stem produces a `pending` media row; the Codex path has no pending card because `ImageRequested` only arrives at completion. Do not "fix" a slow image by shortening `TURN_SILENCE_LIMIT`: an open tool row already multiplies it by 6 (`main.rs:5086`, `:5158`).
2. **`references/codebase-map.md`**: to import other agents' histories, the sources are `~/.claude/projects/<slug>/<uuid>.jsonl`, `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, `~/.local/share/opencode/opencode.db` (SQLite, tables `session_v2` and `session_message`), and `~/.cursor/projects/<slug>/agent-transcripts/<id>/<id>.jsonl`. OpenCode is not a folder of JSON files any more.
3. **`references/design-language.md`**: bot replies colour headings, inline code, strong and `==mark==` through `--md-*` (`apps/desktop/src/ui/shell.css:677-682`); the renderer already parses `==…==` (`SafeMarkdown.tsx:23`). Any further colouring goes through the same token block, not literals.
4. **`references/codebase-map.md`**: the computer MCP is a Harbor proxy with no input tools; the only desktop interaction today is `computer_exec` plus the ticketed `computer.view` frame.


