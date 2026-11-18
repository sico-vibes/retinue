# Wave 2, automations lane: contract for the screens

> **Status:** living. Written before any code (wave-2 lesson). The Director builds the
> schedules and task screens from this file; every shape here is stable unless a section
> says "changed".
> **Lane:** Ada, automations. Source: `docs/research/n8n-nod/06-automations.md`, ideas
> 4.1 to 4.7. Code: `crates/retinue-daemon/src/automations.rs`, `crates/retinue-daemon/src/tasks.rs`,
> `crates/retinue-storage/src/lib.rs`, small hooks in `main.rs`, `team.rs` and `tiers.json`.

## What changes for the UI

Nothing on the existing methods changes shape; fields are only added. Three additions:

1. Automations and runs gain money and provenance fields (ideas 4.2, 4.6, 4.7).
2. A task gains `nextCheckAt` and one new RPC (`task.checkBack`) (idea 4.3).
3. Runs can end with two new reasons, `unchanged` and `spend_check`, and two new
   transcript notices appear (ideas 4.4, 4.5, 4.6).

## Automation DTO

`automation.create`, `.list`, `.get`, `.update`, `.pause`, `.resume` all return the same
automation object as today, with these added fields:

```ts
type Automation = {
  // ...every existing field is unchanged...
  skipUnchanged: boolean           // idea 4.5, default false
  spendCheckUsd: string | null     // idea 4.6, decimal major units ("2.50"); null = off
  authorisedBy: 'owner' | 'bot' | null   // idea 4.7
  authorisedMessageId: string | null     // the message the owner acted on; may be null
  approvedAt: string | null              // ISO instant of the last authorisation
  spend: {                               // idea 4.2, computed on every read
    currency: 'USD' | null               // null until a run reports a cost
    projection: string | null            // decimal major units ("18.40"); null when unknown
    basis: 'recent_runs' | 'no_priced_runs' | 'one_off' | 'no_upcoming_runs'
    sampledRuns: number                  // priced runs the average came from (at most 20)
    monthRuns: number | null             // occurrences in the next 30 days, null when unknown
  }
}
```

- Money is **only what the provider reported**. A run with no reported cost contributes
  nothing; `spend.currency` and `spend.projection` stay `null`, never `0`.
- `spend.basis` is:
  - `recent_runs`: an average of the last (at most) 20 priced runs times `monthRuns`;
  - `no_priced_runs`: the schedule has no priced run yet, so there is no estimate;
  - `one_off`: a `once` schedule has no monthly projection;
  - `no_upcoming_runs`: the schedule has no occurrence in the next 30 days, or more than
    1000 of them, so the projection is not computed (honest over convenient).
- Copy for the card, if useful: **"About $X a month from N runs"** when `basis` is
  `recent_runs`, **"No cost reported yet"** when `no_priced_runs`, **"One-time run"** when
  `one_off`. Never say "budget" or "limit" for this line.

### New request parameters (all optional; existing callers are unchanged)

- `automation.create { ...existing, sourceMessageId?: string, skipUnchanged?: boolean, spendCheckUsd?: string | null }`
- `automation.update { id, ...existing, skipUnchanged?: boolean, spendCheckUsd?: string | null, approve?: boolean }`
- `automation.resume { id, messageId?: string }` (the existing Approve button already calls this)

Rules the daemon enforces, so the screens can rely on them:

- `spendCheckUsd` is a non-negative decimal with at most 2 fractional places, at most
  `"10000"`. `null` clears it. It is a **check-in**, not a budget: see "Runs" below.
- Creating with `enabled: true` records the owner as authoriser (`authorisedBy: 'owner'`,
  `approvedAt` now). A bot proposal stays `authorisedBy: 'bot'` with no `approvedAt` until
  the owner approves it (`automation.resume`).
- `automation.update` on a row that is waiting for approval (`pauseReason:
  'needs_approval'`) never approves it: the row keeps waiting until the owner resumes it.
  An edit that keeps it enabled records the owner as authoriser again with the new
  definition and a fresh `approvedAt`, and returns `definitionChanged: true` in the reply
  (see below).

