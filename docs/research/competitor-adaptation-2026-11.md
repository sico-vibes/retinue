# Competitor adaptation research: T3 Code, OpenBot, Nuphos, Monocode

Date: 10 November 2026. Author: Grey (research lane). Task: study four coding-agent apps, find ideas worth adapting, and describe two features in depth ("Send now" while a bot works, and resuming after a usage limit resets).

This is research only. No product code was changed.

## 0. Method, confidence, and licences

**How I worked.** Each repository was cloned shallow into `%TEMP%\opencode\competitors\` and read directly; nothing was built or run, no dependency installed, and the clones are removed at the end. Every claim about another product cites `path:line` in that clone at the commit listed in section 9. Claims about Retinue cite our own files. Public material (docs, blog posts, videos) is cited by URL; where I could only read a README or a doc, I say so.

**Confidence labels used below:**

- **[code]** verified in their source.
- **[docs]** from their own documentation or README, not checked in code.
- **[public]** a third-party article, post or video.
- **[unchecked]** I could not verify it; treat it as a lead, not a fact.

**Licences (this decides reuse, not just reading):**

| Project | Licence | What we may do |
| --- | --- | --- |
| T3 Code (`pingdotgg/t3code`) | MIT (`LICENSE:1`) | Ideas **and** code, with attribution. |
| Monocode (`hardbeat920/monocode`) | MIT (`LICENSE:1`) | Ideas **and** code, with attribution. |
| Nuphos (`nuphos/nuphos`) | Apache-2.0 (`LICENSE:1`), trademark policy in `TRADEMARK.md` | Ideas and code with attribution and NOTICE; no use of their marks. |
| OpenBot (`nightly-labs/openbot`) | PolyForm Noncommercial 1.0.0 (`LICENSE:1`) | **Ideas only.** Copying code into Retinue would breach the licence if Retinue is ever sold; read it as a design reference. |

**Retinue rules this report must not break** (`AGENTS.md`, and the golden rules in `.claude/skills/retinue-development/SKILL.md`): the daemon owns sessions, permission decisions, usage and storage; the UI never fabricates success, usage or prices; no copied provider authentication and no terminal scraping; prompts go through stdin/protocol, never shell text; lanes never commit. Every proposal below keeps the decision in the daemon and keeps provider credentials out of the UI.

## 1. Summary: the ideas most worth adapting

Ranked by value to Retinue against the effort to build. "Effort" means daemon + UI + tests in this codebase, at the pace of the last few waves.

| # | Idea | From | Why it is worth it | Effort |
| --- | --- | --- | --- | --- |
| 1 | **"Send now" delivers a queued message into the running turn** (steer), not just to the front of the queue | T3 Code, Monocode | Our Send now only moves the item to the front (`queue.rs:326`), so the owner waits for a whole turn. T3 and Monocode both put the message in front of the bot now. This is the most useful difference found. | M–L |
| 2 | **Promote, then interrupt**, for providers that cannot steer | OpenBot, T3 Code | OpenBot's non-steer path is "reorder to head, interrupt the turn, let it run next" (`turns.tsx:435-475`). We have both halves already and never join them automatically. | M |
| 3 | **Steering for Claude and OpenCode, not just Codex** | T3 Code, OpenBot | `provider_can_steer` is `codex` only (`main.rs:2450`), so "Steer now" is dead for the other two providers Retinue ships. T3 steers on Claude and OpenCode and falls back to interrupt-restart. | L |
| 4 | **Per-conversation "Resume at reset" and "Snooze until reset", in a banner** | T3 Code, Monocode | We have a global experiment and an automatic sweep; the owner cannot opt one conversation in or out, or delay it. T3's banner says "Resumes 2:00 AM" or "Resets 2:00 AM" and offers both actions. | S–M |
| 5 | **Edit a queued message in the composer** | T3 Code | Our edit is an `<input>` that commits on blur (`PromptQueue.tsx:74`). T3 opens the item in the composer with its attachments, a Cancel, and restores the previous draft. | S |
| 6 | **Switch a limited conversation to another account of the same provider** | Monocode | `withUsageLimitAccount` re-points the session at another account, carrying the native transcript when possible (`usageLimit.ts:33-55`). We already track accounts and resume provider sessions. | M |
| 7 | **"Second opinion": hand a finished turn to a different provider to review** | Monocode | A multi-runtime app is the one place this is cheap; the prompt, file list and CI context are already bounded (`secondOpinion.ts:104-150`). Today we only have heavier bot-to-bot delegation. | M |
| 8 | **"Run this as a task" on a queued message** | OpenBot | Promote a follow-up into its own task thread (`agent-service.ts:5246-5278`). We have a task board and a delegation ledger; this connects two things we already own. | S–M |
| 9 | **`/btw`: a read-only side question about one turn** | Monocode | An isolated prompt with a bounded transcript snapshot (`btw.ts`), never written into the parent conversation. Cheaper than our side conversations when the owner just wants to ask "why?". | M |
| 10 | **Settle a conversation**, with auto-settle and undo | T3 Code | A state between "active" and "archived": handled, out of the way, still searchable. With many bots, that is the difference between a working list and noise. | M |

Close runner-ups, all small: a queue-first shortcut and a per-message "opposite action" (T3), a per-bot busy-send mode (OpenBot), marking `steerFallback` on a queue row (OpenBot), and memory redaction patterns (Monocode). Section 8 lists every candidate, including the ones not worth doing.

## 2. Deep dive: sending a message while the bot is working, and "Send now"

### 2.1 How T3 Code does it [code]

**The queue is not a queue table; a queued message is a run.** A message is stored as a message row plus a `run` row whose `status` is `"queued"`, with a `queuePosition` for order (`Orchestrator.ts:6513-6536`). Because runs live in the projection database (`orchestration_v2_projection_runs`, `apps/server/src/orchestration-v2/ProjectionStore.ts:2129`), the queue survives a restart and is re-derived from the database, not from memory.

**Which mode a send takes is a policy decision, not a UI guess.** `resolveMessageDispatchIntent` (`CommandPolicy.ts:92-116`) and `resolveDeliveryIntent` (`CommandPolicy.ts:144-182`) choose between five dispatch modes (`orchestrationV2.ts:3217-3226`):

- `steer_active` — text joins the running turn.
- `restart_active` — interrupt the turn, then restart it with the new text (used for providers that cannot steer live).
- `queue_after_active` — wait for the turn to end.
- `start_immediately` — no turn running.
- `defer_start` — start later with its own workspace.

The interesting part is the capability ladder at `CommandPolicy.ts:171-181`: if the provider session reports `supportsActiveSteering`, steer; else if `supportsQueuedMessages`, queue; else if `supportsInterrupt` + `supportsSteeringByInterruptRestart`, restart; else queue. Two special cases are handled first: `/compact` and `/logout` cannot accept a steer, so they queue (`CommandPolicy.ts:99-104`), and a run that is `preparing`/`starting` or whose provider turn has not actually started yet queues, so a steer cannot be rejected in the gap (`CommandPolicy.ts:159-169`).

**What "Send now" does.** The web control emits `promoteQueuedRun` when a turn is active, and `resumeThreadQueue` when the thread is stopped (`apps/web/src/components/chat/QueuedRunsControl.tsx:172-192`), gated by `deriveThreadQueueWorkflowState` (`packages/client-runtime/src/state/threadWorkflows.ts:164-179`). On the server, `queued-message.promote-to-steer` (`orchestrationV2.ts:3275-3281`) is dispatched by `dispatchQueuedMessagePromoteToSteer` (`apps/server/src/orchestration-v2/Orchestrator.ts:9997-10220`), which:

1. refuses if the run is not `queued` (`:10028-10035`), is an automatic completion/notification (`:10058-10067`), or wants a fresh context (`:10113-10119`);
2. marks the queued run `cancelled` with `queuePosition: null` (`:10151-10164`);
3. calls `dispatchSteerIntoRun` with the queued message's text and attachments against the active run (`:10192-10211`).

So **Send now is a steer, not an interrupt**: the active turn keeps its work and receives the text. When the provider cannot steer live, `decideSteeringExecution` (`CommandPolicy.ts:257-278`) returns `interrupt_restart`, and the turn is interrupted and restarted with the new text — the user's message is never dropped.

**Edge cases, all handled explicitly:**

- *Several queued messages*: only the queue head auto-starts when a turn ends; `queued-run.start` refuses while anything is active (`Orchestrator.ts:13731-13752`). Any queued item may be promoted, not just the head, and promotion emits `queuePosition: 1` for a compaction skip (`:10090-10098`).
- *Editing and removing*: `queued-run.edit` replaces text and optionally all attachments, absent leaves attachments for older clients (`orchestrationV2.ts:3306-3315`); `queued-run.cancel` and `queued-run.reorder` are separate commands (`:3287-3305`). The UI edits in the composer, keeps the row highlighted, and restores the previous draft afterwards (`docs/user/composer.md`, "Queued messages"; `QueuedRunsControl.tsx:308-372`).
- *Approvals waiting*: a run in `waiting` counts as active (`CommandPolicy.ts:119-126`), so a plain send queues behind it; a text reply while a Claude approval is pending declines that request and feeds the text back to the agent (`docs/user/permission-modes.md`).
- *A turn that fails*: queued runs are held, not fired — stopping a thread calls `holdStoppedThread`, which sets `queueHeld` on each queued run (`Orchestrator.ts:11832-11840`, `:2161`), and the UI shows "Resume" (`docs/user/composer.md`).
- *A crash or restart*: the queue is projection rows, so it comes back; a queued run is only started by an explicit `queue.resume`/`queued-run.start` or the after-turn drain.
- *Racy send*: the integration test `keeps a message queued when send now races with a running turn` (`SteeringCompletion.integration.test.ts:918`) pins that a promotion that loses the race leaves the message queued rather than losing it.

**One more nicety**: the client shows optimistic queued rows until the server acknowledges them, and a queued row is drawn dashed and unfilled (`QueuedRunsControl.tsx:452-460`) — "a turn that has not been sent yet".

### 2.2 How Monocode does it [code]

Monocode keeps `queuedMessages` as JSON on the session row (`crates/monocode-core/src/store/shared_schema.sql:12`, written through `store/shared.rs:1214-1223`), and decides delivery in the renderer:

- `canDispatchQueuedHead` — idle, not usage-limited, not paused/resuming, not editing the head, not preparing a handoff (`src/features/sessions/model/messageQueue.ts:37-50`).
- `canSteerQueuedHead` — busy, turn ready, not usage-limited, no unanswered question, head intent not plan/orchestrate (`messageQueue.ts:52-73`). This is the same "steer the head now" idea as T3's Send now.
- `deliverQueuedFollowUp` re-checks the session after preparation, so an edit, cancel, provider switch or stop during preparation wins (`queuedFollowUp.ts:6-37`).
- A `TurnNotReadyError` defers rather than fails (`queuedFollowUp.ts:29-33`) — the message stays at the head and is retried when the turn is ready.

The gap versus T3: Monocode has no persisted "held" flag on the queue; it pauses via `queueStatus`, and a quit interrupts the turn with an explicit transcript line and a `Continue from where you left off.` prompt on restart (`src/features/sessions/model/inFlight.ts:7-10`).

### 2.3 How Retinue works today [code]

- **Storage**: `queued_prompts` rows per session, with `kind` (`user`, `send_later`, `retry`), `position`, `due_at`, `status` and a `sendId` (`crates/retinue-storage/src/lib.rs:4112-4125`). Cap 20 (`crates/retinue-daemon/src/queue.rs:22`).
- **Adding while busy**: the UI sends `session.queue.add` with `clientMessageId` (`apps/desktop/src/ui/App.tsx:2118`); the daemon validates the body exactly like `session.prompt`, deduplicates by client id, and stores an allowlisted copy of the prompt (`queue.rs:167-233`).
- **Drain**: after a completed turn the next item is claimed and sent under the same `clientMessageId`, so a replay answers with the first outcome (`queue.rs:471-484`, `399-454`). A failed or cancelled turn pauses the queue (`:478-481`), a restart pauses plain prompts (`:56-59`), and an ambiguous delivery marks the item failed instead of replaying it (`:433-450`).
- **Send now**: `session.queue.send_now` (`queue.rs:308-332`). While a turn is running it **moves the item to the front** and returns `{queued: true}`; it does not reach the running turn. The UI says exactly that: the button's title is "Send next, when the current turn ends" (`apps/desktop/src/ui/PromptQueue.tsx:92`). When idle, it claims and sends the item immediately.
- **Steering**: a separate call, `session.steer` (`crates/retinue-daemon/src/main.rs:4531-4584`). It needs a live turn, records the text as a user message with `meta.kind == "steer"`, and when the adapter reports the turn is gone it queues the text instead of dropping it (`main.rs:4578-4581`). **Only Codex can steer**: `provider_can_steer` matches `"codex"` (`main.rs:2450-2452`), and `ClaudeAdapter`/`AcpAdapter` return `Unsupported` (`main.rs:10888-10889`).
- **The composer** offers "Steer now / After this turn" as a setting (`ui/busySend.ts:5-7`) with a per-message opposite action (`busySend.ts:53-60`), and a separate "Stop and send instead" link while busy (`PromptQueue.tsx:143`).
- **Editing**: an inline `<input>` inside the queue row, committing on blur or Enter (`PromptQueue.tsx:74-77`). Attachments cannot be changed; `session.queue.update` only takes `text` and `dueAt` (`queue.rs:242-282`).

### 2.4 The gap, precisely

| Behaviour | T3 Code | Monocode | Retinue today |
| --- | --- | --- | --- |
| Send now while busy | Steers into the running turn (or interrupt-restarts) | Steers the queue head when the turn is ready | Moves to the front; waits for the turn to end |
| Per-item control | Any queued item can be promoted, edited, reordered, cancelled | Head can steer; edit/reorder/cancel | Any item can be edited, reordered, cancelled |
| Attachments on edit | Replaceable; the message keeps its images | Not applicable | Text only |
| Stopped/restarted thread | Queued runs held; explicit Resume | `queueStatus: paused`; sends only after recovery | Paused; Resume clears it |
| Provider without steering | Interrupt and restart with the new text | Refuses the steer, keeps it queued | Queues, and says so |

**What we would change.** Make `session.queue.send_now` (and a new `session.queue.steer`) try the running turn first:

1. `queue.rs: send_now` claims the item (`queue_claim`) rather than only reordering, then:
   - if a turn is live and the runtime's `steer` capability is on, call the same code path as `session_steer_marked` with the item's text, reusing its `sendId` as the message id so a retry cannot duplicate it;
   - if the adapter answers `Stale`, put the item back at the front (today's behaviour) and report `{steered:false, queued:true}`;
   - if the runtime cannot steer, keep the current reorder behaviour and return the reason, so the UI can say "no steering on this provider" instead of pretending.
2. Items with attachments, commands, references or annotations keep queueing, as the composer already decides (`busySend.ts:58`), because `SteerRequest` carries text only (`main.rs:4566`). Carrying attachments would need an adapter change; T3 does carry them, so this is a known limit, not a law of nature.
3. Add the capability ladder for our adapters, starting with Claude: `interrupt_restart` is a legitimate middle rung and we already own "stop then send" in the composer — the difference is doing it automatically for a message the owner already wrote.

**Parts touched:** daemon `crates/retinue-daemon/src/queue.rs` (`send_now`, a `steer_claimed` helper), `main.rs` (`session_steer_marked` refactor so a queued item can call it, `provider_can_steer` table), adapters `retinue-adapter-claude` and `retinue-adapter-acp`/`-opencode` (`steer`), storage `queue_claim`/`queue_requeue` already exist; UI `PromptQueue.tsx` (label and result), `App.tsx` (`sendQueuedNow`), `types.ts` (the send-now reply); contract in `docs/ipc-contract.md`; tests beside `queue.rs`, the adapters' `#[cfg(test)]`, and `PromptQueue` mounted tests.

