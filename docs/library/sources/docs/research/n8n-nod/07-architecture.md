# nod's architecture and engineering, against Retinue's daemon, storage, gates and releases

> **What this is:** a research report for the owner, one of a set written at the same time about `n8n-io/nod`. This one covers nod's overall layout, server and agent loop, storage and migrations, event model, error handling and retries, logging, configuration, testing, CI, release and code-quality tooling, compared with Retinue's. It does not cover the sandbox, providers, connectors, Vault or messaging; those are reports 01 to 05.
> **Status:** research only. No Retinue code was changed, no commit was made.
> **Source:** `https://github.com/n8n-io/nod`, cloned at commit `47cfee3fe77279e1d543a367131d3ff2c1202e18` (16 November 2026, `main`, `fix(agent): balance feedback during long tasks (#975)`). The clone is at `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-sandbox`; it was read, never installed, built or run. Every nod citation below links to that commit.
> **Owner:** Vex. **Author:** Grey (research lane). **Date:** 16 November 2026.

## Summary

1. nod is one Node process, Fastify plus Postgres plus pg-boss, split into six layers (`web`, `shared`, `ports`, `runtime`, `providers`, `server`), and a 77-line script fails the build when any import crosses a layer, so its architecture is enforced rather than described.
2. Its turn loop is one durable queue row per thread: turn state lives in Postgres, one writer lock per thread, turns still marked running at boot are failed, and a failed turn writes its single failure message once, guarded by a stream id and a staleness check.
3. Everything else in that shape is smaller than ours: one conversation per owner, coarse "refresh" plus text deltas over SSE, a schema created by idempotent DDL with no version number, and no persisted event log.
4. What genuinely transfers is the engineering around the code: an import-boundary gate, a fail-closed CI selector with a nightly run and no silently skipped tests, secret scanning and formatting in a pre-commit hook, schema-per-test isolation with the environment restored, an injectable failure sink, and release builds that install the previous version and prove the update works.
5. Retinue is ahead on events, migrations with backups, retry policy, diagnostics, performance gates and consent; nod is ahead on boundaries, CI hygiene, secret scanning, and proving a release before shipping it.

## Licence and copying

Root `LICENSE` is Apache-2.0 and `NOTICE` carries the attribution ([LICENSE](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/LICENSE), [NOTICE](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/NOTICE)). Apache-2.0 permits copying with attribution and a change notice. Every idea below is a design written in our words; one file is small enough to copy almost verbatim, and idea 2 names it. Nothing was copied into Retinue for this report.

## How nod does it

### The shape