Reply of `automation.update` gains one field:

```ts
{ automation: Automation, definitionChanged: boolean }
```

`definitionChanged` is true when the name, prompt, schedule, session mode, approval mode,
runtime cap, `skipUnchanged` or `spendCheckUsd` differ from what was stored before the
call. The screen can say "Saved. You approved this change just now." when it is true.

## Run DTO

`automation.runs` and `lastRun` keep every existing field and gain:

```ts
type AutomationRun = {
  // ...id, automationId, sessionId, status, reason, scheduledFor, startedAt, finishedAt...
  cost: {
    amount: string | null    // exact reported total in major units ("0.0015894")
    currency: 'USD'
    reported: number         // provider reports that carried a cost
    unreported: number       // provider reports that carried none
  }
}
```

New `reason` values on a run (existing ones unchanged):

| status | reason | meaning |
|---|---|---|
| `skipped` | `unchanged` | idea 4.5: the project fingerprint matched the last successful run, so no model was called |
| `cancelled` | `spend_check` | idea 4.6: the run's reported cost reached the owner's check-in amount, so it stopped |

Suggested labels: "Skipped: the project had not changed", "Stopped at your spend
check-in". The run is **not** a failure; it does not count toward the four-failure pause.

## Events

No new event types. Every change is already announced by:

- `automation.changed {automation}` (carries the new fields, including `spend`),
- `automation.run.changed {run}` and `automation.run.finished {runId, automationId,
  agentId, sessionId, status, reason}` (now with `reason: "unchanged" | "spend_check"`),
- `message.completed` for the two new notices (below),
- `task.changed {task}` / `task.run.changed {run}` for a check-back run.

## Transcript notices

Both are `role: "notice"` rows with `meta.kind`, appended by the daemon and announced with
`message.completed`. They carry no prompt text, no path and no tool payload.

- `meta.kind: "automation_failed"` (idea 4.4), written only when the automation will not
  retry: a run that failed after changing things, or the fourth consecutive failure.
  Text: `"<name>" could not finish: <reason>. Open its run history to run it again.`
  `<reason>` is one fixed sentence per cause.
- `meta.kind: "automation_spend"` (idea 4.6), written when a run stops at the check-in.
  Text: `"<name>" stopped at a spend check-in: this run has cost $<amount> so far, at or
  past the $<check> you set. This is a check-in, not a budget: the provider did not block
  anything. Run it again when you are ready.`

## Idea 4.1: the late note (inside the run's prompt, not the UI)

When a scheduled run starts later than 60 seconds after its due time, the daemon appends
two sentences to the prompt it sends. They are the last lines, so a prompt cache still
matches on the prefix:

```
This run was due at 09:00 on 17 November and started at 15:12, 6 hours 12 minutes late.
3 later runs were skipped; catch up only on what still matters now.
```

- Local wall-clock times use the schedule's own timezone. The skipped count is capped:
  `100 or more later runs were skipped`.
- It never appears for **Run now** or a webhook call: those are not late.
- The note is information only; it grants nothing.

## Idea 4.3: task check-back

- `BoardTask` gains `nextCheckAt: string | null`. A card with a future `nextCheckAt` and a
  task state that is not `running`/`queued` reads **"Checking back <when>"**.
- `task.checkBack { taskId, expectedVersion, at: string | null }` → `{ task }`.
  - `at` is an RFC3339 instant at least one minute ahead and at most 90 days ahead.
  - `null` clears it (cancel a planned check).
  - `expectedVersion` is the task's current version, like every other task write.
- When due, the daemon starts **one new run of the same task** (a fresh conversation, a
  fixed nudge appended to the task prompt) and clears `nextCheckAt`. It is not a schedule:
  there is no schedule row, no clone, no `scheduleState` change. The run goes through the
  normal queue (global pause, bot busy, concurrency) and **that run only** is unattended
  (never bypass); the task's own `unattended` flag is not changed, so a later manual drop
  still asks.