**Risks:** double delivery (mitigated by the existing `clientMessageId`/`sendId` rule), a steer accepted after the turn's last tool call (provider-dependent; T3 handles this by queueing when the provider turn is not `running`), and permission prompts: a steer does not answer an approval, and must never change the mode (`main.rs:4526-4530`).

## 3. Deep dive: scheduling a resume after the usage limit resets

### 3.1 How T3 Code detects the limit and the reset time [code]

The failure *class* is what the UI and the worker key on: `class: "usage_limit"` with an optional `resetAt` on the turn's error item (`packages/contracts/src/orchestrationV2.ts`, `OrchestrationV2ProviderFailure`; example fixture `scripts/fixtures/issue-21397.json:122`).

Per provider:

- **Claude**: the SDK result's `api_error_status === 429` or a `rate_limit_info` with `status: "rejected"` marks `usage_limit`; the reset comes from `resetsAt` (epoch seconds) in that payload, or from a gateway 503 whose text carries `(reset after 1h 2m 3s)`, which is turned into an absolute time from the turn's completion (`apps/server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts:4089-4094` and `:4165-4181`).
- **Codex**: a `usageLimitExceeded` error code, or all exhausted rate-limit windows. The reset is the **latest** reset among windows at 100 %, and if any exhausted window has no reset time it returns null rather than guessing (`apps/server/src/provider/codexUsageLimits.ts:189-203`).
- **OpenCode**: a 429 `APIError` (`packages/provider-opencode/src/server/adapter.ts:2667-2671`), or a `Resets at <ISO>` line in the message (`packages/provider-opencode/src/server/v2/adapter.ts:559`); OpenCode Go also reports rolling/weekly/monthly windows with `resetsAt` (`packages/provider-opencode/src/server/usageLimits.ts:38`).

