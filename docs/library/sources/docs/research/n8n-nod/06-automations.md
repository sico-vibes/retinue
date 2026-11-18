# nod (n8n labs): automations and unattended work, against Retinue's automations and task board

> **What this is:** A research report on how n8n's `nod` defines, stores, starts, limits, reports and fences scheduled and triggered work, compared with Retinue's automations (`automation.*`), its scheduled board tasks and the 12 October 2026 rule that unattended work brings its own caps and never uses bypass.
> **Status:** current
> **Last updated:** 2026-11-16

**Source and licence.** Cloned shallow from [github.com/n8n-io/nod](https://github.com/n8n-io/nod) at commit [`47cfee3`](https://github.com/n8n-io/nod/tree/47cfee3fe77279e1d543a367131d3ff2c1202e18) (16 November 2026) and read at `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-sandbox`; nothing was installed, built or run. Links below are permalinks at that commit; Retinue is cited as `path:line`. The repository root is **Apache 2.0** ([`LICENSE`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/LICENSE#L1-L3), `package.json:4`), so a copy with attribution would be legal; this report still recommends **designs, not code**. `NOTICE` shows nod also ships GPL-3.0-or-later speech code — take none of it. This is research only; no Retinue code was changed and nothing was committed.

**How this was read.** nod's code (schedules, proactivity, tasks, ledger, runtime tool gate) plus its own user docs, which are unusually precise and were used to check my reading of the code. Where I rely only on the docs I say so. On our side I read `automations.rs`, `tasks.rs`, `webhooks.rs`, the unattended rails in `main.rs`, the storage schema, `SchedulesView.tsx` and the automations guide.

## Summary

1. **Three kinds of unattended work, not one.** nod has a schedule (five-field cron or one exact instant in an IANA zone), a task's own `nextCheck` ("check back Friday"), and a background observer that polls connected apps. Retinue has the first two — as separate features (`automation.*` and a scheduled board task) — and no observer.
2. **Every occurrence is a durable row and a queued job.** `schedules` plus `schedule_runs`; the job carries the schedule's `revision` and due instant, so an edited or stale job is a no-op, and a boot pass re-queues whatever is missing. Retinue polls `next_run_at` every 10 seconds, coalesces missed runs and records each outcome on a run row.
3. **The limits are the design.** One run at a time per schedule; a global monthly spend limit; a per-task spend check-in (5 USD by default) that stops tools and forces one question; a separate preparation allowance; quiet hours; a pause switch. Retinue has single flight, a 1-to-3 concurrency cap, a 15-minute busy deferral, a global pause, a per-run runtime cap — and **no cost limit at all**, because budgets were retired.
4. **What a run may do is one prompt rule plus tool-level fences.** nod keeps its observer read-only with an explicit allowlist and lets its trust check gate every outside action. Retinue clamps the mode in one function (`is_unattended`), auto-denies anything the permission classifier would not allow, and dismisses questions.
5. **The gap worth closing is reporting.** nod shows each run's cost, a projected monthly cost, a "due at X, started at Y, N runs skipped" note **inside the run's own prompt**, and a chat message when a run fails. Retinue records status and reason on the run row but tells neither the model nor the owner, except inside the app.

## 1. How nod does it

### 1.1 The three kinds of unattended work

| Kind | What starts it | Where it is defined |
|---|---|---|
| Schedule | pg-boss job at the due instant, or **Run now** | `schedules` row ([`schedules.ts:509-552`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L509-L552)) |
| Task check-back | the sweep, when `tasks.next_check` is due | `tasks` row plus its latest turn ([`tasks.ts:618-646`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/tasks.ts#L618-L646)) |
| Observer | due source poll, a change token, or app activity | `proactive_sources` and `proactive_issues` ([`proactivity.ts:1337-1545`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/proactivity.ts#L1337-L1545)) |

Inbound triggers: a chat message from the owner (Telegram, WhatsApp or web), never a webhook. There is no inbound HTTP trigger for a schedule at all; nod's routes are the app's own API ([`schedules.ts:1239-1282`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L1239-L1282)). Apps are *pulled*, with change tokens where the app offers them ([`proactivity.ts:1396-1472`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/proactivity.ts#L1396-L1472)).

### 1.2 Main files

- `src/server/schedules.ts` (1,249 lines): the schedule store, the pg-boss queue, dispatch, run history, observations, cost projection.
- `src/server/tasks.ts` (984 lines): the task ledger, the status function, `nextCheck` wake, `scheduleDraft`.
- `src/server/proactivity.ts` (1,635 lines): the observer: source claims, read limits, quiet hours, prepared work, delivery policy.
- `src/server/ledger.ts`: usage rows, the monthly limit, the preparation allowance with reservations, the per-task check-in.
- `src/server/work-admission.ts` (197 lines): a global "pause and drain" gate around every HTTP route.
- `src/runtime/index.ts` and `src/runtime/tools.ts`: the per-turn tool gate (observer allowlist, spending blocks).
- `src/runtime/prompt.ts`: what a background turn is told, and prompt ordering for cache.
- `src/web/pages/schedule.tsx`, `src/web/pages/tasks.tsx`: the two screens.
- `docs/src/content/docs/use/schedules.md`, `spending.md`, `tasks.md`: the behaviour, written for the owner.

### 1.3 Data flow of one scheduled run

1. **Define.** The owner (or the assistant, through the trust check) saves a title, prompt, `kind: once|recurring`, a five-field cron **or** a `runAt` with offset, and an IANA `timeZone` ([`schedules.ts:40-45`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L40-L45)). Timing is validated by asking the scheduler to preview it, so an invalid zone or cron is refused before storage ([`schedules.ts:226-246`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L226-L246)).
2. **Queue, in the same transaction as the write.** `enqueue` sends a job `{scheduleId, revision, scheduledFor}` with `startAfter: due`, `retryLimit: 5`, `retryDelay: 15`, backoff, and `expireInSeconds: 60`, then stores `job_id` and `next_run_at` ([`schedules.ts:309-335`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L309-L335)).
3. **Dispatch, defensively.** The worker re-reads the row under `FOR UPDATE` and refuses to run when the job's id, `revision` or due instant no longer match — an edit or a pause between queueing and firing is a no-op ([`schedules.ts:442-455`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L442-L455)).
4. **Concurrency by kind.** If a run is active: a `once` schedule waits 60 seconds and tries again; a recurring one **skips this occurrence and queues the next immediately** ([`schedules.ts:457-476`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L457-L476)).
5. **Watch skip.** A recurring schedule with a `watch` list fingerprints the watched accounts and skips the run when nothing changed and the last run succeeded ([`schedules.ts:477-495`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L477-L495)).
6. **Start.** `startRun` enqueues a **background** turn with a fixed prompt wrapper and `sourceId: schedule:<id>:<jobId>`, then inserts the run row ([`schedules.ts:355-391`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L355-L391)). The wrapper says: deliver what this asks for in this run, and message the owner only about a useful result, a problem, or a necessary question.
7. **Lateness is told to the model.** A run more than 10 minutes late gets a line in its own prompt: due at X, started at Y, and how many later runs were skipped, capped at "100 or more" ([`schedules.ts:279-307`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L279-L307)).
8. **Boot.** At startup every schedule is reconciled in a transaction: a live job is kept, otherwise it is cancelled and re-queued at its next occurrence ([`schedules.ts:558-576`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L558-L576)).
9. **Duplicates.** A create from the owner or the assistant that matches an active schedule by title or prompt, timing, zone and delivery chat returns the existing one instead of a copy ([`schedules.ts:1002-1033`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L1002-L1033)).

### 1.4 Limits

| Limit | Where | How it behaves |
|---|---|---|
| One run per schedule | [`schedules.ts:344-353`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L344-L353) | Run-now returns 409 while a run is queued, running or waiting for the owner. Recurring runs skip; one-offs wait. |
| Job retries | [`schedules.ts:553`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L553) | 5 attempts with backoff, 60-second expiry. This is queue delivery, not the run's work. |
| Monthly spend | [`ledger.ts:398-421`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/ledger.ts#L398-L421) | At the limit, schedules, tasks and background work stop **before** the model call; chat still answers, only to change the limit (`docs/.../spending.md:64-87`). |
| Preparation allowance | [`ledger.ts:423-487`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/ledger.ts#L423-L487) | A separate monthly pot, held by an advisory lock, with an estimated maximum reserved before each request. A request that cannot fit does not start, and an unknown price blocks preparation. |
| Per-task check-in | [`ledger.ts:751-774`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/ledger.ts#L751-L774) | Spend since the owner's last message to that task; past the threshold every tool except reporting throws, and the assistant must ask ([`index.ts:457-469`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L457-L469)). |
| Quiet hours | [`proactivity.ts:1089-1113`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/proactivity.ts#L1089-L1113) | The observer releases its claims and returns; new tasks and offers are deferred, except a high-impact offer with a deadline before the window ends (`schedules.md:98-102`). Schedules the owner created still run. |
| Global pause | `proactivePaused`, `work-admission.ts:6-76` | The observer has its own switch, off by default. The app's own update path pauses every route and drains in-flight work ([`work-admission.ts:143-205`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/work-admission.ts#L143-L205)). |
| Runtime | not found | I found no per-run runtime cap. A run ends when its turn ends; the model's own tool loop and the process supervise it. |

### 1.5 Reporting

- **Status, per task:** queued, failed, working, needs you, waiting, scheduled ("checking back"), done, stopped ([`tasks.ts:109-118`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/tasks.ts#L109-L118)). A task is "needs you" when a turn failed, or is waiting with no answer, or has no next check.
- **History, per schedule:** newest first, cursor-paginated, each run with status, error, the first reply's text and preview (up to 4,000 characters), and `costUsd` when the pricing is complete ([`schedules.ts:591-648`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L591-L648)).
- **Projection:** `monthlyUsd` is the mean cost of up to ten recent runs times the occurrences in the next 30 days; a schedule with no runs borrows the newest similar run ([`schedules.ts:698-757`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L698-L757)). The assistant is told to state it when it creates a recurring schedule (`runtime/skills/schedules-and-watches/SKILL.md`).
- **Failure reaches the owner in the conversation.** A failed background turn writes an assistant message that names the work: "I couldn't finish “Scheduled task: …”. <error>" ([`thread.ts:1250-1293`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/thread.ts#L1250-L1293)).
- **Intentional silence is a value, not an absence.** A run that finds nothing ends with `(pass)`-style quiet, and the wrapper asks for a message only on a useful result, a problem or a question.

### 1.6 Safety when nobody is watching

- **The observer is read-only by allowlist, not by mode.** In a proactive turn any tool other than `read_source`, `proactive_issue`, `task`, a spending `ask_user`, and four read actions throws with "This tool is unavailable in proactive work" ([`index.ts:428-445`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/index.ts#L428-L445)).
- **A background turn may only do what its instruction says.** "Outside actions need the schedule or notification instruction. Stay quiet without useful work" ([`prompt.ts:61`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/runtime/prompt.ts#L61)).
- **Authority is stored, not remembered.** A schedule keeps the owner messages and the approved complete definition that authorised it, and an assistant-proposed change that differs from the checked change does not run ([`schedules.ts:879-1000`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/schedules.ts#L879-L1000)).
- **Every outside action is still checked.** Approvals are not switched off for unattended work; the run carries the owner's saved inputs and the trust check decides (`schedules.md:64-66`).
- **Kill switches.** Pause one schedule, stop one task, stop all background work, pause preparation, the monthly limit, quiet hours, and a drain on shutdown ([`tasks.ts:454-472`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/tasks.ts#L454-L472), `docs/.../tasks.md:54-64`).

## 2. How Retinue does it now

### 2.1 What an automation is

A saved prompt plus a five-field cron or one instant, per bot, with `sessionMode` (`new` or `reuse`), an approval mode clamped to `plan` or `auto`, and a `maxRuntimeSecs` between 60 and 14,400 (default 1,800) (`crates/retinue-storage/src/lib.rs:1901-1906`). The RPCs are `automation.create|list|get|update|pause|resume|delete|run_now|runs|preview` (`crates/retinue-daemon/src/automations.rs:497-558`), and `automation.preview` returns up to 10 next runs in UTC and local time.

An assistant can only **propose**: `schedule_task` stores the automation disabled, `plan` mode, `createdBy: agent`, `pauseReason: needs_approval`, and the Schedules page shows it under "Proposed by your bots" with an Approve button (`automations.rs:563-589`, `apps/desktop/src/ui/SchedulesView.tsx:121-167`, `:234-243`).

### 2.2 Storage and starting

Tables `automations` and `automation_runs`, with a partial unique index that allows exactly one `running` run per automation (`crates/retinue-storage/src/lib.rs:644-656`). A run opens a normal bot session whose `link.kind` is `automation` and `link.approvalMode` is the clamped mode; the turn-end path maps the outcome back onto the run row (`automations.rs:292-339`).

The 10-second tick does four things in order: the runtime watchdog, then due automations, then question expiry and proposals, then queued board tasks (`automations.rs:378-433`). Constants: late threshold 60 seconds, busy deferral 15 minutes, retry delays 1/5/15 minutes, four consecutive failures to pause (`automations.rs:22-32`).

### 2.3 The limits that exist

| Limit | Where | Behaviour |
|---|---|---|
| Global pause | `automations.paused`, `automations.rs:381` | Sweep returns immediately; `run_now` answers `conflict`; the UI has a switch (`SchedulesView.tsx:145-150`). |
| Concurrency | `automations.maxConcurrent` 1-3, plus a global 4 live turns | A due automation marks itself busy; after 15 minutes it records `skipped` with reason `busy` (`automations.rs:395-406`). |
| Single flight | unique index, `claim_start` | The sweep and Run now cannot both start one automation (`automations.rs:57-60`). |
| Runtime | `maxRuntimeSecs`, watchdog | A run past its cap is closed `timeout` (or `timeout_after_changes`) and its turn is cancelled (`automations.rs:342-367`). |
| Retries | `RETRY_DELAYS_SECS` | Only a run that **only read** is retried. A run that changed files or ran commands is never retried: the owner gets Retry now (`automations.rs:270-288`, `automationSchedule.ts:140-143`). |
| Task board | `task.concurrency.max`, `task.globalPaused` | The same shape for board tasks, with queue reasons `global_pause`, `bot_busy`, `global_limit` (`crates/retinue-daemon/src/tasks.rs:230-249`). |
| Cost | none | `budget.*` returns `unsupported`; `AGENTS.md` (12 October 2026) says budgets are retired and forbids claiming budget enforcement. |

### 2.4 The unattended rails

One predicate, `is_unattended`, covers an automation session and a board task that a schedule started (`crates/retinue-daemon/src/main.rs:2202`). In `exec_options_for_session` it forces `ApprovalMode::Auto`, keeps `plan` as plan-mode-over-auto, and disables workflows, whatever the bot, its lock, the project or the global default say (`main.rs:2239-2248`). In the permission path the request is auto-approved only when the classifier allows it; otherwise a choice from the deny list is sent back and the event `permission.auto_denied` is recorded (`main.rs:5679-5704`). Remembered "Always allow" rules never answer an unattended run (`main.rs:5640-5643`). A question is answered `None` so the turn can finish (`main.rs:5919-5931`). Scheduled board tasks also carry `link.unattended` and `approvalMode: auto` (`tasks.rs:390-391`).

### 2.5 Triggers other than time

- **Signed webhook.** `POST /api/hook/<id>` through the Remote gateway only, HMAC-SHA-256 over `<timestamp>.<body>`, 300-second window, replay cache, per-source lockout, everything failing as the same 404; the body reaches the prompt only as inert quoted data, and only when `includeBody` is on (`crates/retinue-daemon/src/webhooks.rs:1-44`, `:152-173`).
- **A bot proposal** through MCP (`automations.rs:563-589`).
- **A scheduled board task**: a `when` on the task fires one copy per occurrence, clones the template, skips an occurrence if the previous child is still active, skips anything more than 300 seconds late, and marks the copy unattended (`tasks.rs:473-525`, `crates/retinue-storage/src/lib.rs:5063-5077`).

### 2.6 Reporting, and one documentation drift

Run history exists per automation: `automation.runs` with cursor (`before`), status, reason, `scheduledFor`, `startedAt`, `finishedAt`, and a link to the run's conversation (`SchedulesView.tsx:246-258`). Reasons are turned into sentences in `automationSchedule.ts:145-158`: "Started N min late", "The previous run was still going", "Nothing to report", "It stopped after changing things, so it was not retried on its own".

**Drift worth fixing regardless of nod:** the automations guide says retries happen "at 30 seconds, then 60 seconds; after three consecutive failures it pauses itself" (`docs/library/content/guides/automations-memory-and-skills.md:34`), while the code uses 1, 5 and 15 minutes and pauses on the fourth (`automations.rs:28-30`). The guide is marked "not re-checked for `main` on 22 October 2026", but it is the page an owner reads.

## 3. What each side does better

**nod does better**

1. **Cost is part of the run.** Per-run cost, a 30-day projection, and a spend check-in that stops work and asks. Retinue has no cost view for unattended work and cannot stop it for money.
2. **The model is told the truth about the run.** Due time, start time and how many later runs were skipped go into the prompt, so a 15:00 catch-up of a "morning brief" does not pretend it is morning.
3. **A failed run reaches the owner where they will look.** A message in the conversation, naming the work; Retinue's failure lives on the run row inside the app.
4. **Skip work that cannot have changed.** Change tokens, a revision/marker comparison, and a per-run observation compared with the previous verified one.
5. **A task can plan its own check** (`nextCheck`, status "Checking back") and resumes in its own session; it does not need a second schedule. Retinue's scheduled task is a whole-task clone per occurrence.
6. **An explicit vocabulary for unattended state:** needs you, waiting on someone, checking back, failed, stopped — plus "waiting on" as a first-class field.
7. **Authority is recorded next to the schedule** and revalidated when a proposed change is applied.
8. **A drain gate.** In-flight work is allowed to finish with a deadline before the runtime is paused, instead of being cut.
9. **Duplicate suppression** for an identical active schedule.
10. **Prompt ordering for cache** puts per-run volatile fields last, so a short-gap schedule reuses the cached prefix (`prompt.ts:298-321`).

**Retinue does better**

1. **Approval policy is one function, not a model call per action.** The run mode is clamped in `exec_options_for_session`; a classifier answers what `auto` may do; everything else is denied and recorded. nod decides each outside action with a trust model, which is more expensive and less predictable.
2. **A bot cannot create a live schedule.** `schedule_task` can only propose; it is disabled, plan-mode, and needs the owner's Approve. nod's assistant creates schedules directly, subject to a trust check.
3. **Retries distinguish read-only from acted runs.** `run_acted` means a run that changed something is never retried automatically. nod retries the job, not the work, and does not look at what the run did.
4. **One predicate for every unattended rail** (`is_unattended`), used by options, permissions, questions and model choice, and tested. nod's equivalent is spread across the observer's allowlist, the prompt and the trust check.
5. **A signed inbound trigger exists.** nod has none; ours has HMAC, a timestamp window, a replay cache, a per-source lockout and a fixed 404.
6. **No external services.** Local SQLite and a 10-second tick, versus Postgres and pg-boss. Simpler to ship, simpler to reason about on a desktop.
7. **Bounded permission memory.** An unattended run never consumes a remembered "Always allow" rule.

## 4. Ideas we could adopt, ranked

Ranked by value against effort. Each names the Retinue files it would change and the `AGENTS.md` rules it touches. Nothing here copies code.

### 4.1 Tell the run it is late, and how much it missed (small)

**Idea.** When a run starts after its due time, add one sentence to the prompt it sends: "This run was due at <local time> and started at <local time>. N later runs were skipped." It is the last line of `start_run`'s prompt, built from `scheduled_for`, the automation's schedule and `LATE_THRESHOLD_SECS`, the same data the UI already turns into "Started N min late".

**Why.** Cheap, and it fixes a quiet quality bug: a catch-up brief that says "good morning" at 3 pm. It also makes the skipped-run count visible to the thing that writes to the owner.

**Effort:** small. One prompt builder, one count loop capped like nod's 100.

**Risk:** low. The note must not appear for an owner-started **Run now** (that is not late), and the count must be computed from the same `now` the sweep used. A long outage could produce a large loop; cap it.

**Files:** `crates/retinue-daemon/src/automations.rs` (`start_run`, `advance_next`), a test beside the existing late-threshold test; `apps/desktop/src/ui/automationSchedule.ts` (`runReasonText`) only if the wording should match.

**Rules touched:** none of the security rules. The 12 October rule is satisfied by design (it adds information, not authority).

### 4.2 Show what unattended work cost, and what it is likely to cost (medium)

**Idea.** Two numbers on the Schedules page: each run's cost, and a "about $X a month at recent runs" line on a recurring schedule. Retinue already stores `usage_events` with per-turn tokens and reported cost, and a run is one session with one turn, so this is a read, not new bookkeeping. Show "no estimate yet" when there are no priced runs, and never present it as a limit.

**Why.** This is the widest gap. An owner cannot currently tell whether a nightly test run costs pennies or dollars.

**Effort:** medium. A storage query joining `automation_runs.session_id` to `usage_events.turn_id`, one field on the run DTO, one on the automation DTO, and two lines in the UI.

**Risk:** medium. Pricing is per provider and not always present; plan-covered runs have no cost. The label must say "estimate from recent runs" and show nothing when unknown — the same honesty rule as `quotaModel.ts`. Do not resurrect `budget.*` or claim enforcement.

**Files:** `crates/retinue-daemon/src/automations.rs` (`automation.runs`, `automation.list`), `crates/retinue-storage/src/lib.rs` (a read), `apps/desktop/src/types.ts`, `apps/desktop/src/ui/SchedulesView.tsx`, `automationSchedule.ts` (formatting), preview fixture in `preview/fixtureBridge.ts`.

**Rules touched:** `AGENTS.md` 12 October 2026 — "budgets are retired … do not claim budget enforcement". The feature must be a projection, not a cap, or it needs an owner decision first (see 4.6).

### 4.3 A task that can check back without becoming a schedule (medium)

**Idea.** A board task gets an optional `nextCheck`. When it is due, the sweep resumes **the same** task run in its own session with a fixed nudge ("this task's planned check is due; check what changed, then update or finish"), instead of cloning the whole task. The card shows "Checking back" and the time.

**Why.** "Look for a reply on Friday" is the most common follow-up there is, and today it costs a whole schedule plus a clone task. It also gives the board a state it lacks today: not working, not failed, not waiting for a person — waiting for time.

**Effort:** medium. A column and an index (the task table already has `schedule_state`; this is a different, per-run field), a sweep step with the same caps as `advance_queue`, a prompt, and a UI field.

**Risk:** medium. A new wake path is a new way to run unattended work: it must go through `queue_reason` (global pause, bot busy, concurrency), set `link.unattended` so the rails apply, and reconcile on restart so a check is never fired twice. Keep it off by default on existing tasks.

**Files:** `crates/retinue-daemon/src/tasks.rs` (`sweep`, a `resume` path, `after_turn`), `crates/retinue-storage/src/lib.rs` (`tasks` schema and reads), `apps/desktop/src/ui/TaskForm.tsx`, `ui/taskModel.ts`, `ui/useTasks.ts`, `types.ts` (`BoardTask`), preview `preview/fixtureTasks.ts`.

**Rules touched:** the 12 October unattended caps (all four apply through `queue_reason`), and "never weakening a test" for the board tests.

### 4.4 Write a "couldn't finish" message into the run's conversation (small)

**Idea.** When a run ends `failed` or `timeout` **and the automation is not going to retry it**, append one assistant message to that run's conversation: the automation's name, the reason, and the Retry now hint. Not on the first failure of a read-only run that will retry in a minute.

**Why.** The run already has a conversation; today a failure is a red dot in a list the owner may never open. nod's version of this is the single most useful reporting idea in its tree.

**Effort:** small. One insert at the points where `account_failure` decides not to retry, and on the fourth failure.

**Risk:** low-medium. Must be a daemon-written message with no tool payloads, no paths and no prompt text; the automation's own name is the only content. Do not notify the owner for every transient failure.

**Files:** `crates/retinue-daemon/src/automations.rs` (`after_turn`, `account_failure`, `watchdog`), storage `session` message helper already used elsewhere; a test asserting that a retried failure writes nothing and a paused one writes one message.

**Rules touched:** "no message text in logs" (this is a message, not a log, and it carries only a name and a reason), and the 12 October "the owner decides" note for a run that acted.

### 4.5 Skip a run when the project cannot have changed (medium)

**Idea.** An opt-in "skip when nothing changed" for a recurring automation on a project: before starting, compare the repository's `HEAD` and a short working-tree fingerprint with the values stored on the last successful run. Unchanged means record `skipped` with reason `unchanged` at no model cost. A forced **Run now** always runs.

**Why.** The natural Retinue version of nod's watch. We already have the read-only repository view (`repository.rs`) and a UI that shows changes; nightly work on an idle repository currently costs a full turn for nothing.

**Effort:** medium. A stored fingerprint, a check in `sweep` before `start_run`, a reason and label, a checkbox in the editor.

**Risk:** medium. A run that should act on time (a reminder, a digest) must not skip, so it is per-automation and off by default. The fingerprint must tolerate an untracked build folder; use `HEAD` plus the tracked status the repository module already computes, not a directory hash. A repository read failure must **run**, never skip.

**Files:** `crates/retinue-daemon/src/automations.rs`, `crates/retinue-daemon/src/repository.rs` (reuse `locate`, or a small helper), `crates/retinue-storage/src/lib.rs` (two columns), `apps/desktop/src/ui/SchedulesView.tsx`, `automationSchedule.ts`, `types.ts`.

**Rules touched:** none security; it reduces spend, which the 12 October amendment wants. It must not read or store paths that leave the host (the run reason is content-free).

### 4.6 A spend check-in for unattended runs (large — needs the owner)

**Idea.** nod's strongest single safety idea: a run that has spent N dollars since it started stops at the next tool boundary and is allowed exactly one question: "this has cost $X, continuing would do Y — keep going?" For Retinue the question is not available (unattended questions are dismissed), so the version that fits is: **stop and mark the run `needs_you`**, with the cost and what it was doing.

**Why.** It is the missing fourth rail. We have time, concurrency, pause and mode; money is unbounded.

**Effort:** large. Per-run spend needs a running total (we have `usage_events` by turn, so it is a read per tool boundary — a cache is needed), a new terminal run state and card, and a UI that can resume.

**Risk:** high. It contradicts the retired-budget position and the rule against claiming budget enforcement, so **this is an owner decision, not an implementer's choice**. A wrong estimate could stop legitimate work; a resume path is new authority for the owner to grant, not the bot.

**Files:** `crates/retinue-daemon/src/automations.rs`, `crates/retinue-daemon/src/main.rs` (the per-turn tool gate), `crates/retinue-storage/src/lib.rs`, `apps/desktop/src/ui/SchedulesView.tsx`, `automationSchedule.ts`, `docs/ipc-contract.md`.

**Rules touched:** `AGENTS.md` 12 October 2026 (budgets retired; do not claim enforcement), the unattended rail (a question becomes a state, never a prompt), and the remote tiers (`automation.*` is desktop-only; a resume would be too).

### 4.7 Record who authorised a schedule, and re-check it on change (medium)

**Idea.** A schedule stores the id of the owner message (not its text) that created it, and the definition the owner approved. When a bot proposes an update, apply it only if the proposed definition equals the one that was checked for that message; otherwise refuse with "ask again". Retinue already has the safety half (a proposal is disabled until approved); this adds the provenance half.

**Why.** With bots now able to propose hires, tasks and schedules, "why does this run exist and who said so" should be answerable from the row, without reading the transcript.

**Effort:** medium. Two fields, one comparison at the update path, one line in the schedule details.

**Risk:** low-medium. Message ids can be deleted (a backup import, a purge); a missing id means "unknown, ask again", never "allow". Do not store text: our rules forbid content in logs and would make a backup larger and leakier.

**Files:** `crates/retinue-daemon/src/automations.rs`, `crates/retinue-storage/src/lib.rs` (`automations` columns, `automation_create`, `automation_update`), `apps/desktop/src/ui/SchedulesView.tsx`.

**Rules touched:** "the daemon owns decisions"; the backup allowlist and the rule that a backup carries no message content (`backup.rs:71-74` lists settings, not rows — the new fields are ids and a small JSON definition).

### 4.8 What we should not copy

- **The observer, yet.** A background source poller with its own allowance, claims, read limits and delivery policy is a second product with a new egress and new cost. It is also the widest surface in nod. If we ever want it, it should be a Retinue-shaped read-only bot on a project, not a source crawler.
- **An assistant that creates live schedules.** Our proposal-only rule is better and matches the "the daemon owns decisions" line.
- **A model as the approver for each outside action.** nod's trust check is a per-action model; our classifier is bounded and testable.
- **Giving the model the skipped count as authority.** The late note is information. It must never read as permission to catch up on a burst.

## 5. Open questions for the owner

1. **May unattended work have a cost ceiling at all?** 4.2 (show) is safe under the retired-budget rule. 4.6 (stop and ask) is a rule change: is a per-run spend stop wanted, or does the owner prefer to keep unattended work unlimited and watch the numbers?
2. **A run that stops for cost: what state should the board and the schedule show?** "Needs you" with a resume button is honest but gives a bot's run a way to bring the owner back. Is that acceptable for a schedule the owner created, and never for a run a bot proposed?
3. **Do we want "checking back" on the task board (4.3), or is a scheduled task enough?** It is the largest behaviour change in this report, and it makes the board feel alive.
4. **Should a failed run write into its own conversation (4.4)?** It puts automation output in a place the owner may archive; the alternative is a Needs-you flag plus a badge.
5. **Is a project-fingerprint skip (4.5) the right meaning of "nothing changed" for Retinue,** or should it wait for a real change signal like a branch head moving?
6. **Do we want the late note to include the skipped count (4.1)?** It is honest, but a 40-run outage may make a prompt look alarming; capping it like nod ("100 or more") is my recommendation.
7. **The automations guide is wrong about retries and the failure count** (section 2.6). May a documentation pass fix it in the same wave as any of the above?

## 6. What this report did not establish

- Nothing was run. nod was read, not installed, and its behaviour is what the code and its own docs say, not what I observed.
- I did not audit nod's Postgres/pg-boss configuration beyond the queue options in `schedules.ts`, so "5 retries" is what the code asks for, not what a deployment gets.
- nod's per-task check-in is documented as applying to chat, schedules and the observer alike (`spending.md:110`). I verified the task path (`ledger.checkIn`, the runtime gate) and the observer's mention; I did not trace every caller.
- I did not compare cost per token between the two products; nod's pricing tables are not ours.
- Retinue's `usage_events` cost fields are provider-reported and may be absent. Whether a per-run cost can be shown for every provider is untested here.
- The retry numbers in `docs/library/content/guides/automations-memory-and-skills.md:34` are stale against `automations.rs:28-30`; I did not check whether an older release matched the guide.