- A check that comes due while the task is `queued` or `running` **waits** for a later
  tick; it is never dropped, and it never starts a second concurrent run.
- `task.changed` carries `nextCheckAt`, so an open board updates without a reload.
- A run that fails to start consumes its check (the run records the failure); nothing
  retries a check on its own.
- Bot tool `check_back { at }`: only inside a task run; sets that task's check (at most 30
  days ahead) and returns a plain sentence. Outside a task run it refuses.
- A task deleted while a check is pending simply loses it.

## Data shape changes, and who else is affected

- **Storage:** additive columns on `automations` (`skip_unchanged`, `last_fingerprint`,
  `spend_check_usd`, `authorised_by`, `authorised_message_id`, `approved_definition`,
  `approved_at`) and on `tasks` (`next_check_at`), plus one new table
  `automation_run_costs(run_id, automation_id, cost_decimal, currency,
  reported_updates, unreported_updates, updated_at)`. All additive and idempotent;
  older databases gain them on open.
- **Reported cost:** `usage_events` deliberately ignores reported money (the retired
  monetary model, pinned by a storage test). This lane does not change that and does not
  add money to `usage.*` or analytics. Reported cost is stored **only** for automation
  runs, in `automation_run_costs`, and only to serve this contract.
- **Who else:** the Director's schedules and task screens; the remote policy table gains
  one `never` row (`task.checkBack`) and no new events. No change to budgets (still
  `unsupported`), to the unattended rails, or to backup settings.
- **Remote:** `automation.*` stays desktop only. `task.checkBack` is `never`. Both new
  notices are ordinary messages, so they follow the existing message rules.

## Running log

- Report written first (17 Nov 2026). Implementation follows.
- **Done, storage** (`crates/retinue-storage/src/lib.rs`):
  - `ensure_automation_schema` adds `skip_unchanged`, `last_fingerprint`, `spend_check_usd`,
    `authorised_by`, `authorised_message_id`, `approved_definition`, `approved_at`, and the
    table `automation_run_costs` (line ~617).
  - `automation_run_dto` carries `cost` (line ~1830); `automation_read` carries the new
    fields (line ~1900); `automation_definition`, `automation_spend_check`,
    `automation_message_id`, `automation_cost_total` are the new helpers.
  - `automation_create` records the authoriser and the approved definition; `automation_update`
    returns `definitionChanged` and re-records the owner's approval on a definition change
    (a row waiting for approval stays waiting unless `approve: true`); `automation_resume(id, messageId)`
    records the approval.
  - New reads/writes: `automation_set_fingerprint`, `automation_link_for_session`,
    `automation_cost_add`, `automation_run_cost`, `automation_run_cost_at_least`,
    `automation_costs_recent`, `session_latest_user_message_id`.
  - Tasks: `next_check_at` and `check_pending` columns; `task_set_next_check`,
    `task_checks_due`, `task_take_due_check`, `task_mark_check_pending`,
    `task_take_check_pending`; `task_read` carries `nextCheckAt`.
- **Done, daemon**:
  - `automations.rs`: `late_note`, `skipped_occurrences`, `human_span`, `short_local`,
    `occurrences_in_30_days`, `with_spend`, `spend_checkin`, `project_fingerprint`,
    `notice_could_not_finish`, `failure_sentence`; the sweep skips an unchanged project
    (reason `unchanged`) and runs the spend check-in; `after_turn` stores the fingerprint
    after a successful run and the failure notice is written only when nothing will retry.
  - `repository.rs` `fingerprint`: the HEAD commit when the tree has no tracked changes;
    untracked files ignored; any tracked change, unborn repository or read failure means
    "no fingerprint", so the run happens.
  - `tasks.rs`: `task.checkBack`, `check_backs` in the sweep, the fixed nudge on a
    check-back run, and the `check_back` bot tool.
  - `main.rs`: one hook in the `UsageReport` arm stores the reported cost against the
    run named by the session's own link.
  - `team.rs`: the `check_back` tool schema and dispatch.
  - `tiers.json`: `task.checkBack` is `never`.