### 3.2 How T3 schedules the resume [code]

`UsageLimitRecoveryWorker.ts` is the whole mechanism:

- It is a **sweep**, registered with the scheduler on every tick (`:153`), reading candidates from SQL; there is no in-memory timer, so a restart loses nothing. A `DueHint` invalidates the sweep when a relevant event arrives (`:82-101`) and records the next wake time when nothing acted (`:142-145`).
- **Arming**: `limitRecoveryCommand` (`:17-79`) refuses unless the thread is `failed`, the last error class is `usage_limit`, there is a latest run and a reset time, it is not archived, not settled and no runtime request is pending. It also refuses an **already-expired** reset reported with a fresh failure ("cannot start a retry loop", `:33-39`) and only arms up to 24 hours overdue (`:15`, `:43`).
- **State**: `{runId, resetAt, autoResume, snooze}` is stored on the thread shell as `limitRecovery` (`orchestrationV2.ts:488`), with the identity `threadId:runId:resetMs` so a new failure or a new reset re-arms cleanly (`:40-55`).
- **Resume**: when armed with `autoResume`, the reset has passed, and the thread is not snoozed, it dispatches a message with `text: "Continue where you left off."`, `dispatchMode: start_immediately`, a deterministic `messageId` (`limit-resume:…`) and `usageLimitContinuationOfRunId` (`:63-78`). The deterministic id is the idempotency: a sweep that runs twice cannot send twice.
- **Candidate query**: the SQL in `ProjectionStore.ts:4090-4161` only considers threads whose **latest executed run** failed (queued runs excluded) with a usage-limit error and a reset in the future, excludes subagent threads of settled/archived parents, and honours `snoozedUntil`. Only threads with a usage-limit error item are considered at all, to keep the sweep cheap (`:4119-4127`).
- **Snooze**: a thread-level `snoozedUntil` gates both arming and resuming, and `limitRecoveryOwnsSnooze` in the client decides whether the banner's snooze is still the live one (`packages/client-runtime/src/state/thread-execution.ts:441`).

**What the user sees** (`apps/web/src/components/chat/UsageLimitRecoveryBanner.tsx`):

- A warning bar: "Usage limit reached" with "Resets 2:00 AM" — or "Resumes 2:00 AM" when an auto-resume owns it (`:59-75`), and "Reset time unavailable; retry manually" when there is no reset.
- Two inline actions, "Resume at reset" / "Cancel auto-resume" and "Snooze until reset" / "Wake now" (`:141-153`), with a toast on failure rather than an inline error.
- For ChatGPT-plan limits it uses OpenAI's own wording and a "Copy request for admin" action (`:77-99`).
- A held limit also keeps its worktree from being cleaned up (`apps/server/src/storageCleanup.ts:163-189`).

**Edge cases:** a pending runtime request blocks recovery changes (`UsageLimitRecoveryBanner.tsx:49-51`); an archived or settled thread is never resumed; a subagent whose parent settled is abandoned (`ProjectionStore.ts:4153-4161`); a thread that already passed its reset when it failed is left for the user.

### 3.3 How Monocode does it [code]

Monocode is the minimal version, and honest about it:

- `usageLimitFromError` is a regex fallback for adapters that only send text (`src/features/sessions/model/usageLimit.ts:4-10`).
- `usageLimitResumeDue` requires an armed `resumeAtReset`, a reset time, an idle session, and a **30-second grace** past the reset, because providers can still refuse at the boundary (`usageLimit.ts:57`, `:72-79`).
- The timer is the **desktop app**, not a daemon; the phone is told so: `limitation: "Automatic continuation requires Monocode running with this session loaded. This preference is kept only until the session closes or the app restarts."` (`usageLimit.ts:95-140`).
- The phone's actions carry an opaque token built from the session id, harness, model, provider session id, last block id and the limit object, so a stale phone view is refused rather than acted on (`usageLimit.ts:100-115`, `validateMobileUsageLimit`).
- Nice extra: `withUsageLimitAccount` moves the limited conversation to another account of the same provider, carrying the native session when possible and otherwise starting a fresh provider thread from a handoff brief (`usageLimit.ts:33-55`).

### 3.4 How Retinue works today [code]