Six directories under `src/`, and the only file allowed to see all of them is `src/server/composition.ts` ([AGENTS.md:32](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/AGENTS.md#L32)):

| Layer | What lives there | May import |
| --- | --- | --- |
| `src/web` | React pages, hooks, components | `web`, `shared` |
| `src/shared` | types and pure helpers used by both sides | `shared` |
| `src/ports` | interfaces only: agent, computer, integrations, speech, memory, key store | `ports`, `shared` |
| `src/runtime` | the agent loop, prompt, tools, skills | `runtime`, `ports`, `shared` |
| `src/providers` | every vendor SDK and adapter (Pi, Composio, 1Password, PostHog, OTLP, Docker, ...) | `providers`, `ports`, `shared` |
| `src/server` | Fastify routes, turns, storage access, policy | `server`, `ports`, `shared` |

That table is not documentation. [`scripts/check-architecture.mjs`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/scripts/check-architecture.mjs) parses every TypeScript file with the TypeScript compiler API and fails on three things: an import that crosses a layer (`:49-61`), a vendor package imported outside `providers` (`:43-44`), and `process.env` read anywhere outside `server` (`:66-69`). It runs as part of `pnpm run lint`, which CI runs as "Check types and import boundaries" ([ci.yml:88-90](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/workflows/ci.yml#L88-L90)) and as the pre-push hook ([lefthook.yml:9-11](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/lefthook.yml#L9-L11)).

### Server and agent loop

One process. `src/server/index.ts` builds the app, mounts Vite in development or the built client in production, and shuts down on SIGINT, SIGTERM or losing the parent ([index.ts:1-112](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/index.ts#L1-L112)). `createApplication(createDefaultAdapters(process.env))` is the whole wiring; routing lives in `app.ts` (1,000+ lines of Fastify routes).

A chat message's path:

1. `POST /api/messages` parses the body with a strict zod schema, then `submitUserMessage` writes the message row and its `turns` row in one transaction (`app.ts:518-583`, `thread.ts:596-826`).
2. The turn is queued by sending a pg-boss job after the transaction commits: `sendDrain` (`thread.ts:316-329`), with `afterCommit` so a rolled-back write never schedules work.
3. A worker (`boss.work`, one local concurrency, 0.5 s poll, batch 20, `thread.ts:1309-1333`) drains the thread: `claimTurns` takes the thread lock, refuses a session that already has a running turn, and flips queued rows to `running` in one transaction (`:1335-1360`).
4. `executeTurn` (`:1624-2366`) builds a `RuntimeContext` and hands it to `src/runtime`, which opens a Pi session (`@earendil-works/pi-coding-agent`) and streams events back through `context.emit`, `context.reply` and `delta()`.
5. Every tool call and lifecycle step is appended to `turn_events` (`insertEvent`, `:331-354`) and announced to the UI as a coarse `refresh` or a text `delta` (`events.ts:15-26`).

The thread lock is a row in `nod_locks` taken with `FOR UPDATE` (`locks.ts:6-12`), and every write from a turn goes through `withTurnLock`, which refuses when the turn is no longer `running` or the abort signal fired (`thread.ts:289-308`). That is the rule that makes Stop safe: a stopped turn cannot land a late write.

### Storage and migrations

Postgres. The whole schema is one `initializeDatabase()` of idempotent DDL: `CREATE TABLE IF NOT EXISTS`, `ALTER TABLE ... ADD COLUMN IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`, plus two triggers and a repair pass that cancels stale queued turns and backfills an expiry column by inspecting `pg_attribute` ([db.ts:59-203](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/db.ts#L59-L203)). There is no migration number and no schema history. The schema is 199 lines in one file, next to the code that uses it.

`transaction()` is abort-aware: it races the connection against the turn's signal, sets `client_connection_check_interval`, rolls back and fires `afterRollback` callbacks, or commits and fires `afterCommit` ([transaction.ts:1-78](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/transaction.ts#L1-L78)). Side effects are written against that pair, not against the commit itself.

### Events between server and UI

One in-process `EventEmitter` with four shapes: `refresh`, `working`, `activity`, and a text `delta` ([events.ts:4-27](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/events.ts#L4-L27)). `/api/events` is a hand-rolled SSE stream: it sends `refresh` on connect, writes a `: keepalive` comment every 15 s, and filters deltas by owner and channel ([app.ts:592-627](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/app.ts#L592-L627)). The client answers a `refresh` by refetching `GET /api/state`, which returns the active thread, its last 150 messages, 50 turns, tasks, activity, pending questions and configuration, with older pages fetched by id ([app.ts:340-405](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/app.ts#L340-L405)).

So the event model is "coarse invalidation plus a read model", not an event log. Nothing about an event is durable except the `turn_events` rows written by the turn itself. If a socket drops, the next connect gets a fresh `refresh` and the client catches up by reading.

### Errors and retries

Two rules, both written down and both implemented in one place.

- **A failed turn reports once.** `failTurn` marks the turn failed only if it is still `running`, then writes one assistant message whose `stream_id` is `failure:<turnId>` (or `model-failure:<turnId>`), and only when no message with that stream id exists and no newer user message has arrived ([thread.ts:1204-1298](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/thread.ts#L1204-L1298)). A stale failure cannot surface after the owner has moved on.
- **Turns are not retried.** The queue job retries twice and the drain resend retries three times with 250 ms doubling (`thread.ts:1421-1432`); the turn itself is never re-run, and `AGENTS.md` says tool calls and turns are never retried automatically, with model requests retried only inside Pi ([AGENTS.md:22](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/AGENTS.md#L22)).

Failure text is composed, not echoed: `failureText` redacts secrets, caps at 2,000 characters, and appends actionable advice derived from the provider's own message for quota, size and daily-limit errors (`modelRecoveryAdvice`, `thread.ts:1168-1202`).

### Logging and diagnostics

One function, `logFailure(event, error, context)`, prints one JSON line carrying only `event`, `name`, `code`, `statusCode`, `status`, each truncated to 64 characters, so user text cannot reach the log at all ([failure-log.ts:29-47](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/failure-log.ts#L29-L47)). A sink is injected through `AsyncLocalStorage` (`:9-14`), so a test can capture the failures a code path reports without touching the console; `approvals.test.ts` and `channel-delivery.test.ts` do exactly that. The HTTP error handler logs only 5xx, and answers every internal error with one fixed sentence while client errors keep their message ([app.ts:273-308](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/app.ts#L273-L308)).

There is also an opt-in model-trace pipeline with a redaction layer and pseudonymised ids (`src/server/traces.ts`), and an OTLP provider. Both send data off the machine, so neither is a candidate for us.

### Configuration

All environment reading is in `src/server/config.ts`, which is a few dozen lines: it parses the port and token through `serverNetwork`, validates the desktop configuration as a whole and throws one sentence if anything is missing, and defaults everything else ([config.ts:1-45](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/server/config.ts#L1-L45)). The architecture gate makes that monopoly real.

### Testing

`node:test` through `tsx`, 26 files, about 600 tests, run as `tests/*.test.ts` ([package.json:22](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/package.json#L22)). Two practices stand out.

- **Schema per test.** A test creates `test_thread_<random>` and `test_boss_<random>`, points `DATABASE_URL` at that schema, sets `PGBOSS_SCHEMA` and a fresh `NOD_DATA_ROOT` under `mkdtemp`, imports the modules fresh, and restores the previous environment in `t.after` ([thread.test.ts:50-80](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/tests/thread.test.ts#L50-L80)). No test sees another's rows or the developer's data folder.
- **No silent skips.** Database tests carry `{ skip: !process.env.TEST_DATABASE_URL }`, and the runner fails the run when anything was skipped, marked todo or cancelled: `if (!success || counts.skipped || counts.todo || counts.cancelled) process.exitCode = 1` ([test-integration.mjs:61](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/scripts/test-integration.mjs#L61)). The CI job sets `TEST_DATABASE_URL` against a Postgres service, so in CI nothing may skip.

Integration tests run in a Docker image with Chromium and a private Postgres, selected by file, at concurrency `min(4, availableParallelism())` ([test-integration.mjs:40-61](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/scripts/test-integration.mjs#L40-L61)). `AGENTS.md` also caps what may be tested at all: only the harness, sandbox isolation, safety, owner isolation and real integration contracts, and "no tautological tests, mocks that return what they were told, or tests of UI copy" ([AGENTS.md:65-75](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/AGENTS.md#L65-L75)).

Separately, `evals/` runs real tasks against a real model with fake apps for scoring, never in unit tests, with a per-scenario fingerprint and a recorded proof line such as `"main-2bc71c2: 5/5"` ([evals/gate.json](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/evals/gate.json), [evals/README.md](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/evals/README.md)).

### CI, release and code quality

CI is twelve jobs plus a selector ([ci.yml](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/workflows/ci.yml)):

- `scope` reads the changed paths and decides which jobs run, using a tested path-to-check table ([ci-scope.mjs:20-182](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/scripts/ci-scope.mjs#L20-L182)). Unknown paths run everything; package or lockfile changes run everything.
- `checks` is a single required job that fails unless every *selected* job passed: `requireChecks` rejects a skipped-but-required job and a missing scope output ([ci-scope.mjs:184-204](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/scripts/ci-scope.mjs#L184-L204)). This is what stops a "path filter" from letting a pull request through untested.
- `secrets` scans the whole history with a checksum-verified Gitleaks, and first runs `check-secret-config.py` to prove the allowlist exceptions still detect what they must ([ci.yml:423-455](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/workflows/ci.yml#L423-L455)).
- There is a nightly run on a cron, so OS and dependency drift is found without a pull request ([ci.yml:9-10](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/workflows/ci.yml#L9-L10)).
- Every action is pinned by commit SHA and every downloaded tool is checked against a published digest.

Release is a pipeline, not a script run by hand: a `plan` job refuses to start unless CI on that exact commit passed (["Wait for CI on the release commit", desktop-release.yml:82-86](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/workflows/desktop-release.yml#L82-L86)); per-platform `*-checks` jobs build and run update acceptance, which installs the previous version, updates it, and verifies the profile survived and a wrongly signed build is refused ([desktop-release.yml:515-560](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/.github/workflows/desktop-release.yml#L515-L560)); packaging follows; then the `release` job verifies the uploaded asset digests, signs provenance and drafts the release (`:703-733`). A saved context artefact lets a failed release be re-driven (`desktop-release-recovery.mjs`).

Code quality is three small things: `tsc --noEmit`, Prettier (`printWidth` 100, single quotes, trailing commas), and the architecture script. There is no ESLint. A pre-commit hook formats staged files and runs Gitleaks on them ([lefthook.yml:1-8](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/lefthook.yml#L1-L8)).

## How Retinue does it now

**Processes and channels.** The shell, the daemon and the adapter crates (`.claude/skills/retinue-development/references/architecture.md:5-22`). The UI talks to the daemon over loopback HTTP `POST /v1/rpc` and WebSocket `GET /v1/events`, guarded by a capability token. Boundaries are enforced by one test, `apps/desktop/src/transport/noDirectNative.test.ts`, plus the `tiers.json` parity between Rust and TypeScript.

**The RPC surface.** One `match` in `dispatch()` (`crates/retinue-daemon/src/main.rs:1182`), in a 15,580-line file. Each arm validates its own parameters by hand, which gives precise error codes but no single schema per method.

**Storage and migrations.** SQLite through `crates/retinue-storage`, 11,640 lines in `src/lib.rs` plus `phase*.rs` modules. Migrations are numbered with `PRAGMA user_version` and each step takes a backup and runs `PRAGMA integrity_check` around the change (`lib.rs:56-200`), with idempotent `ensure_*_schema` functions for new tables and `migration.*` settings markers for one-time data steps.

**Events.** Every event is a row in `app_events` with a sequence number, then broadcast on a tokio channel: `AppState::emit` persists and sends, so a reconnecting client resumes by sequence (`main.rs:212-249`). Events are classified for remote in `apps/desktop/src/remote/tiers.json`.

**Logs and diagnostics.** A rolling file under `logs/` (20 MB, 14 files), a `RedactingWriter` on every line, a `diagnostics.logLevel` setting, an export zip, and a source-text test that every log call site passes a fixed message (`crates/retinue-daemon/src/diagnostics.rs:3-58`).

**Retries.** Typed and deliberate: overloaded retries with doubling backoff, rate-limited retries padded past the reset with jitter, a 6-hour default ceiling, restart grace and a single decision owner per turn (`crates/retinue-daemon/src/retry.rs:1-37`). Schedules have their own policy (1/5/15 min, four consecutive failures).

**Tests.** At the last release, 1,733 passing Vitest tests and, in Rust, 1,064 daemon and 166 storage tests (the checklist's own figures), with daemon tests using `Storage::open_in_memory()` and a `state()` helper (`main.rs:7605`). Fixtures exist for adapters (for example `crates/retinue-adapter-claude/tests/fixtures/subagent-agent-turn.ndjson`). `references/verification.md:13-42` lists a long set of known local failures, flaky tests and machine-specific environment leaks.

**Gates and CI.** `npm run typecheck`, `npm test`, `cargo test --workspace --exclude retinue-desktop`, `cargo clippy -D warnings`, both web builds, `docs:check`, the performance gate and the render budget, run as one 90-minute `windows-latest` job on push and pull request (`.github/workflows/ci.yml:21-40`). There is no formatter, no linter for `apps/desktop`, no secret scanner and no nightly run.

**Release.** A manual `workflow_dispatch` build on `windows-latest` that signs through SignPath and records a digest (`.github/workflows/release.yml:1-13`), then `scripts/release-windows.ps1` run locally, where the updater key lives, and a hand-written entry in `docs/operations/release-checklist.md`. The checklist's own entries repeat "Not checked: the in-app update from beta.N".

## What they do better, and what we do better

**They do better**

| Their practice | Why it matters for us |
| --- | --- |
| Import boundaries enforced by a build step | We have the same rules as prose in `SKILL.md` and a single boundary test; nothing stops a new module from reaching across |
| Fail-closed CI selection, one required `checks` job, nightly run | A single 90-minute job invites "the known `remoteBuild.test.ts` failure" to be tolerated, which it was, for days |
| No skipped or todo tests allowed in CI | Skipped tests are how a suite rots quietly |
| Secret scanning in the hook and in CI, with tested allowlists | Our privacy rules are enforced only by review, and a committed secret cannot be taken back |
| Schema-per-test isolation, environment saved and restored | Three of our documented local failures come from tests reading the owner's ambient environment |
| An injectable failure sink, so tests assert what is reported | We test that log lines are fixed text; we do not test that a failure is reported at all |
| Release builds that wait for CI and prove the update round trip | "The in-app update from beta.N was not checked" recurs in our own checklist |
| One short, current invariants list | `AGENTS.md` is 400+ lines of dated amendments; a newcomer must read all of it to know today's rules |

**We do better**

| Our practice | Why it beats theirs |
| --- | --- |
| Events are persisted with sequence numbers | Their `refresh` is fire-and-forget; a dropped socket loses the fact that something happened until the next read, and nothing can be replayed or audited |
| Migrations are numbered, backed up and integrity-checked | Their schema has no version and no history; a data transform, a column drop or a tested upgrade path is impossible to express |
| Many conversations, projects, bots and machine links | They support exactly one thread per owner |
| Retry policy is typed and durable | Their rule is "never retry a turn", which is safe but blunt; ours survives a restart and knows why it waited |
| Diagnostics: rolling redacted log, fixed-message test, export zip, doctor | They log one JSON line per failure and have no export |
| Performance and render budgets as gates | They have no performance gate |
| Approval tiers, per-action consent, remote policy in one table | Structurally different products; our consent model is far more developed |
| Test isolation is cheap (`open_in_memory`) | Schema-per-test in Postgres is slower and needs Docker |

## Ideas we could adopt, ranked

Each idea below is a design, not copied code. Effort is for one lane, one focused change. Risk is the chance of a wrong result reaching the owner, not the chance of work.

### 1. A secret scan and a format check, in CI and in the hook — small effort, low risk

**What.** Add Gitleaks with a reviewed allowlist, run on the whole history in CI and on staged files in a pre-commit hook; add Prettier with a config and a `format:check` gate. Copy the trick that makes an allowlist safe: a script that proves every exception still detects a planted secret, run before the scan.

**Why.** The 16 November amendment says a Vault value never appears in "an event, a log, a backup, a diagnostics bundle, a push or a reply", and the 14 October one says never log credentials. Nothing mechanical checks a commit. A secret that reaches `main` is not recoverable by a revert.

**Risk.** A false positive blocks a lane; mitigated by a narrow allowlist for test fixtures and by running the check before the hook blocks anything. Formatting an old tree creates a large noise diff; apply it once, in its own commit, with no other change.

**Files.** `.gitleaks.toml` (new), `lefthook.yml` (new), `package.json`, `.prettierrc.json` (new), the hook install step in `.claude/skills/retinue-development/references/pipeline.md`, `.github/workflows/ci.yml`.

**AGENTS.md rules touched.** The privacy rules in the 14 October dictation amendment and the 16 November Vault amendment; the lane rule "lanes never commit" (the hook runs on the Director's commit, not the lane's); golden rule 7 in `SKILL.md` ("run the gates").

### 2. An import-boundary check for the daemon and the UI — small effort, low risk, best value per hour

**What.** Port `check-architecture.mjs` as a design (77 lines here; the table is the value). Declare the layers we already have and fail on a violation: `crates/retinue-storage` must not know about adapters; `retinue-agent-core` and `retinue-runtime` must not depend on `retinue-daemon`; `retinue-daemon` may not build HTTP clients outside `remote/` and `machine_client.rs`; on the UI side, `apps/desktop/src/ui` may call the daemon only through `tauri.ts`/`nativeBridge.ts` (the existing test generalised), `preview/` must not be imported by `ui/`, and `remote/` must not import `tauri.ts`. Add a rule that a new dependency of a crate must appear in an allowlist, the same way nod confines vendor SDKs to `providers`.

**Why.** Our compile speed and review cost are dominated by two files of 15,580 and 11,640 lines, and the pressure to reach across boundaries grows with every lane. A boundary that fails a build is the only kind that survives 40 waves. This is the single highest-leverage item in the report.

**Risk.** The first run will find violations. Fix them or allowlist them with a one-line reason, in the same change. A rule that is wrong is expensive; keep the table short (five or six rules) and each rule justified in the script's header.

**Files.** `scripts/check-architecture.mjs` (new, Node, reading `Cargo.toml` and `use` lines for Rust and imports for TS), `package.json`, `.github/workflows/ci.yml`, `.claude/skills/retinue-development/references/verification.md`, `references/codebase-map.md`.

**The one file worth copying.** `scripts/check-architecture.mjs` is 77 lines of generic script with no nod-specific logic beyond a layer table and three regexes. I recommend copying it under Apache-2.0 with a notice in `docs/legal/third-party-notices.md`, changing the table and adding the Rust reader. The allowlists of layer-to-layer edges are the design work; the script is incidental.

**AGENTS.md rules touched.** Golden rule 7 (gates); the lane brief template in `.claude/skills/retinue-development/references/pipeline.md` (a new gate the Director runs).

### 3. One current invariants page, each rule naming its test — small effort, low risk

**What.** A single "Invariants" page with the rules that are true today, one sentence each, each naming the file or test that enforces it: the daemon owns decisions; the UI never fabricates usage; no copied provider auth; prompts go through stdin; events are persisted and redacted for remote; logs carry fixed messages; `vault.*` is never remote; the owner's computer needs three things. Move dated amendments to `docs/history/` and keep only `AGENTS.md`'s pointer. Add one line per rule for its enforcing test, and treat a rule with no test as a to-do.

**Why.** nod's `AGENTS.md` keeps one "Architecture" section and one "Safety invariants" section, each stating the current truth, backed by tooling ([AGENTS.md:20-58](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/AGENTS.md#L20-L58)). Our `AGENTS.md` now supersedes itself in eleven amendments; reading it correctly is a skill, and a mis-read rule is how agents ship the wrong thing.

**Risk.** Losing history. Keep every amendment, move it, do not delete it. Do the move in a change that touches no code.

**Files.** `AGENTS.md`, `.claude/skills/retinue-development/SKILL.md`, `references/architecture.md`, `references/pipeline.md`, a new `docs/reference/invariants.md`.

**AGENTS.md rules touched.** Line 3 (read the skill before working) and every amendment it summarises; the release gate in the 14 November amendment.

### 4. CI: split the one job, select checks by path, require them, and run nightly — medium effort, medium risk

**What.** Take nod's shape: a `scope` job with a tested path-to-check table that fails closed (an unknown path runs everything, a lockfile change runs everything), separate jobs for typecheck and Vitest, Rust tests by crate, both web builds, docs and the performance gate, and one final `checks` job that fails unless every selected job passed. Add a nightly run on a cron. Keep the Windows runner: our gates are Windows-specific.

**Why.** Our own release notes record CI red at clippy on two release commits and a test that failed on `main` for days; a single long job makes the whole gate binary and easy to dismiss. Selection also cuts the 90 minutes to minutes for a docs or UI-only change.

**Risk.** A selector that skips a needed check. Mitigate exactly as they do: fail closed, require the final job, and test the selector itself as a unit (`ci-scope.test.mjs`). Second risk: our Rust steps depend on `RUST_MIN_STACK` and a long temp path, so any job split must carry them.

**Files.** `.github/workflows/ci.yml`, `.github/scripts/ci-scope.mjs` (new), `.github/scripts/ci-scope.test.mjs` (new), `.claude/skills/retinue-development/references/verification.md`, `references/pipeline.md`.

**AGENTS.md rules touched.** Golden rule 7; the Director's "run every gate and commit" clause in the 30 October and 16 November amendments (the gates move from one job to a matrix; the set must not shrink).

### 5. Make a test see only what it created — small effort, low risk

**What.** One daemon test helper that returns an isolated `AppState` with an in-memory database, a fresh temporary data root, and every environment variable the daemon reads set explicitly and restored afterwards: `RETINUE_DB_PATH`, `RETINUE_REMOTE_WEB_DIR`, `RETINUE_HOOK_BRIDGE`, `RETINUE_DATA_DIR`. Then convert the tests that use `std::env::temp_dir().join(...)` by hand. Delete the three "fails on the owner's machine" entries from `verification.md` because they can no longer happen.

**Why.** We already do the database part (`Storage::open_in_memory()`). nod's contribution is the rest: fresh data root, explicit environment, restore in a `t.finally`-equivalent. Our own verification notes document the failure mode it removes (`verification.md:42`).

**Risk.** A test may silently depend on the ambient environment in a way that only shows when it is removed; each conversion is small, and a failure is a real bug found.

**Files.** `crates/retinue-daemon/src/main.rs` (the `state()` helper near `:7605`), the test modules that build their own directories, `.claude/skills/retinue-development/references/verification.md`.

**AGENTS.md rules touched.** Golden rule 8 ("never weaken or delete a test"); the gate list in golden rule 7.

### 6. One failure sink, so tests can assert that failures are reported — small effort, low risk

**What.** Give `diagnostics` an optional sink (a `Mutex<Vec<FailureLine>>` in tests) and a `record_failure(event, error, context)` that writes both the sink and the log. Then assert in a handful of tests that a dropped adapter connection, a refused remote request and a failed schedule run each record exactly one entry with the expected event name.

**Why.** Today we assert that every log call site passes a *fixed message* (good) but not that the interesting failures reach the log at all. A failure path that returns an error without logging is invisible in the diagnostics bundle the owner sends us.

**Risk.** Low. The sink must never be populated in production.

**Files.** `crates/retinue-daemon/src/diagnostics.rs`, `crates/retinue-daemon/src/main.rs` (call sites for the three chosen failures), a new test module.

**AGENTS.md rules touched.** The privacy rules (the sink carries the same content-free fields as the log); golden rule 7.

### 7. Prove the update round trip, and gate the release on green CI — medium to large effort, medium risk, highest blast radius

**What.** Two parts. First, a `release.yml` step that refuses to build unless the CI run for that commit succeeded (their `gh run watch` gate). Second, a scripted acceptance run: build the previous release and the new one with a throwaway app identifier and a test updater key, install the old one into a temporary profile, publish a local `latest.json`, update, and assert the app starts with its data intact. Run it on the Windows runner and locally before publishing. Keep the real updater key out of CI, exactly as the workflow comment already requires.

**Why.** Our own checklist repeats "Not checked: the in-app update from beta.N". That is the one failure a user cannot work around, because the updater is how they receive the fix. nod treats update acceptance as a release gate on two platforms and refuses to publish without it.

**Risk.** The acceptance script can damage an install if it uses the real identifier or the real key. Use a separate identifier (`com.retinue.desktop.acceptance`), a separate data folder, and a test key generated once for this purpose. Second risk: runtime. Budget about ten minutes per platform.

**Files.** `.github/workflows/release.yml`, `scripts/release-windows.ps1`, `scripts/update-acceptance.ps1` (new), `docs/operations/release-checklist.md`, `docs/operations/updater.md`.

**AGENTS.md rules touched.** The 14 October release-cleanup amendment (a release is not finished until cleanup is recorded); the 14 November release gate ("a release that changes these defaults needs the owner's live check before it ships"); the 17 October Native release gate if the updater manifest changes.

### 8. Bound what a conversation read returns — medium effort, medium risk

**What.** Adopt their windowing shape for the one case we know is heavy: `session.get` returns the newest window of messages (say 200) with `hasEarlier` and a cursor, plus a `session.messagesAround {sessionId, messageId}` for jump-to and search hits, and a per-turn "load tool payloads" call. Our architecture note already names the problem: "what is left is the tool payloads, which a conversation read still returns in full" (`references/architecture.md:27`).

**Why.** Opening an old conversation is the slowest thing the app does on the owner's real data, and it grows without bound.

**Risk.** Every UI surface that reads a whole conversation must handle a window: the plan, the phase, tool rows, the review cards, search and the companion. This is the largest change on the list in UI terms; do it behind a setting, measure with `npm run perf` before and after.

**Files.** `crates/retinue-storage/src/lib.rs` (`session_detail_parts`), `crates/retinue-daemon/src/main.rs` (`session.get`), `apps/desktop/src/types.ts`, `ui/App.tsx`, `ui/conversation.ts`, `preview/fixtureBridge.ts`.

**AGENTS.md rules touched.** None directly; it changes the IPC contract, so `docs/ipc-contract.md` and its "protected path" status apply, and `tiers.json` needs a row for any new method.

### 9. A small, fingerprinted behaviour suite — large effort, low risk, later

**What.** Not their eval harness (about 40 files, live models, money). Just its discipline: a fixed list of recorded conversations, each with a fingerprint of the fixture and a recorded proof of the last good result, run by hand or nightly when a prompt, a tool description or an adapter mapping changes. We already keep adapter fixtures; this adds the fingerprint and the proof line.

**Why.** Prompt and tool-text changes are the least tested part of the product, and the owner currently finds their regressions in use.

**Risk.** Low, but the value depends on keeping the fixtures honest. Do this only when a regression has already escaped once.

**Files.** `crates/retinue-agent-core/src/` and `crates/retinue-daemon/src/team.rs` (prompt text), new `docs/fixtures/behaviour/`, a script under `scripts/`.

**AGENTS.md rules touched.** Golden rule 8; the 30 October lane rules (which lane runs it).

## What not to adopt

- **Postgres, pg-boss and advisory locks.** Our SQLite writer plus a durable queue table is the right shape for a single-user desktop app, and `open_in_memory` makes our tests an order of magnitude cheaper.
- **Idempotent DDL with no version.** It cannot express a data transform, a rename or a drop, and it cannot be tested as an upgrade. Our numbered migrations with a backup and an integrity check are better.
- **Coarse `refresh` events, and refetching the whole state as the primary read.** Fine for one thread and 150 messages; wrong for a client with dozens of conversations. Idea 8 takes the windowing and leaves the refetch.
- **Model-trace egress and PostHog analytics.** Out of scope for us on privacy grounds, whatever their engineering quality.

## Open questions for the owner

1. **Which layer rules are non-negotiable?** The boundary check is only as good as its table. My proposal is five rules: storage knows no adapters, adapters know no daemon, the daemon reaches the network only through `remote/` and `machine_client.rs`, the UI reaches the daemon only through `tauri.ts`/`nativeBridge.ts`, and `preview/` is imported by nothing in `ui/`. Do you want a sixth for the native shell?
2. **Would you accept one large formatting commit?** Prettier over `apps/desktop/src` touches every file once. The alternative is to skip formatting and take only the secret scan.
3. **Should the release ever run without update acceptance?** If yes, the checklist needs a named waiver line per release, as it has for the threat model.
4. **Is the CI split worth the complexity now, or only when the suite runs clean for a week?** A selector on a suite with known red items just runs them less often. My recommendation: reach a green `main` first, then split. Today's known red item is clippy under a newer Rust; the phone-bundle size test was the other, fixed on 14 November.
5. **Do you want the invariants page to live in `AGENTS.md` or in `docs/reference/`?** The second is easier to keep current and harder to forget to read.
6. **The one thing I did not evaluate:** whether nod's `executeTurn` can teach us anything about our own turn loop. It is 740 lines and I read only its entry and exit; a deeper read belongs with a change in hand, not a survey.

## What I verified and what I assumed

**Verified by reading the clone.** Every nod citation above, with line numbers at commit `47cfee3`. I read the CI workflows, the scope script and its test, the architecture script, `db.ts`, `events.ts`, `transaction.ts`, `failure-log.ts`, `thread.ts` (locking, queueing, worker, failure paths), `app.ts` (routes, error handler, SSE, state), `config.ts`, `locks.ts`, `local-database.ts`, `test-integration.mjs`, `thread.test.ts` setup, `evals/gate.json` and `AGENTS.md`, and the release workflow. I ran nothing from the clone: no install, build, test or binary.

**Verified in Retinue by reading.** The line counts, the `dispatch()` location, the event emit path, `diagnostics.rs`, `retry.rs`, the storage migration functions, `ci.yml`, `release.yml`, `release-windows.ps1`'s comment that the updater key never enters CI, and the checklist entries that record unchecked updates. The test counts are from the checklist's own last entries, not from a run.

**Assumed.** That our daemon's `Storage` writes are always on one connection (stated in `references/architecture.md:26`, not re-verified in code here). That no ESLint or Prettier config exists for `apps/desktop`; I searched the tree and found configs only in `apps/site` and `apps/website`. That the three documented "fails on the owner's machine" tests are still present; I read the notes, not the tests. That nod's `executeTurn` internals are out of my area beyond its shape.

**Not checked.** nod on Windows, its Docker path, or any live behaviour. Our CI's current status on `main`. The actual size of a formatting diff.