- **Tests written:** storage `automation_costs_add_exactly_and_unknown_stays_unknown`,
  `automation_provenance_records_who_authorised_and_a_waiting_row_is_not_enabled_by_an_edit`,
  `automation_spend_check_is_a_bounded_decimal`,
  `task_check_back_is_planned_taken_once_and_waits_while_busy`; daemon `automations::tests`
  (`late_note`, threshold, `once`, projection, `human_span`) and
  `a_schedule_projects_spend_only_from_reported_costs`.
- **Verification (final):**
  - `cargo check -p retinue-daemon --tests` in the main checkout's warm target (the only target whose build scripts run on this PC): **passes**, before and again after the two test fixes, so the daemon, `retinue-storage`, `retinue-usage` and every test module this lane added typecheck.
  - `cargo test -p retinue-storage --lib` ran in a private copy of that target (18 GB, `%TEMP%\opencode\ada7-target`, since deleted). It listed and ran this lane's tests: 171 total, **169 passed and 2 of this lane's failed**; both were fixed (an `approve` on a waiting row now records the owner's approval even when the definition did not change, and two test instants were moved into the past). **The fixed tests were not re-run:** every re-linked test executable on this PC is refused by Smart App Control (os error 4551), so the binary could not be executed again. `--no-run` links and `cargo check --tests` passes.
  - **Not run:** the TypeScript parity test (this worktree has no `node_modules`; `task.` covers `task.checkBack` and no new event names were added), the full workspace gates, and any live daemon run.
  - **Not verified by execution:** the late note reaching a real prompt, the skip-unchanged fingerprint against a real repository, the spend check-in stopping a real run, the two notices in a live conversation, and a check-back run starting from the sweep.

## Done, not done, and where

- **Done:** all seven ideas (4.1 to 4.7) are implemented, typechecked and documented above.
  - 4.1 `automations.rs` `late_note`/`skipped_occurrences` (used in `start_run_with`);
  - 4.2 `automations.rs` `with_spend`/`occurrences_in_30_days`, storage
    `automation_cost_add`/`automation_run_cost`/`automation_costs_recent`, the write in
    `main.rs` `UsageReport`;
  - 4.3 `tasks.rs` `check_backs`/`check_back_tool`/`task.checkBack`, storage
    `task_set_next_check`/`task_checks_due`/`task_take_due_check`/`task_*_check_pending`;
  - 4.4 `automations.rs` `notice_could_not_finish` (called from `account_failure`);
  - 4.5 `repository.rs` `fingerprint`, `automations.rs` `project_fingerprint`, the sweep
    branch, storage `automation_set_fingerprint` and the two columns;
  - 4.6 `automations.rs` `spend_checkin`, storage `automation_run_cost_at_least`;
  - 4.7 storage `automation_definition` and the create/update/resume authorisation rules,
    `authorisedBy`/`authorisedMessageId`/`approvedAt` on the DTO.
- **Not done:** no new terminal run state (`needs_you`) for the spend check-in: the run
  table's `status` CHECK cannot gain a value without a table rebuild, so it is
  `cancelled` + `reason: "spend_check"`. The docs guide that says retries are "30 seconds,
  then 60 seconds; after three failures" is still wrong (report 06, section 2.6); it is
  outside this lane's files.
- **Open for the Director:** decide whether the schedules screen wants the
  `definitionChanged` line, and whether a "Checking back" card should read the time from
  `nextCheckAt` in the local zone (the daemon sends UTC).
- **Skill updated:** `references/verification.md` (Smart App Control blocks newly linked
  test executables, and a shared target can serve another lane's test binary),
  `references/lessons.md` (same, as a dated entry), `references/codebase-map.md` (this
  wave's daemon and storage pieces).