- **Detection**: the adapters' structured hint wins — `RateLimited` plus `retryAfterSecs`, parsed from `resetsAt`/`resetAt`/`retry_after` at any depth (`crates/retinue-agent-core/src/recovery.rs:117-149`). When the hint has no time, the daemon reads the provider's quota snapshot and takes the latest reset among windows at 100 %, refreshing a snapshot older than ten minutes (`crates/retinue-daemon/src/retry.rs:540-576`).
- **State**: the session row holds `state='limited'` and `limited_until`, written once per failed turn (`crates/retinue-storage/src/lib.rs:4038`), plus `recovery_count`, `lastLimitReset` and `continuation_pending`. A reset that is not newer than the last one is not stored, which is the same loop guard T3 gets from its identity.
- **Scheduling**: `retry::sweep` runs every second (`retry.rs:655-664`), claims due sessions (`lib.rs:4060`) and, in one transaction, clears the limited state, bumps `recovery_count` and inserts a `retry` queue item with the text **"Continue where you left off."** (`lib.rs:4097-4109`). A restart is safe for the same reason as T3's: the state is in SQLite.
- **Caps and giving up**: `recovery_count < 3` (`lib.rs:4038`, `:4100`); at the cap `settle_exhausted` writes "Automatic continuation stopped after 3 tries. Send a message to continue." (`retry.rs:624-635`).
- **The message the owner sees**: a transcript note, "Limited until {at}. I'll pick up when the window opens." when the experiment is on, "…Resume when you're ready." when it is off, and "Limited. The provider gave no new reset time." when there is none (`retry.rs:251-258`), rendered as a chip with a "Continue in another bot…" action (`apps/desktop/src/ui/TeamMessages.tsx:137-142`).
- **Per-message "when the limit resets"**: `session.queue.add` with `whenLimitResets: true` takes the conversation's current reset as `due_at`, refuses when the conversation is not waiting for a limit, and an extended window moves every waiter to the new reset (`queue.rs:190-201`, `:372-380`; test `phase1b_tests.rs:344`).
- **Controls**: one global switch, `resumeAfterUsageLimits`, on by default (`apps/desktop/src/ui/experiments.ts:15,25`), with no per-conversation opt-in, no snooze, and no cancel for a pending continuation.

### 3.5 The gap, precisely

| Behaviour | T3 Code | Monocode | Retinue today |
| --- | --- | --- | --- |
| Where the timer lives | Daemon sweep over SQL, restart-safe | Desktop app, session must be loaded | Daemon sweep over SQL, restart-safe |
| Opt in per conversation | Yes, stored on the thread | Yes, stored on the session | No; one global switch |
| Delay it | "Snooze until reset", "Wake now" | No | No |
| What the owner sees | Banner: "Resumes/Resets <time>", two actions | Card with reset label and two actions | Transcript chip with the time |
| Loop guard | Reset must be after the run ended; 24 h overdue cap; deterministic message id | 30 s grace | Reset must be newer than the last one; 3-attempt cap |
| Give up | Not needed (no attempt cap) | n/a | After 3 tries, a note telling the owner to send a message |
| Provider coverage of reset times | Claude (incl. gateway text), Codex (all windows), OpenCode Go windows | Regex fallback only | Structured hints + quota snapshot |

Our scheduler is at least as solid as T3's, and our per-message "when the limit resets" has no counterpart in either. What theirs does better is **control and legibility**: the owner can see that a resume is armed, cancel it, or delay it, and the app distinguishes "Resets" from "Resumes". Our 3-attempt cap also ends a busy day silently apart from one chip.

**What we would change:**

1. Store two per-conversation fields beside `limited_until` — `auto_resume` (bool, default from the global experiment) and `snoozed_until` — written by a new RPC pair `session.limit.resume {sessionId, autoResume}` and `session.limit.snooze {sessionId, until}`. The sweep already selects by `state='limited' AND limited_until<=now`, so adding the two predicates is a one-line change in `lib.rs:4060` and the claim in `lib.rs:4100`.
2. Replace the transcript chip with a banner that owns the state: "Resumes 3:00 AM" when armed, "Resets 3:00 AM" when not, with Resume at reset / Snooze until reset, and a cancel. Keep the note for the transcript record; the banner is the control.
3. Keep the 3-attempt cap but surface the countdown ("2 automatic continuations left") and make the cap a per-conversation reset when the owner sends a message.
4. Do not build a second scheduler. Reuse `retry::sweep`; the arming/disarming is data, which is also what makes it survive a restart.
5. Add Monocode's account switch later, on top of `accounts.rs`: a "Continue on another account" action in the same banner, carrying the provider session where the provider allows it.

**Parts touched:** storage `crates/retinue-storage/src/lib.rs` (`limited` claim at `:4100`, `watchdog_limit` at `:4038`, a migration for two columns or JSON fields), daemon `retry.rs` (`sweep`, `after_failed_turn`, the note text), `main.rs` (new RPC names), `remote/tiers.json` (the banner is a write from the desktop only; a phone should read, not change, recovery), UI `TeamMessages.tsx`/a new `UsageLimitBanner.tsx`, `App.tsx` wiring, `experiments.ts` default, `settingsIndex.ts` if a setting moves into the page, plus `phase1b_tests.rs` and a mounted test.

**Risks:** a per-conversation opt-in must not let an unattended automation bypass the rail (`is_unattended` keeps scheduled runs on the automation rules; keep it); and the banner must never show a fabricated reset time — when the provider gives none, it must say so, as it does today.

## 4. T3 Code (`pingdotgg/t3code`, MIT)

### 4.1 What it is

An "agent harness control surface": a server that owns provider processes, terminals, Git and project files, plus four clients — a local web app, an Electron desktop app, an iOS app and an Android app. It drives Claude Code, Codex, Cursor, Grok Build, OpenCode, Antigravity and fx on the user's own subscriptions (`README.md:3-7`). The product bet is "remote-ready": the same environment is reachable from a phone, and the machine that owns the workspace does the work (`docs/internals/overview.md:3-6`).

### 4.2 Architecture in brief

- **Server owns everything executable**: providers, terminals, Git, files; clients control it over authenticated RPC, and providers sit behind adapters so the domain never branches per provider (`docs/internals/overview.md:3-31`, `CommandPolicy.ts:1-20`).
- **Event-sourced orchestration**: commands are decided into events, projections are the source of truth, and `EventSink` commits events, projections, the command receipt and outbound effects in one transaction. A command acknowledgement means the intent committed, not that the provider finished; an `EffectWorker` performs the side effects afterwards (`docs/internals/overview.md:44-52`). This is why their queue and limit recovery survive restarts and why a retried command cannot duplicate work.
- **A capability model per provider session**: `supportsActiveSteering`, `supportsQueuedMessages`, `supportsInterrupt`, `supportsSteeringByInterruptRestart`, fork, rollback, context handoff (`CommandPolicy.ts:24-35`). Every feature the UI offers is gated by one of these, and a refusal names the capability (`:38-51`).
- **Monorepo split**: `apps/server`, `apps/desktop`, `apps/web`, `apps/mobile`, and ~30 `packages/*` including one per provider (`packages/provider-opencode`, `provider-cursor`, …) and one per source-control host.

### 4.3 Notable features

| Feature | What it is | Where |
| --- | --- | --- |
| Send now and queue | Described in section 2 | `QueuedRunsControl.tsx`, `Orchestrator.ts:9997` |
| Resume after a usage limit | Described in section 3 | `UsageLimitRecoveryWorker.ts` |
| Permission modes | Supervised / Auto-accept edits / Auto / Full access, per thread, per project, with an admin ceiling | `docs/user/permission-modes.md`, `DispatchModeLimit.ts` |
| Portable handoffs | Switching provider or forking where the provider cannot resume: a budgeted selection of saved history, never an agent-written summary, with references to what was omitted | `docs/user/portable-handoffs.md` |
| Thread states | Pin, settle, snooze, archive, drag between sections, undo toast on each, auto-settle | `docs/user/thread-sidebar.md` |
| Working fold | Threads that are working or waiting on work collapse into a "Working" section | `docs/user/thread-sidebar.md` |
| Scratch threads | A thread with no project, in `~/.t3/scratch/<date>-<words>-<id>`, later "Turn into project" | `docs/user/thread-sidebar.md` |
| Prompt links | A URL that opens a draft; never runs, never changes mode, branch or worktree | `docs/user/prompt-links.md` |
| Triage | The user's own agent reads logs and state, checks the source for that version, and drafts an issue | `docs/user/triage.md` |
| Outside agents | An MCP server so agents T3 did not start can read projects and message threads | `docs/user/outside-agents.md` |
| Usage | Pooled subscription limits per provider with reset credits and widget support | `docs/user/usage.md` |
| Worktree held for a limit | Cleanup skips a worktree whose run is still owed a reset | `storageCleanup.ts:163-189` |

