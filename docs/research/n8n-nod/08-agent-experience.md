# nod's agent and user experience, against Retinue's

> **What this is:** research for the owner. One of three reports written at the same time about `n8n-io/nod` (after the earlier five in this folder). This one covers the agent loop, planning, memory, skills and instructions, context compaction, sub-agents and parallel work, how progress and plans are shown, onboarding, settings and notable UI patterns.
> **Status:** research only. No Retinue code was changed, nothing was committed.
> **Source:** `https://github.com/n8n-io/nod`, shallow clone at commit `47cfee3fe77279e1d543a367131d3ff2c1202e18` (16 November 2026, `main`), at `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-sandbox`. Read only: no install, build or binary was run.
> **Owner:** Vex. **Author:** Grey (research lane). **Date:** 16 November 2026.

**Licence.** The repository root is Apache License 2.0 ([LICENSE](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/LICENSE#L1-L3)), so a copy with attribution would be legal. `NOTICE` flags one GPL-3.0-or-later file, the speech phonemiser, which is not in this area. The brief says not to copy, and nothing here needs their code: every idea below is a **design** we would write in Rust and TypeScript. No file was copied.

---

## Summary

1. nod owns its whole agent harness (Pi), so its agent experience is policy it controls: one conversation, one session per task, a live status line the **model writes**, memory with sources and an automatic recall pass, skills the agent writes for itself, and compaction it schedules.
2. Retinue rents the harness in each coding-agent CLI, so the same gains must come from three places we do control: what the daemon injects before a turn, what our own MCP server offers the bot, and what the UI can infer or show.
3. Their best ideas for us, in order: **learned skills** the bot writes and the owner reviews; a **"now" line the bot writes**; **automatic memory recall** before a turn; a **plan that outlives one turn**; **first-run suggestions grounded in the project**; a **docs search tool** for product questions.
4. Almost all of them are small or medium, because we already have the plumbing: per-turn suffix blocks with hashes, a per-session tool catalog, memory FTS, a skills pool, and a plan carried on the turn row.
5. The one large idea is the learned-skill write path. It needs a review queue and the existing memory write rails, because a skill is an instruction other bots will follow.

## 1. How nod does it

### The shape in one line

One thread per owner. The chat is a session. Each substantial request becomes a saved **task** with its own session. Every run is a **turn**, one at a time per session, sessions in parallel. Every tool call passes one guard, and outside results reach the model as data ([threads-and-turns.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/threads-and-turns.md#L8-L10)).

### The agent loop

| Piece | Where | What it does |
| --- | --- | --- |
| One turn | [`src/runtime/index.ts:207`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L207) | `runTurn` opens (or reuses) a Pi session, binds tools, streams events, records usage, and ends `succeeded`, `waiting`, `failed` or `cancelled` |
| Tool binding and the guard | [`src/runtime/index.ts:720-808`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L720-L808) | Each tool is wrapped: argument validation with a readable message, the turn guard, and an "admission" gate |
| Parallel and ordered tools | [`src/runtime/index.ts:88-105`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L88-L105) | Reads and helpers run in parallel; any call that carries a trust check or may ask the owner also passes an ordered admission gate, so at most one approval question is open in a turn |
| Repeat-failure stop | [`src/runtime/index.ts:60-72`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L60-L72) | Five matching failures for one tool end the run before another model request |
| Untrusted wrapping | [agent-runtime.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/agent-runtime.md#L222-L224) | Web, app, browser and script results arrive in one `untrusted_tool_result` block with a data-not-instructions notice; workspace tool results do not |

The ordering rule is the interesting part. `admission()` ([`src/runtime/index.ts:702-719`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L702-L719)) chains each asking call behind the ones before it: a later call that would act is refused while an earlier one waits for the owner. That is what stops a parallel burst from producing three approval cards at once.

### The prompt: three layers, cache-aware

| Layer | Contains | Sent |
| --- | --- | --- |
| Static prompt | Shared behaviour, security and style rules, guide **names**, tool guidelines | Once per session |
| Owner context | Account details, date format, the memory index and the learned-skill index | Again only when it changes |
| Turn message | Mode, time, channel, message id, reply target, recent conversation, the instruction | Every turn |

Code: [`src/runtime/prompt.ts:34-64`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/prompt.ts#L34-L64) (`systemPrompt`), [`src/runtime/prompt.ts:153-164`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/prompt.ts#L153-L164) (`ownerContext`). The owner context is a **message of its own**, never part of the system prompt, so a memory change does not break the cached prefix; compaction leaves it out and the next request gets a fresh copy ([agent-runtime.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/agent-runtime.md#L226-L258)). A separate short system prompt exists for small context windows ([`src/runtime/prompt.ts:66-73`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/prompt.ts#L66-L73)).

Consequence worth copying: **instructions never change because memory changed**, so the provider thread is never restarted for a note, a skill edit or a plan update. Retinue made the same call for its per-turn suffixes (section 2).

### Compaction and the context budget

All of it is one policy per model in [`src/providers/pi/context.ts`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/pi/context.ts#L1-L33): window, output cap, safety margin, compaction reserve and keep-recent tokens, and the tool-output byte and token budget. Above 65,536 tokens the window compacts at 256k or 16k below the window (whichever is smaller) and keeps the newest third; below that it compacts at half the window and keeps the newest eighth ([agent-runtime.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/agent-runtime.md#L99-L105)).

Two mechanisms are worth naming separately because they are the ones that keep a long turn alive:

- **Tool results are bounded before the model sees them.** A result over budget is written under `.nod/outputs/` in the workspace and replaced by a preview plus a file path ([`src/providers/pi/index.ts:549-587`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/pi/index.ts#L549-L587)).
- **Oversized messages are offloaded, largest first.** Before a request, `fit()` replaces the largest text blocks with a workspace file reference and a bounded head and tail, leaving tool calls and thinking intact ([`src/providers/pi/context-output.ts:108-161`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/pi/context-output.ts#L108-L161)).

Summaries run on the session's model at its **lowest supported effort**, with a quarter-window output cap, and an oversized or partial summary becomes a file reference rather than a checkpoint; a failed request retries only the model call, never a completed tool ([`src/providers/pi/index.ts:779-830`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/pi/index.ts#L779-L830), [agent-runtime.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/agent-runtime.md#L124-L138)). When the reported use hits the safety margin they say "full context" and ask for a new conversation instead of silently continuing.

### Memory: one document, sources, and a recall pass

Memory is one JSON document per owner, `memory/MEMORY.md`, up to 256 KiB, outside the workspace, in five areas: `about`, `preferences`, `people`, `routines`, `lessons` ([memory.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/memory.md#L12-L26)).

Every entry has `area/slug`, text, an optional one-line abstract, **source message ids**, and an optional verbatim **support quote**; entries are `tentative` (mentioned once), `observed` (twice or more, or from email style) or `stated` (owner-saved). Removal is normally **archival with a reason** (superseded, merged, duplicate, expired, stale, obsolete, contradicted, manual); an explicit forget deletes the entry and its archived versions ([`src/server/memory-tree.ts:89-120`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/memory-tree.ts#L89-L120), `:174-179` statuses, `:311-329` index layout).

Reading has three levels: the **L0 index** in the owner context lists every entry in full, one line as `path: text`, confirmed first, tentative marked; `memory_read area` lists an area; `memory_read area/slug` returns the entry with status, evidence count and quote ([memory.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/memory.md#L30-L48)). Word search is `memory_find`, no model, no embeddings ([`src/server/memory-tree.ts:342-364`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/memory-tree.ts#L342-L364)).

Before an owner-started turn, a **cheap model selects notes and expands search terms**, then a second call ranks the candidate messages; both share a **one-second deadline**, and falling back to word matches is the designed behaviour, not an error ([`src/server/memory-recall.ts:68-195`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/memory-recall.ts#L68-L195), deadline at `:95`). The packet carries facts, messages with dates and a `searches` list with `complete` and `nextCursor`, and is bounded by budget. The model may select only from supplied paths and ids; it cannot introduce a fact.

Writing is deliberately narrow: the model **cannot write a note**. It calls `memory_learn` with owner message ids; a memory model writes candidate notes **from the owner's own messages only**; the server keeps a note only if a cited owner message contains its quote and it holds no card, key or login link ([memory.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/memory.md#L89-L115)). An hourly job repairs (expired, duplicate, stale) and reflects (reads owner messages 40 at a time and proposes adds), with compare-and-swap on the document version ([memory.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/memory.md#L132-L166)). Sensitive topics are dropped unless the owner turned them on.

### Skills: a guide library, and skills the agent writes

Two different things share one tool.

**Task guides** are 27 hand-written procedures in `src/runtime/skills`, loaded by progressive disclosure: names in the prompt, `SKILL.md` on demand, a `references/` file only when the guide says its condition applies ([skills.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/skills.md#L12-L29), core four named at [`src/runtime/prompt.ts:18`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/prompt.ts#L18)). Reads stay inside the guide's own folder; a guide grants no tool, account or permission ([skills.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/skills.md#L79-L91)).

**Learned skills** are procedures the agent writes for itself after a task: the steps, the gotchas, what to verify, what did not work ([`src/server/learned-skills.ts:107-166`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/learned-skills.ts#L107-L166)). The rails are the design:

- The draft is redacted, then a **curator model rewrites it as a reusable procedure** with placeholders, dropping personal data and any step that sends something outside unless that is the flow's purpose ([`src/server/learned-skills.ts:47-48`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/learned-skills.ts#L47-L48)).
- A result with a card number, key or login link is refused (`:102-104`).
- Built-in guide names are reserved (`:113-116`).
- **Once the owner edits a skill, the agent cannot overwrite or patch it** — the call is refused and told to ask first (`:120-123`).
- 40 skills, least-recently-used and owner-unedited dropped first (`:16`, `:54-62`).
- Only owner-started work may save; background runs and subagents may read ([memory.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/memory.md#L182-L197)).
- The index lists name and description; the body is read on demand and arrives as **untrusted data**, unlike a built-in guide.

They publish their own eval of the guide library and are honest about it: task completion 71.0% without guides to 76.7% with, unsafe attempts 2 to 4, and an independent judge found no gain in the full-case pass rate ([skills.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/skills.md#L93-L106)). Treat that as a note against over-claiming.

### Parallel work: tasks and subagents

There are two different kinds of parallelism.

- **Tasks** are durable records with their own session: goal, completion condition, planned steps, progress, result. A task runs one turn at a time; different tasks run in parallel with no task limit; the coordinator does not relay each result through another model call ([task-sessions.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/task-sessions.md#L8-L56)). The ledger is the source of truth; each turn sees up to 100 tasks, open first.
- **Subagents** are transient: `delegate {title, instruction}` runs one part in a fresh in-memory session with Nod's tools, a shared workspace and **its own browser**; the model calls it several times in one step and the answers return as untrusted tool results ([`src/runtime/tools.ts:286-293`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/tools.ts#L286-L293), [`src/runtime/index.ts:865-946`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L865-L946)). A subagent cannot delegate, ask or message the owner, connect apps, sign in, pay, or hand its browser over; its browser is discarded when it ends ([threads-and-turns.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/threads-and-turns.md#L127-L136)).

### How progress, plans and tool steps are shown

nod separates **live status** (updates in place, no notification) from **messages** (an interruption, so it must be worth one). The rule is written into the tools:

- `set_status {text}` replaces the live line; 140 characters; "never claim completion" ([`src/runtime/tools.ts:219-224`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/tools.ts#L219-L224)); it emits one `step` event ([`src/runtime/index.ts:954-962`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L954-L962)).
- `send_message` carries an `intent` of `update`, `final` or `handoff`, with the cadence rules in its description ([`src/runtime/tools.ts:259-269`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/tools.ts#L259-L269)).

The line itself falls back through a fixed order: the model's step, `Using <site>`, a known tool's kind of work, `Waiting for <model>` after a quiet minute, a rotating idle word, and a `· N min` quiet timer ([`src/web/components/live-line.tsx:79-112`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/web/components/live-line.tsx#L79-L112)). A retry shows "Gemini 3.8 Flash failed, trying again (1 of 3)"; a reasoning summary whose block starts with a heading becomes a step ([agent-runtime.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/agent-runtime.md#L266-L296)).

**Plans** are the task's `steps` array plus `stepIndex` on the ledger, with `doneWhen`, `waitingFor` and `nextCheck` beside them ([`src/server/tasks.ts:44-77`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/tasks.ts#L44-L77), `:109-118` status, `:120-139` the compact facts a turn sees). The UI draws the checklist with done/current/todo marks ([`src/web/pages/tasks.tsx:186-194`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/web/pages/tasks.tsx#L186-L194), `:283-301`), and open tasks appear as **pips above the composer** so several can be seen and reopened ([`src/web/components/task-dots.tsx:5-37`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/web/components/task-dots.tsx#L5-L37)). Turn status has one label set used everywhere: Queued, Working, Needs your input, Done, Could not finish, Stopped ([`src/web/components/turn-status.tsx:4-11`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/web/components/turn-status.tsx#L4-L11)).

### Onboarding and first run

Six steps: welcome, email, about, brain (model connection), privacy, join ([`src/shared/onboarding.ts:3`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/shared/onboarding.ts#L3)). The "about" answers are a small fixed vocabulary of scope, role, who-for, company size and jobs, plus 200 characters of free text (`:9-60`), and they feed everything later.

The part worth studying is **first-task suggestions**. After setup a model proposes three first tasks: one finishable now and two recurring jobs. Its input is a bounded **read** of the owner's connected accounts (about 50 recent message subjects, a bounded future calendar window), and its instructions are strict: ground the "now" task in a concrete observed item, do not invent events, never infer private facts from an app being connected, never send ([`src/server/onboarding-suggestions.ts:35`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/onboarding-suggestions.ts#L35), reads at `:50`). Each card carries the source it came from.

### Settings and UI patterns

Settings are reachable two ways: the screens, and **asking in chat** — "tell Nod what to change", with the tool reporting the saved result ([chat.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/use/chat.md#L240-L246)). Their interface page is unusually concrete for a young product: 8 px spacing, 8/12/16 px corners, 7:1 body contrast, no eyebrow labels, colour never carries a state alone, one title size, native charts on a fixed palette, shared `Tabs`, `ActionMenu`, `SearchPicker` and `Tooltip`, and a check of light/dark, desktop/phone, empty/full ([interface-design.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/contribute/interface-design.md#L12-L133)).

---

## 2. How Retinue does it now

| Area | Where | What it actually does |
| --- | --- | --- |
| The loop | `crates/retinue-adapter-{claude,codex,acp}` | The CLI owns the loop, the prompt, compaction and tools. The daemon owns the turn, approvals, usage and storage |
| Prompt suffixes | `crates/retinue-daemon/src/main.rs:4764-4794` | One prompt is assembled from the message plus optional suffix blocks: handover, recap, CEO knowledge, `<retinue_memory>`, `<retinue_skills>`, references, reply snapshot. Each block is sent **once** and re-sent only when its hash changes |
| The plan | `crates/retinue-agent-core` `PlanUpdated`; `main.rs:5567-5581`; `apps/desktop/src/ui/TurnPlan.tsx:12-50`; `App.tsx:3202` | The bot's to-do tool becomes `session.plan`, stored on the **turn** (`turns.data.plan`) and drawn under the newest turn only |
| The working line | `apps/desktop/src/ui/doingNow.ts:15-36`, `:56-60`; `ui/WorkingMark.tsx:18-24`; `ui/companionStatus.ts:10`, `:67` | Inferred from the running tool: "Running npm test", "Editing App.tsx", with a step icon and a timer; the companion says "Waiting for Claude Code…" after 60 s of silence. The bot writes no status text |
| Retry and queue | `ui/PromptQueue.tsx:179`; `ui/ComposerStatus`; `App.tsx:3205` | "Retrying automatically" with Cancel; "Waiting for a free working slot for &lt;bot&gt;" while a delegated turn waits |
| Memory | `crates/retinue-daemon/src/memory.rs:396-435`; `memory.rs:386-394` tools; caps at `:23-32` | Per-bot notes (scopes bot, project, shared) with FTS5 (`crates/retinue-storage/src/lib.rs:749`), a reviewed catalog of one-line summaries with short ids injected once per change (3900-char cap), tools `memory_search/get/write/update/forget`, rails against secrets and injection-looking text, 5 writes per turn |
| Skills | `crates/retinue-daemon/src/skills.rs:1026-1089`; tools at `:941-959` | An owner-managed pool in `<data>/skills/<name>`, filled by import from each CLI's own folders, a vendored library at hire time, and built-ins. The per-turn index lists `- name: description` under a cap; `load_skill` reads `SKILL.md`, `read_skill_file` reads the rest. **Bots cannot write a skill** |
| Parallel work | `main.rs:2203-2204`; `ui/BlobSettings.tsx:555-562`; `crates/retinue-daemon/src/queue.rs`, `subbots.rs`, `delegations.rs` | A global `concurrency.botsAtOnce` (default 4, 1–16) is the only enforced limit: a per-bot cap was retired (FIX39 item 18), and the bot page says so. A message to a busy teammate is queued with "Waiting for &lt;bot&gt; to finish another conversation" (`team.rs:1366`). Parallelism also comes from side conversations, sub-bots and `fanout` |
| Questions | `main.rs:5919-5953`; `ui/TeamMessages.tsx:255-298` | Claude's question requests become a card with lettered options, descriptions, multi-select and "Type your own answer" — the same pattern nod uses |
| Context | `crates/retinue-adapter-claude/src/lib.rs:962-965`; `main.rs:5845-5868`; `ui/ContextWindowIndicator.tsx:42`; `knowledge.rs:442-509` | We observe the CLI's `compact_boundary`, emit a context notice, read the provider's own usage (`get_context_usage`), show a percentage and suggest a new conversation at 75%, and after a real context loss send a one-off `<retinue_recap>` (8 messages, 6,000 chars, plus changed files) |
| Onboarding | `apps/desktop/src/ui/FirstRunOnboarding.tsx:141-186` | Four steps: look, name, scan for coding agents, create the CEO. No suggestions after it |
| The review UX | `ui/SessionRail.tsx`, `ChangesView.tsx`, `ApprovalCard.tsx`, `MemoryPage.tsx`, `ContextWindowIndicator` | Changes, diffs, revert with undo, pull requests, "Always allow" rules, memory review, context meter — none of which nod has |

Two things to be precise about. First, our memory catalog and skills index are already the same **"send once, re-send on change, never in the instruction hash"** trick nod uses for owner context, so the cache discipline matches. Second, our per-turn plan is real but short-lived: `latestPlan` comes from the newest turn, so a plan that spans a question or a second turn is replaced.

---

## 3. What they do better, and what we do better

**Their advantages** (all consequences of owning the harness):

| Better | Why it helps |
| --- | --- |
| A live line the model writes | The line says "Comparing flight prices · skyscanner.net", not "Running command". Tool-inferred labels are accurate but low-level |
| One "now" discipline, written into the tools | Status and messages are separate tools with rules, so routine steps do not become interruptions |
| A task model with done-when, waiting-on and next-check | Work outlives a turn, is visible on a card, and can be stopped or replied to individually |
| Memory with sources and a recall pass | Notes carry evidence and a status; a cheap model picks the relevant ones, so answers use history without asking |
| Learned skills | The agent gets better at a repeated flow, and the owner can edit or lock that knowledge |
| Compaction and context budgets it controls | Offloading, summary effort, overflow recovery and "you are nearly full" are all policy, not luck |
| Grounded first-run suggestions | The product's first useful act uses the owner's real data, with a source on each card |
| A docs tool | Product questions are answered from the shipped docs with links, offline |

**Our advantages** (they have no counterpart):

| Better | Evidence |
| --- | --- |
| A team: roles, hiring, a CEO that plans and delegates, a delegation ledger with done-goals, side conversations | `team.rs`, `delegations.rs`, `ui/DelegationGroup.tsx` |
| The task board with lanes, timeline, tree and drag-to-assign | `ui/TasksView.tsx`, `ui/DelegateBoard.tsx`, `tasks.rs` |
| Review as a first-class step: worktrees per conversation, diffs with line comments, revert with undo, PR creation, abandoned-copy cleanup | `git_ops.rs`, `pull_request.rs`, `ui/ChangesView.tsx` |
| Approvals with saved "Always allow" rules, per-conversation modes, five modes including Full access with its rails | `ApprovalRulesPage.tsx`, `permissions.default_mode` |
| Several coding agents with the owner's own sign-ins, no stored provider keys (before the 16 Nov Pi amendment) | adapters, `accounts.rs` |
| Remote on a phone, machine links, handoff | `remote/`, `machines.rs`, `handoff.rs` |
| Memory per bot **and** per project/shared scope, with a review page | `memory.rs`, `MemoryPage.tsx` |
| Skills interoperable with each CLI's own folders, plus a hire-time library | `skills.rs`, `skill_library.rs` |
| Usage, limits, retries, watchdog, honest unknown handling | `quota*.rs`, `retry.rs`, `ContextWindowIndicator` |
| Rich replies and interactive views | `RichTable`, `RichChart`, `openui`, document pane |

So the target is not "become nod". It is: **borrow the parts of their agent experience that do not need us to own the model call.**

---

## 4. Ideas we could adopt, ranked

Each idea says what to build, effort, risk, the Retinue files it would change, and the `AGENTS.md` rules it touches. Ranked by value for effort.

### 1. Learned skills, with an owner review gate — Medium

**What.** A bot can save a procedure it just verified, and patch one that was wrong, through two new tools in our MCP server (`save_skill`, `patch_skill`). The daemon rewrites and redacts the draft (reuse the memory rails: no secrets, card numbers, keys or login links; placeholders for people and numbers), stores it as a skill with `source: "agent"` and `learnedBy`, and the Skills page shows "Learned" with Approve, Edit and Forget. Until approved it is attached only to the bot that learned it; after approval it can join the shared pool. An owner-edited skill is refused to the bot afterwards, and a built-in or vendored name is reserved. Cap at 40 per bot or per project with least-recently-used eviction.

**Why.** This is the single biggest capability gain: our bots repeat project chores (release gates, migration steps, review checklists) and relearn them every conversation. Skills are also cheaper than memory: one line in the index, body on demand.

**Effort.** Medium. The pool, index, loading, importing, hire-time attach and review page already exist; the new part is a write path with a review state.

**Risk.** Medium-high, and worth stating plainly. A learned skill is an **instruction other bots will follow**, so it is a prompt-injection path if it records something a web page told the bot. Mitigations, all from nod: only an owner-started turn may write; redact and rewrite; refuse secrets; owner review before sharing; keep skill text as guidance that grants no permission (our index already says so). Second risk: a stale skill confidently leading a bot wrong. Nod's answer is `used` timestamps plus LRU; ours would be the same, plus the review date on the row.

**Files.** `crates/retinue-daemon/src/skills.rs` (new tools, write path, review state, cap), `crates/retinue-storage/src/lib.rs` (a new `source` value `agent` beside the existing `SOURCES` list at `skills.rs:37`; add `reviewState`, `learnedBy`, `usedAt`), `crates/retinue-daemon/src/team.rs:613-757` (schemas and the dispatch match), `skills.rs` index text, `apps/desktop/src/ui/SkillsPage.tsx` (review actions), `apps/desktop/src/preview/fixtureBridge.ts` (a learned row), tests in `skills.rs` and `main.rs`.

**AGENTS.md rules.** "No copied provider authentication … never log credentials, tokens … message content"; the memory module's write rails are the precedent; `docs/remote/THREAT_MODEL.md` gains a row for "a bot can write an instruction another bot follows"; `tiers.json` rows for any new event or RPC (likely `never`, and `drop`); the 16 Nov vault rule that a model never holds a value applies to what a skill may contain.

### 2. A "now" line the bot writes — Small

**What.** One tool, `set_working_note {text}` (≤140 characters), in the same MCP server, available only while a turn is running and only in an attended conversation. The daemon emits `session.status.note {sessionId, turnId, text}` (not persisted, or kept on the turn row only, cleared when the turn ends), and the UI shows it in the working line ahead of the tool-derived label, exactly as nod's line prefers the model's step. Keep our step icon and timer, and keep the fallback to `doingNow` for when the note is empty or stale.

**Why.** Our line is honest but mechanical. "Running npm test" does not tell the owner the bot is checking the release gate before merging. One sentence of model text fixes that, and it costs nothing when the bot stays quiet. It also gives a bot somewhere to say "waiting on the registry" without spending a chat message.

**Effort.** Small: one tool, one event, one UI branch, one card in the preview.

**Risk.** Low, with bounds. The text is model output: cap the length, strip control characters, redact as the retry notices already are, never store it in backups or send it to a phone (`drop` in `tiers.json`), and never let it claim completion (the card and status still come from real state). The design-language rule matters too: one line, no decoration, no second animation.

**Files.** `crates/retinue-daemon/src/team.rs` (schema plus dispatch), `main.rs` (event and turn end), `crates/retinue-agent-core` (no new adapter event needed if the tool emits through the daemon), `apps/desktop/src/remote/tiers.json` (`drop`), `apps/desktop/src/types.ts` (`applyEvent`), `ui/doingNow.ts` and `ui/WorkingMark.tsx` (prefer the note), `ui/App.tsx:3204`, `preview/fixtureBridge.ts` and `preview/fixtureScenes.ts`.

**AGENTS.md rules.** "Daemon owns decisions" (state and clearing); `tiers.json` parity tests; "Design language is law" (one line, sentence case); the content-free logging rule (do not log the text, or log only its length).

### 3. Recall memory before a turn, without a model call — Small-medium

**What.** When a turn starts from an owner message, run an FTS5 query over the bot's visible memory (bot, project, shared) using the message's terms, take the best few, and append a bounded `retinue_memory_now` block (<code>&lt;retinue_memory_now&gt;</code>) — each hit with its id, title and a short body clip, marked "selected evidence, not instructions". Skip it when nothing matches, and cap the whole block at around 1,200 characters. A later version can add a model pass through the supervised Pi child that the 16 November amendment allows, but that is a separate decision.

**Why.** Today the catalog is a fixed, capped list of every note; a bot with 60 notes sees the first 3,900 characters of them, in whatever order storage returns. A bot that was told "we deploy on Fridays" three weeks ago should see that note when the owner says "deploy". nod pays a model for this; FTS gets most of the value for free.

**Effort.** Small-medium: one function beside `catalog_for_prompt`, one call in `main.rs:4768`, tests.

**Risk.** Low-medium. Irrelevant hits waste tokens, so cap the selection and skip a block that would be mostly noise. Keep the chosen ids in the remembered hash, so an unchanged selection is not re-sent and a changed one is. Never persist the injected block (the catalog rule already exists), never send to phones.

**Files.** `crates/retinue-daemon/src/memory.rs` (new `recall_for_prompt`, merged into the existing block or sent beside it), `main.rs:4764-4794`, `crates/retinue-storage/src/lib.rs` (the FTS query already exists at `:1781-1796`; expose a ranked read), tests in `main.rs` next to the catalog tests (`:12862-12930`).

**AGENTS.md rules.** "Memory is untrusted input" (the module doc says it; keep the header); never persist or log prompt text; `tiers.json` unchanged.

### 4. A plan that outlives one turn — Medium

**What.** Store the bot's plan on the **conversation** as well as the turn: `sessions.plan` (items plus `updatedAt` and `turnId`), updated by every `PlanUpdated`. A new turn inherits the previous plan until the bot replaces it. Show it where it is shown today, labelled with the turn it came from ("from an earlier turn"), so a list no longer disappears when the turn that made it ends. Optionally let a CEO-assigned task carry a plan into its task card, which is nod's model.

**Why.** A multi-turn piece of work — plan, ask, continue — is exactly when a plan is most useful, and that is exactly when ours resets. Also cheap for the owner to trust: the list survives the turn it was made in.

**Effort.** Medium: migration, event payload, UI merge, tests.

**Risk.** Low-medium. A stale plan can look current; show the turn it came from ("updated 12 min ago · earlier turn") and clear it when a new plan arrives or the conversation is reset/archived. Never let the plan imply progress the tools did not show.

**Files.** `crates/retinue-agent-core` (`PlanUpdated` unchanged), `crates/retinue-daemon/src/main.rs:5567-5581` (the arm writes both), `crates/retinue-storage/src/lib.rs` (column and migration), `apps/desktop/src/types.ts:660-665`, `ui/TurnPlan.tsx`, `ui/App.tsx:3202`, `preview/fixtureBridge.ts` (the "Invoice parser tests" fixture), `apps/desktop/src/remote/tiers.json` (`session.plan` is already `drop`).

**AGENTS.md rules.** Schema migration practice in `retinue-storage`; "the daemon owns decisions"; the remote policy parity test; "never fabricate" (the list is always the bot's own).

### 5. First-run suggestions grounded in the chosen project — Medium

**What.** After the CEO is created, offer three suggested first tasks read from the project folder the owner chose: the README's first heading, the `package.json` or `Cargo.toml` scripts, the last five commit subjects, any `AGENTS.md` headings. Each suggestion sits on a card with the file it came from, and one click opens a conversation with it. Nothing runs until the owner sends it.

**Why.** Our onboarding ends at "Create CEO", and the first thing the owner sees is an empty composer. nod's suggestions are its best first-run idea; ours can be grounded in local files instead of mail, which fits our privacy rules better.

**Effort.** Medium: a read path in the daemon (we already have `repository.rs` and `project_search.rs`), a proposal or suggestion row, a card, a preview fixture.

**Risk.** Low-medium. Suggestions can be wrong or bland; each must cite a real file and be dismissible, and the whole feature must degrade to static prompts when the folder is empty or unreadable. No model call is needed for a first version; a later version could summarise with the CEO's own CLI.

**Files.** `apps/desktop/src/ui/FirstRunOnboarding.tsx:186`, `crates/retinue-daemon/src/repository.rs` or a new `suggestions.rs`, `crates/retinue-daemon/src/proposals.rs` if it reuses a proposal card, `main.rs` for the RPC, `preview/fixtureBridge.ts`, `ui/StaffProjectDialog.tsx` (the same card after a new project).

**AGENTS.md rules.** "The UI never fabricates success, usage or prices" extends to suggestions: cite the file, no invented tasks; front end is the Director's lane; design language (no fake data in cards); nothing leaves the machine.

### 6. A docs search tool for bots — Small-medium

**What.** `search_docs {query}` and `read_doc {path, offset}` over the documentation library the app already builds (`docs/library/content`), indexed at startup like `skill_library` or embedded with `include_str!`. The `retinue` built-in skill gains one line: "for any question about Retinue itself, search the docs and cite the path".

**Why.** Bots answer "how do I…" questions from a large but finite instruction set and get details wrong. nod's `nod_docs` is a small, offline, well-scoped win, and we already have the content and a build step (`scripts/docs-library.mjs`).

**Effort.** Small-medium: a module, a tool, a build embedding, tests.

**Risk.** Low. Docs can lag the build; the tool must return the path and a note that docs describe the shipped behaviour, not current state (nod says exactly that). Size: index at startup, bound each read.

**Files.** new `crates/retinue-daemon/src/docs_search.rs`, `crates/retinue-daemon/build.rs` (embed `docs/library/content`), `crates/retinue-daemon/src/team.rs` (schema and dispatch, `:613-757`), `crates/retinue-daemon/src/skills/retinue/SKILL.md`, and tests in `team.rs` and `main.rs`.

**AGENTS.md rules.** `docs/` protected paths (never move `docs/library` without the build); "keep this skill current"; no network; cite `path:line`.

### 7. Search earlier messages, for the owner and the bot — Medium

**What.** FTS5 over messages (per conversation, with a project-wide variant), one RPC for a search box in the conversation header, and a bot tool `search_history {query, from?, to?}` scoped to the bot's own conversation or project. Show date, author and a highlighted excerpt.

**Why.** Today neither the owner nor a bot can look back. A compacted or fresh session cannot find "what did we decide about the retry budget" except by scrolling in the CLI's own transcript. nod's `history_search` and chat search cover this; it is the other half of long-term context, next to memory.

**Effort.** Medium: index maintenance (insert on message write), migration, RPC, tool, UI, tests.

**Risk.** Medium. Privacy: a bot must only see its own project, never another project's conversations or a machine's. Performance: index growth; the perf gate exists (`scripts/perf-gate.mjs`), so measure. Query text must not be logged.

**Files.** `crates/retinue-storage/src/lib.rs` (new FTS table and maintenance beside `memory_fts` at `:749`), `main.rs` (RPC and tool), `team.rs` (tool), `apps/desktop/src/ui/ConversationsPanel.tsx` or the chat header (search), `apps/desktop/src/types.ts`, `apps/desktop/src/remote/tiers.json`.

**AGENTS.md rules.** Remote never for the raw search unless a tier row is written; local-only data; the database read-connection rules (a search must not take the writer lock); perf gate.

### Worth considering later, not ranked

- **Per-bot concurrency is already ours.** Since FIX39 there is no per-bot cap, so several conversations of one bot can run at once, bounded by the global limit. nod's parallel tasks need no new switch. What they do raise is the **file question**: two turns of one bot in one folder collide unless each conversation already has its own copy. That is a rule to write down, not a feature to build.
- **A bot may propose a settings change** (nod lets the owner change settings by asking). Ours would be a proposal card applied by the daemon, never a tool that writes `settings.set`. Needs an owner decision and an `AGENTS.md` note because `settings.set` is desktop-only and remote `never`.
- **Transient helpers instead of durable sub-bots** for a single turn. We already have `spawn_subbots` and Claude's own sub-agents; the gap is that sub-bots are designed for delegating work, and a helper should cost one prompt and return text.
- **Idle words in the working line**, as nod has. Ours is silent with a timer. That is a design-language question for the owner, not a technical one.

---

## 5. Open questions for the owner

1. **Learned skills: when may one enter the pool?** My recommendation: immediately for the bot that learned it, review before any other bot can use it, and never overwrite an owner-edited skill. The alternative — review first, always — is safer and slower.
2. **Who may write one?** nod allows only an owner-started turn. Should a scheduled task be allowed to save a skill it verified, given our automation rails?
3. **A bot-written status line:** is model text acceptable in the working line, or would you rather it appear only in the companion and the rail? And is 140 characters the right cap for English and for other languages?
4. **Plan lifetime:** should a new turn inherit the previous plan by default, or only when the bot did not finish (an unanswered question, a stopped turn)?
5. **Memory recall:** FTS only to start, or is a model pass per owner turn acceptable cost once the supervised Pi child exists? FTS is close to free; a model pass is one small call per turn.
6. **Onboarding:** may the daemon read the chosen project folder to build suggestions (file names, README headings, scripts, commit subjects), with nothing sent off the machine? Any folder we should not read, such as one with secrets in its name?
7. **History search:** one conversation, or every conversation in the project? And does the CEO see project-wide history while an ordinary bot sees only its own conversation?
8. **Do we want nod's liveliness** — idle words, "Got your message, I'll read it after this step" — or the quieter line we have? It is a voice question, and voice is yours.

---

## 6. What I verified, and what I assumed

**Verified by reading the clone at `47cfee3fe77279e1d543a367131d3ff2c1202e18`:** every `src/...` citation above; the shape of `runTurn`, the tool guard and admission chain, `set_status` and `send_message` descriptions, `delegate` and its restrictions, the prompt layers and owner context, the compaction policy file, tool-output bounding and `fit()`, the memory tree and index, the recall pass and its one-second deadline, the learned-skill rails, the task schema and step fields, the live line, task pips, turn labels, onboarding steps and the suggestion instructions.

**Verified from nod's own docs:** the numbers (compaction thresholds, 256 KiB memory, 40 skills, their eval table). These are their claims, not measurements of mine.

**Verified on our side:** every `path:line` citation above, read in this checkout on 16 November 2026.

**Assumed, not proven:**

- That a new tool added to `team.rs` reaches all three CLIs the same way today's tools do. The catalog and the dispatch match are shared, but Claude is launched with `--allowedTools mcp__retinue` only, and Codex and OpenCode each have their own tool plumbing; the first implementation step is to confirm each of the three sees the new tool.
- That FTS recall on a prompt is cheap enough at our data sizes. Memory is one FTS table; the cost is unmeasured.
- That messages can be indexed without hurting write throughput, because every message write would add an index write. Needs a measurement on the perf seed.
- That a learned-skill draft can be redacted well enough by rules alone. nod uses a model for the rewrite; without one, our redaction is weaker and the review gate matters more.

**Not done:** nod was never installed, built or run. No file from it was copied into Retinue. Nothing was committed.