### 4.4 What we could adapt

1. The queue/steer design in section 2 — the highest-value item in this report.
2. The per-thread limit recovery banner in section 3.
3. Thread settle/snooze with undo, mapped to our "Needs you" bell rather than a new sidebar concept (`ui/NeedsYou.tsx`).
4. A **"Working" fold**: conversations with a live turn collapse into one group and come back when they finish, fail or need an answer (`docs/user/thread-sidebar.md`). Our sidebar already carries per-conversation state; folding the busy ones is a smaller change than a new state and it keeps a busy team readable. Their worktree-cleanup guard (section 8, row 28) is not needed for us: our cleanup is manual, confirmed and recomputed from live conversations (`worktree_cleanup.rs:1-15`).
5. Scratch conversations without a project, which we refuse today: `sendPrompt` needs a project and the daemon resolves one (`team::default_conversation_project`). A "No project" option would remove friction for the "quick question" case and maps onto our own folder-per-conversation idea.
6. Prompt links as a *draft-only* deep link. Our preview harness already parses query flags; a `retinue://new?prompt=…` that only fills the composer is cheap and keeps "a link never runs anything".

**What not to copy.** Their event-sourcing is elegant but we already get the properties we need from `clientMessageId` idempotency, `delivery_unknown`, and a durable queue; rewriting the daemon's dispatch loop to events + outbox is a multi-wave change with no user-visible payoff. Their multi-client RPC is aimed at a web/phone product; our remote gateway already has the tier model they do not (theirs authorises per method too, but with less granularity — `docs/internals/environment-auth.md`).

## 5. OpenBot (`nightly-labs/openbot`, PolyForm Noncommercial 1.0.0)

### 5.1 What it is

A local-first Electron workspace for "persistent AI teammates": each agent has its own workspace, session and queue; agents can message each other, share channel chats with a task owner, use an embedded browser, and optionally drive the desktop through Computer Use (`README.md:7-30`). It is explicitly a development preview, and it says so about permissions: agents run with full access and no per-action confirmations after a first-launch consent.

### 5.2 Architecture in brief

- **Electron main process owns state; renderer renders.** Queues, agent sessions, provider clients and the SQLite database live in `src/backend/`; the UI is in `src/renderer` and `packages/ui` (`src/backend/openbot-database-schema.ts:261-287`).
- **One turn per lane.** An agent's own chat, a task thread and a channel thread are separate lanes, each with one running turn (`src/backend/agent/drain-scheduler.ts:295-307`).
- **Deliveries, not messages, are queued.** Every message becomes one `projection_deliveries` row per recipient with a status of `queued | starting | running | completed | failed | interrupted | cancelled` (`packages/contracts/src/ipc-queue.ts:7-16`), written through `MailboxStore` in single transactions (`src/backend/mailbox-store.ts:283-304`).
- **Provider descriptors are compile-checked.** Thirteen provider ids each need a descriptor row, enforced by `satisfies Record<AgentProviderId, …>` — adding an id without a row fails the build, not a test (`packages/contracts/src/agent-providers.ts:89-207`).
- **Tools reach the model through one `search/describe/call` wrapper** rather than every schema in the prompt (`src/backend/agent/tool-catalog.ts:144-180`).

### 5.3 Notable features

| Feature | What it is | Where |
| --- | --- | --- |
| Queue per agent | Durable deliveries; batched into one turn when several wait; failures redact echoed keys | `mailbox-store.ts:1717-1748`, `:2233-2246` |
| Steer, and its fallback | `turn/steer` with the delivery id; on failure the row is restored *in place*; non-steerable providers mark `steerFallback: "provider-unsupported"` | `queue-controls.ts:246-324`, `QueuePanel.tsx:508-518` |
| Send now without steer | Reorder the message to the head, then interrupt the running turn so it runs next | `src/renderer/src/turns.tsx:435-475` |
| Crash recovery | On boot, `starting`/`running` rows are reconciled by asking the provider, defaulting to `interrupted` on doubt | `src/backend/agent/boot-recovery.ts:49-177` |
| Channels and task ownership | Members, a lead agent, tasks with `ownerAgentId`, dependencies, `assignmentCount`; Stop/Resume/Reassign | `packages/contracts/src/ipc-chat-channels.ts:110-162`, `channel-service.ts:1103-1134` |
| Channel tools | `channel_assign`, `channel_transfer`, `channel_reply`, `channel_result`, `channel_cancel` | `channel-tools.ts:41-67` |
| Run a queued message as a task | Promote a follow-up into its own task thread | `agent-service.ts:5246-5278` |
| Context compaction | Auto at 80 % with a growth rule; two ineffective compactions in a row stop it and tell the user | `src/backend/agent/context-compaction.ts:36-53,354-443` |
| Browser and Computer Use | CDP-driven Electron browser per agent profile, ~35 browser verbs, consent/takeover; `cua-driver` exposed to every provider through one MCP entry | `browser-tools.ts:20-407`, `docs/architecture/computer-use.md:5-9` |
| Per-agent busy mode | `busyMessageMode` (queue or steer) per agent, with an app default | `ipc-queue.ts:18-33`, `agent-service.ts:5783` |
| Permissions, precisely | Stored access is `full` or `workspace`; the three UI levels derive from it; approvals exist and `defaultAutoApprove` is on; Codex gets `approvalPolicy: "on-request"` with a sandbox mode | `ipc-agent-admin.ts:41-75`, `thread-lifecycle.ts:781-790`, `ipc-approvals.ts:81-84` |
| Theme palette files | Drop `<name>.json` into `<data>/themes/` and it hot-reloads | `README.md:83-114` |
| Marketplace | A checked-in catalogue of MCP plugins and skills, installed through `openbot://` links | `marketplace/plugin-catalog/catalog.json`, `README.md:77-79` |

### 5.4 What we could adapt

1. **Send now without steering: reorder, then interrupt** (`turns.tsx:435-475`). This is the cheapest version of the headline feature and it works for every provider: move the item to the front, stop the turn, let the queue drain. We already have both halves (front-move in `queue.rs:326`, "Stop and send instead" in the composer); joining them behind one button, with a confirmation that names the interrupted work, is small.
2. **Mark the fallback on the row** (`steerFallback: "provider-unsupported"`, shown in the queue panel). If we cannot steer, the row should say why, once, instead of silently behaving differently per provider.
3. **Per-bot busy-send mode.** Our `composer.busySendMode` is global. A per-bot override fits "bots are employees with habits" and is one stored field.
4. **"Run this as a task"** on a queued message. We have a task board and a delegation ledger; turning a follow-up into a tracked task instead of a chat message is a natural extension and does not need a new concept.
5. **Batch several waiting messages into one turn.** Their mailbox sends messages that arrived together as one turn (`mailbox-store.ts:1717-1748`). For us this would reduce turn overhead when an owner types three quick corrections; the risk is a muddled turn, so it should be opt-in or limited to messages with no commands/attachments.
6. **Compile-time completeness for provider rows.** Our `providers.rs` table and `runtime.capabilities` are kept honest by tests; OpenBot's `satisfies Record<…>` makes a missing row a build failure. A Rust equivalent (an exhaustive `match` on a `Provider` enum) would fail earlier.

**What not to copy.** Their default posture — full access with automatic approvals on — is the opposite of ours (`AGENTS.md`: permissions belong to the daemon and Full access needs care). The embedded browser and Computer Use are a separate product surface with a large security budget; we do not have them and should not add them without an owner decision and a threat-model section. The PolyForm licence means none of their code may be reused.

## 6. Nuphos (`nuphos/nuphos`, Apache-2.0)

### 6.1 What it is

"The agent workspace for your team": coding agents that run on a machine that outlives the laptop, in sessions the whole team can join (`README.md:5-20`). It is a hosted service with a self-hostable deployment: Bun/Hono backend with MongoDB, Electron desktop, SwiftUI iOS and Compose Android clients, and containerised agent runtimes that wrap Claude Code and Codex behind ACP (`README.md:44-52`, `apps/runtime/README.md`).

### 6.2 Architecture in brief

- **The backend owns sessions; the runtime executes turns.** The runtime image is a vendored `openab` ACP gateway plus one CLI and its adapter, pinned by SHA-256 so an upstream upgrade fails the build until reviewed (`apps/runtime/README.md:12-55`).
- **One claim per session, one FIFO queue behind it** (`apps/backend/src/lib/agent/run-admission.ts:20-22,83-141`; `apps/backend/src/routes/agent/claim-or-enqueue.ts:19-43`). Everything queued — a second person, an agent, a trigger — waits behind whatever arrived first (`session-queue-turn.ts:1-4`).
- **Attribution is server-authored**: `sender{type,id,displayName,avatarURL}`, optional device, `onBehalfOf` for agent tokens, injected into the prompt as a JSON envelope with the instruction that other members' text is data, never an instruction or an approval (`apps/backend/src/lib/agent/message-metadata.ts:6-35,161-197`).
- **Runtimes are swapable**: history-based moves work across providers; workspace moves archive the directory and are refused for local targets (`conversation-runtime-move.ts:25-208`).

### 6.3 Notable features

| Feature | What it is | Where |
| --- | --- | --- |
| Shared sessions | Viewer/reply/owner grants, invite list, speaking silently joins, machine-level presence only | `access.ts:9-98`, `participants.ts:14-66`, `message-attribution.ts:98-101` |
| Queue with steer | Numbered strip: "Runs after the turn ahead", "Send into the running turn", "Take back"; **only the turn's own actor may steer into it** | `routes-conversation-runtime.ts:56-99`, `routes-conversation-queue.ts:1-62`, `QueuedStrip.tsx:53-101` |
| Move a session | `history` (provider-agnostic, 120k-char newest-first preamble) or `workspace` (archive, same provider); refuses while busy with 409 | `conversation-runtime-move.ts:25-208`, `preview-history-preamble.ts:12-49` |
| Split panes | Binary tree layout, edge drop zones, centre swaps, `Cmd+D` / `Cmd+Shift+D`, drag a chat from the sidebar | `splitLayout.ts:1-163`, `WorkspaceSplits.tsx:137-148` |
| Integration panes | Hardcoded React views per provider; the agent never drives the GUI, it calls server-side proxies that inject credentials, or gets minted short-lived credentials | `builtin-skills/skills/grafana/SKILL.md:10-57`, `agent-kubeconfig/render.ts:18` |
| Database access policy | `disabled` / `metadata-only` / `read-only`, with verified read-only capability; writes go through approved Plans | `models/database.ts:9-90` |
| Triggers | Cron (BullMQ) and webhooks with a constant-time shared secret, cooldown and `200 {reason:'rate_limited'}` | `trigger-scheduler.ts:42-121`, `routes/webhooks.ts:34-66,252-266` |
| Team memory | One document per playbook revision with embedded cases; two tools (`save_memory`, `memory_get`); `$text` search across personal and team pools | `memory-native/tools.ts:1-8`, `tools-get.ts:24-74` |
| Team skills | Scoped object store, ETag manifest sync into `.claude/skills` from a signed expiring URL | `team-skills.ts:46-64`, `runtime-skills-token.ts:7-27`, `image/sync-skills.sh:25-37` |
| Cost display | Four-rate table; **cost undefined for unknown models so "$0" is never faked** | `model-pricing.ts:3-64` |
| Mobile auth | PKCE + loopback one-time delivery code; session secret hashed, 14-day idle / 30-day max, revocable | `native-auth.ts:9-54`, `docs/sign-in-sessions.md` |
| MCP both ways | Nuphos as an MCP server for outside agents, with `nuphos_ask`/`nuphos_check` timeout recovery | `routes/mcp.ts:1-58` |

### 6.4 What we could adapt

1. **Steering restricted to the turn's own actor.** Our side conversations and sub-bots can already wake a parent, and the remote gateway lets several people send; Nuphos makes the rule explicit: a message from anyone else queues, it does not join. That is a sentence we could adopt for remote and for automation-originated messages, and it costs no code today because our queue already exists.
2. **A busy move is a 409, not a half-move.** Their session move refuses while a turn runs (`conversation-runtime-move.ts:114-120`). Our machine handoff already refuses to move a conversation that is running (`machines::refuse_if_moved`), but the *reason* is worth surfacing the same way: "the turn must finish first".
3. **History preamble sized to a budget, newest-first, with the compaction summary first** (`preview-history-preamble.ts:12-49`). Our cross-machine import and context handoff build the same kind of selection; their explicit 120k-char budget and ordering is a cleaner rule than our ad-hoc carry.
4. **Team skills that sync instead of being copied per bot.** Our skill library installs into the pool and attaches per employee (`skill_library.rs`, `project.hire` skills). Nuphos syncs from a manifest with ETags on session open, so a grant reaches every agent and updates land without re-hiring. For a small team of bots, a "sync on conversation open" check is a small addition; a signed expiring URL is already how our remote devices fetch things.
5. **`metadata-only` database access as a third policy.** Our permission rules are per-tool patterns; a named three-value policy for a database class of tools ("disabled / metadata only / read only") is easier to explain to an owner than a regex, and matches how we already present `approvalMode`.

**What not to copy.** Their GUI panes are a product surface we do not have (Kubernetes, Grafana, Linear). Building stock panes is a different product with a large surface; the transferable part is the credential policy — agent tools that inject credentials server-side, never handing the token to the model. Our `secrets.rs` already refuses to return a value, which is the same principle, and the specific panes are not worth it.

## 7. Monocode (`hardbeat920/monocode`, MIT)

### 7.1 What it is

A Tauri + React desktop GUI where "tabs are sessions" over eleven coding CLIs (Claude Code, Codex, Cursor, Grok Build, OpenCode, Antigravity, Pi, omp, fx, Hermes Agent, Devin), with split panes, Git worktrees, an in-app editor and diff view, an agent-facing app CLI, and — the part most relevant to us — a second, native Rust + GPUI front end that shares the same database (`README.md:10-32`).

### 7.2 Architecture in brief

- **One database, two front ends.** `crates/monocode-core` holds provider transports, persistence, accounts, Git/files and remote connections with **no Tauri or GPUI dependency**; `crates/monocode-gpui` is the native shell (`docs/native/migration.md:7-23`). Sessions, queue, context counters and workspace snapshots live in SQLite (`crates/monocode-core/src/store/shared_schema.sql:1-22`).
- **The renderer is the orchestrator.** `src/app/App.tsx` is 14,642 lines and owns queue decisions and the limit timer; the Rust core provides transports and storage. That is the opposite of Retinue's split, and it is why their auto-resume needs the app open.
- **Size, as evidence for the native question**: raw line counts (tests included) are 73,529 for `monocode-core`, 195,405 for `monocode-gpui`, and 14,642 for the React `App.tsx`. A GPUI front end is not a smaller app; it is a second app. Their own parity table lists a long tail still unverified per platform (`docs/native/parity.md:3-60`).

### 7.3 Notable features

| Feature | What it is | Where |
| --- | --- | --- |
| Queue with steer-the-head | Described in 2.2 | `messageQueue.ts:37-99` |
| Resume at reset | Described in 3.3 | `usageLimit.ts:57-140` |
| Account switch on a limit | Re-point a limited session at another account, carrying the native transcript when possible | `usageLimit.ts:33-55` |
| Second opinion | Hand a finished turn to another provider to review, with the request, report, edited files and optional CI context, bounded | `secondOpinion.ts:104-150` |
| `/btw` | A read-only side question about one turn, with a bounded transcript snapshot and an explicit "do not change files or steer the parent" instruction | `btw.ts` |
| `/operator` | An app CLI the agent may call to start sessions, split panes, read/write notes, list models; enabled per thread by typing `/operator`, and only during a turn | `docs/agent-access.md` |
| Quick composer | A separate window on a global hotkey (`Command+Shift+Space`) for starting a turn without finding the app | `quickComposerShortcut.ts` |
| Remote hosts | SSH to another machine, download and verify the host package, install a service, pair a device, forward a loopback port; the host keeps sessions alive when the laptop closes | `docs/remote-access.md` |
| Monos | Long-lived agents across projects, with memory files, habits and scheduled work | `src/features/monos/model/mono.ts:36-58` |
| Memory hygiene | Append-one-dated-bullet, superseded not deleted, and every entry redacted against credential patterns before it reaches disk | `monoMemory.ts:11-40` |
| Checkpoints | Per-turn file checkpoints with exact diffs, `undoable` when no outside change would be lost | `checkpoint.ts:1-20` |
| Inbox | A CI-failure inbox with repair context and a second-opinion path | `src/features/inbox/` |

### 7.4 What we could adapt

1. **Second opinion** (section 1, row 7). We already have every runtime installed, a bot roster, and a delegation ledger; a lighter "ask <bot> to review this turn" needs a bounded prompt and a new conversation, not a delegated task. It would land as one more action on a finished turn, beside the existing message actions, with the daemon building the prompt so no transcript text is assembled in React.
2. **`/btw`** (row 9). Our side conversations are bot-to-bot and durable; a `/btw` is a one-off read-only question that never enters the parent transcript. The vocabulary already exists (`#` project, `@` bot); `/` is free for commands (`commands::directive`).
3. **Account switch on a limit** (row 6). Pairs with the limit banner in section 3.
4. **The memory redaction rules** (`monoMemory.ts:22-40`). Our `memory.rs` already redacts nothing at that granularity; adopting their pattern list is small and aligns with the "never log a credential" rule.
5. **Quick composer**. Our companion is bot-scoped; a global "compose to any bot" panel is a different, smaller surface and could reuse `ComposerControls`.
6. **The native-app evidence.** Their split (core crate with no UI dependency, native front end reading the same DB) is exactly the shape Retinue's `docs/research/rust-native-ui-port.md` suggests trying, and their parity doc is the honest cost estimate: a full second UI, not a performance tweak.

## 8. Every candidate idea

Effort: **S** = a day or two in one area; **M** = a wave touching daemon + UI + tests; **L** = a multi-wave change or an adapter rewrite. The last column names the `AGENTS.md` rule or risk that constrains each idea, if any.

| # | Idea | Source | Where it would land in Retinue | Effort | Risk or rule conflict |
| --- | --- | --- | --- | --- | --- |
| 1 | "Send now" steers into the running turn | T3, Monocode | `queue.rs` `send_now`, `main.rs` steer path, `PromptQueue.tsx` | M–L | Double delivery; must reuse `sendId`/`clientMessageId`. The daemon owns the decision |
| 2 | Promote + interrupt fallback when a provider cannot steer | OpenBot, T3 | `queue.rs` (`send_now` → front + `cancel_session_turn_if`), composer copy | M | The interrupted work is lost; needs an explicit confirmation and a note |
| 3 | Real steering for Claude and OpenCode | T3, OpenBot | `retinue-adapter-claude`, `retinue-adapter-acp`/`-opencode`, `provider_can_steer` | L | Must not scrape the terminal; only protocol-level steering (stream-json input, ACP `delivery: steer`). Check the pinned ACP schema first |
| 4 | Per-conversation "Resume at reset" and "Snooze until reset" | T3, Monocode | storage `limited_until` neighbourhood, `retry.rs` sweep, new banner, `tiers.json` | S–M | Unattended runs must stay on the automation rails (`is_unattended`) |
| 5 | Banner that says "Resumes" vs "Resets" and owns both actions | T3 | `TeamMessages.tsx` / new `UsageLimitBanner.tsx` | S | Copy must not promise a resume that the cap will refuse |
| 6 | Edit a queued message in the composer, with attachments | T3 | `App.tsx` composer, `PromptQueue.tsx`, `queue.rs` `update` (accept attachments) | S | Attachments must be re-validated against the provider, as `queue.rs:266` already does |
| 7 | Switch a limited conversation to another account | Monocode | `accounts.rs`, `retry.rs`, the limit banner | M | Credentials never leave the daemon; no copied provider auth |
| 8 | Second opinion from another provider on a finished turn | Monocode | new daemon helper + message action; reuses `session.prompt` on a bot | M | Prompt built in the daemon, bounded, no transcript in React |
| 9 | `/btw` read-only side question about one turn | Monocode | `commands::directive`, a read-only conversation like `link.subbot` | M | Must not write to the parent or run write tools |
| 10 | Run a queued message as a task | OpenBot | `tasks.rs`, `PromptQueue.tsx` row menu | S–M | The task board and the queue must not both own the message; one claim |
| 11 | Queue-first shortcut, and "opposite action" per message | T3 | `ui/shortcuts.ts`, `shortcuts.bindings` | S | None |
| 12 | Per-bot busy-send mode | OpenBot | `composer.busySendMode` → per-agent field | S | A stored setting per bot must not travel in backups in a surprising way (`backup.rs` allowlist) |
| 13 | Quick composer on a global hotkey | Monocode | new small window or the companion's draft pane, `src-tauri` | S–M | `src-tauri` owns the hotkey; must not add a new permission surface |
| 14 | Settle a conversation (done, with auto-settle and undo) | T3 | `presentedSessionState`, `NeedsYou.tsx`, `ConversationsPanel.tsx` | M | Must stay distinct from archive; auto-settle needs an inactivity rule the owner can see |
| 15 | Scratch conversations without a project + "turn into project" | T3 | `team::default_conversation_project`, `sendPrompt` | M | Every conversation today belongs to a project; the machine and worktree paths assume it |
| 16 | Prompt links that only fill the composer | T3 | `tauri.ts` deep link handler, `App.tsx` | S | A link must never run, change mode, branch or worktree |
| 17 | "Copy prompt link with repo and model" | T3 | message menu in `App.tsx` | S | Must not leak a host path into the URL |
| 18 | Provider descriptors that fail the build when a row is missing | OpenBot | `providers.rs`, `runtime.capabilities` | S | Rust has no `satisfies`; use an exhaustive match over an enum |
| 19 | Mark `steerFallback: provider-unsupported` on a queue row | OpenBot | `QueuedPrompt` DTO, `PromptQueue.tsx` | S | None |
| 20 | Batch messages that arrived together into one turn | OpenBot | `queue.rs` `drain_next` | M | Muddled turns; limit to plain text, never commands or attachments |
| 21 | Ask the provider what happened to an interrupted delivery on boot | OpenBot | `recover_restart_state`, `delivery_unknown` | M | Only where the provider exposes a turn-state call; otherwise keep `delivery_unknown` |
| 22 | Actor-only steering (someone else's message waits) | Nuphos | remote gateway policy, `delegations.rs` wake path | S | Needs a clear sentence in the UI; could confuse the owner in their own bot |
| 23 | History preamble with an explicit character budget, newest-first | Nuphos | `handoff.rs` / machine import | S | Never truncate the owner's new request |
| 24 | Team skills that sync on conversation open | Nuphos | `skill_library.rs`, `project.hire` | M | A synced skill must still be refusable per bot |
| 25 | A three-value database/tool access policy | Nuphos | permission rules presentation | M | Overlaps `permission.rules`; do not create a second authority |
| 26 | Memory redaction patterns before write | Monocode | `memory.rs` | S | Never store a credential; aligns with rule 3 |
| 27 | Phone actions carry a stale-state token | Monocode | `remote/` | S | Our device tokens are per device; this is per action |
| 28 | Worktree cleanup held while a run is owed a reset | T3 | `worktree_cleanup.rs` | — | **Not needed**: our cleanup is manual, confirmed and recomputed from live conversations (`worktree_cleanup.rs:1-15`) |
| 29 | Theme palettes loaded from files | OpenBot | `ui/appearance.ts` | — | Conflicts with the two-look rule (Haze and Stone only) |
| 30 | Embedded browser and Computer Use | OpenBot | n/a | L | New security surface; needs an owner decision and a threat-model section first |
| 31 | Event-sourced orchestration with outbox effects | T3 | `main.rs` dispatch | L | No user-visible payoff now; we already have idempotency and `delivery_unknown` |
| 32 | GPUI/native front end sharing the core crate | Monocode | `apps/native`, `docs/research/rust-native-ui-port.md` | L | A second app: 195k lines of GPUI against 14.6k of React in their repo; not a performance tweak |

## 9. Sources

### 9.1 Repositories read (full commit hashes)

| Project | Repository | Commit | Date | Licence |
| --- | --- | --- | --- | --- |
| T3 Code | https://github.com/pingdotgg/t3code | `1a2a4a5e9684585c31eb55c1974fb32f8b1db2b7` | 2026-11-10 | MIT |
| OpenBot | https://github.com/nightly-labs/openbot | `712e797303c5d41ee256c8360645718a0beb13b2` | 2026-11-10 | PolyForm Noncommercial 1.0.0 |
| Nuphos | https://github.com/nuphos/nuphos | `efee2c028fcb5f97d57d2d036342a7eab12e4cd1` | 2026-11-10 | Apache-2.0 |
| Monocode | https://github.com/hardbeat920/monocode | `7e5a9e3690d5b96a7d93ebf884397835e57b9b7e` | 2026-11-09 | MIT |

Shallow clones (`--depth 1`), so no earlier history was read.

### 9.2 How I identified Monocode

The owner gave only the name. A GitHub repository search for "monocode" returns 71 results; the only one in this space is `hardbeat920/monocode` ("A GUI for your coding agents", Rust, ~3.9k stars, updated the day before), and its site `usemono.dev` describes "A desktop UI for the coding agents already on your machine". I settled on it because: (a) it is the same class of product (a Tauri desktop UI over Claude Code, Codex, Cursor, Grok Build, OpenCode, Pi, omp, fx), (b) the name, repository, site, and a third-party directory listing agree, and (c) a phone companion (`folkykyle01/monocode-mobile`) and reviews refer to it as *the* MonoCode. Other candidates were a font (`aaronmbos/monocode`), a colour scheme (`Mono-Code-Scheme/*`) and an unrelated OCR project; none is a coding-agent app. If the owner meant something else, this section is where the substitution happened.

### 9.3 Public material (not code)

| Source | Link | Used for |
| --- | --- | --- |
| T3 Code site | https://t3.codes/ | What it claims to be ("the open-source control plane for coding agents") [public] |
| "A deep dive into T3 Code", Flavio Copes, 4 Nov 2026 | https://flaviocopes.com/t3-code/ | Independent confirmation that "the queue lives on the server. Queued messages survive restarts. You can edit, reorder, or steer them" and that 0.1.0 added auto-resume after usage limits; also permission modes, worktrees, settle, `t3.json` [public] |
| "T3 Code review: the open-source agent control plane, tested", Continuum Code | https://continuumcode.ai/guides/t3-code-review/ | Positioning and "very early" status [public] |
| "T3 Code Review (November 2026)", Retrodeck | https://retrodeck.ai/guides/t3-code-review | "worth using if you run coding agents on remote machines or switch between providers often"; "early alpha" [public] |
| Theo's post on 20k stars | https://x.com/theo/status/2092941730517746149 (cited by Flavio) | Adoption context; **not fetched directly** [unchecked] |
| OpenBot README | https://github.com/nightly-labs/openbot | Feature list and the full-access warning [docs] |
| "OpenBot by Nightly Labs", explainx.ai, 26 Oct 2026 | https://explainx.ai/blog/nightly-labs-openbot-desktop-workspace-codex-claude-grok-agents-danger-full-access-polyform-noncommercial-2026 | Star growth, licence reading, the danger-full-access critique, server/Docker routes [public] |
| OpenBot site | https://openbot.run/ | Product claim ("runs AI agents as a team on your own computer") [public] |
| Nuphos site | https://nuphos.ai/ | Product claim [public] |
| Nuphos in the Agent Orchestrators directory | https://agent-orchestrators.com/tools/nuphos/ | Stars (~386) and category; it lists the licence as "Other", which disagrees with the repository's Apache-2.0 `LICENSE` — I trust the file [public] |
| Nuphos on X, Desktop 0.80 | https://x.com/NuphosAI/status/2110339842529755418 | Release context; **not fetched directly** [unchecked] |
| MonoCode site | https://www.usemono.dev/ | Product claim [public] |
| MonoCode review, FollowAgents, 7 Nov 2026 | https://followagents.com/en/agents/monocode | MIT, Tauri, eight agents, the warning that the native preview writes the installed database [public] |
| "MonoCode GitHub Overview", YouTube | https://www.youtube.com/watch?v=6pYfAK9USUI | Timestamps name architecture boundaries, provider detection, split panes; **no transcript was retrievable**, so only the chapter titles were used [public, partial] |
| DeepWiki: monocode | https://deepwiki.com/hardbeat920/monocode | Cross-check that it is an orchestration layer over provider CLIs [public] |

### 9.4 What I could not verify

- **Video content.** No transcript for the MonoCode video; only its chapter titles were visible. Any claim about what the video *shows* would be unverified.
- **Social posts.** The two X links (Theo, Nuphos) came from other pages; X did not load for me, so their text is second-hand.
- **Live behaviour.** Nothing here was run. A claim like "steering works on Claude" is a claim about their code and capability flags, not a test I performed. Our own adapters' ability to steer (`AGENTS.md` allows it) is a design question for the implementation wave, not a finding.
- **T3's mobile queue.** I read the mobile queue control (`apps/mobile/src/features/threads/ThreadQueueControl.tsx`) only enough to confirm the same three verbs exist; I did not trace its command wiring.
- **OpenBot's agent-to-agent security review.** I read the code paths but not `SECURITY.md` or the threat model end to end; the statement that their defaults are riskier than ours rests on the permission code cited in 5.3, not on an audit.

**Proposed skill update (not applied, because this run was research-only).** Add one line to `.claude/skills/retinue-development/references/codebase-map.md`, under Docs → Research: "Competitors (2026-11-10): `docs/research/competitor-adaptation-2026-11.md` studies T3 Code, OpenBot, Nuphos and Monocode. It is the reference for the queue/'Send now' and limit-resume designs proposed here."
