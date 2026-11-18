# Invariants

> **What this is:** the rules that are true of Retinue today, one line each, with the file that enforces each one and the test that pins it. `AGENTS.md` holds the history and the dated amendments; this page holds the current truth, so a newcomer does not have to read amendments in order to know what is safe to change.
> **Status:** current, 16 November 2026 (first version, from `docs/plans/nod-adoption-plan.md`, idea 3). **Update it in the same change as any rule or test below.** A rule whose test is deleted, renamed or skipped must be re-pointed here, or it is no longer enforced.
> **Reading "untested":** the rule is real and reviewed by hand. It is a to-do for whoever touches that area next, not permission to break it.

## Safety and privacy

| Rule | Enforced by | Pinned by |
|---|---|---|
| No copied provider authentication. A coding agent's sign-in is never read or stored. Two owner-approved exceptions exist, each for one call: opt-in cloud dictation may read Codex's `auth.json` to reach the transcription endpoint, and the usage bar may read a CLI's stored token to ask that provider's own usage endpoint. | `crates/retinue-speech/src/cloud.rs`, `crates/retinue-daemon/src/quota_sources.rs` | the two exceptions have unit tests in place (`quota_sources.rs`); the rule as a whole is **reviewed by hand** |
| Never log or print a credential, token, capability secret, audio, transcript, message text or a filesystem path. | `crates/retinue-daemon/src/diagnostics.rs` (`RedactingWriter`, `redact`, `scrub_events`) | `diagnostics.rs::redaction_removes_tokens_and_credentials_but_keeps_ids`, and `fix39_p12_tests::every_log_call_site_is_a_fixed_message` (scans every `tracing::`/`info!` call site in the daemon and the native shell; `diagnostics.rs`'s module doc still names it by its old title) |
| A machine token is never returned, logged or exported. | `crates/retinue-daemon/src/machines.rs`, `machine_client.rs` | `machines_tests.rs::a_token_is_never_returned_logged_or_exported` |
| A secret never reaches a commit. | `.gitleaks.toml`, the `hygiene` CI job, the opt-in pre-commit hook (`scripts/pre-commit.mjs`) | `scripts/check-secret-config.mjs` (proves every exception still detects a planted secret beside it) |
| Remote devices see only what `apps/desktop/src/remote/tiers.json` allows, and the table is one policy read by Rust and TypeScript. | `crates/retinue-daemon/src/remote/policy.rs` (`TIERS_JSON`), `apps/desktop/src/remote/remoteTiers.ts` | `apps/desktop/src/remote/remoteTiers.parity.test.ts`, `policy.rs::goldens_authorize` / `goldens_results` / `goldens_snapshots` / `goldens_events` |
| Every `owner_*` and `computer_*` method is `never` remote, every event of theirs is `drop`, and the owner's own computer additionally needs the setting, an approval card and an attended conversation. | `remote/tiers.json`, `crates/retinue-daemon/src/owner_computer.rs` | `policy.rs` goldens; `computer_view_refuses_a_wrong_expired_or_unlinked_ticket_without_forwarding` |
| A machine link at operator, and only a `machine` device, may read and manage another machine's Remote settings and devices (17 November 2026). It never changes or removes its own pairing, never approves a machine and never approves its own request, and each change writes a line in that machine's own activity list naming the machine that made it. Every other `remote.*` method stays `never`, and no phone, tablet or browser reaches the subset at any tier. | `apps/desktop/src/remote/tiers.json` (`manage` class), `crates/retinue-daemon/src/remote/web.rs` (`manage_refusal`), `remote/mod.rs` (`actor_of`) | `remote/tests.rs::a_phone_at_operator_cannot_reach_the_manage_subset`, `a_machine_manages_another_machines_devices_and_is_named_in_its_list`, `a_machine_reads_and_answers_a_waiting_request_but_never_its_own`, `the_manage_subset_still_needs_operator`; `machines_tests.rs::a_linked_machine_manages_the_real_gateway_of_another_daemon` |
| Prompts reach a coding agent through stdin or its own protocol, never as shell command text or a large `argv` string. | `crates/retinue-adapter-*` | adapter tests per crate; **no single test states the rule** |

## The daemon owns the decisions

| Rule | Enforced by | Pinned by |
|---|---|---|
| Sessions, turns, permission decisions, usage and storage live in the daemon. The window shows what it was told and shows unknown as unknown. | `crates/retinue-daemon/src/main.rs` (`dispatch`), `team.rs`, `queue.rs` | `quota.rs::codex_quota_normalization_uses_structured_windows_and_keeps_unknowns_null`; the UI half is **reviewed by hand** |
| Budgets are retired: `budget.*` answers `unsupported` and no screen claims budget enforcement. | `main.rs` (`dispatch`) | `retired_budget_pricing_and_subscription_rpcs_do_not_expose_or_write_legacy_records` |
| Unattended work (a scheduled task, a webhook run) brings its own limits and is never in `bypass`. | `automations.rs` (`run_mode`), `tasks.rs` (`is_unattended`), `main.rs` | `automations.rs` tests, `tasks.rs` tests |
| Every event is a row in `app_events` with a sequence number before it is broadcast, so a window that reconnects resumes rather than guesses. | `main.rs` (`AppState::emit`) | exercised by every replay test (`wait_for_replayed`); **no single test states the rule** |
| A failure that returns an error also records one content-free line, so it reaches the diagnostics bundle the owner sends us. | `diagnostics.rs` (`record_failure`), call sites in `main.rs`, `remote/web.rs`, `automations.rs` | `diagnostics::record_failure_writes_one_entry_per_failure_and_carries_no_content`, `remote/tests.rs::a_refused_origin_is_recorded_once_for_the_diagnostics_bundle`, `watchdogs_expire_both_progress_windows_close_sessions_and_release_turns`, `automation_failures_retry_then_pause_and_success_resets` |

## Boundaries between the layers

| Rule | Enforced by | Pinned by |
|---|---|---|
| A crate depends only on the crates in its row, and only the daemon, the speech crate and the native shell may open a connection of their own. | `scripts/check-architecture.mjs` (`CRATE_DEPENDENCIES`, `HTTP_CLIENTS`) | `scripts/check-architecture.test.mjs`; the check runs in CI's `hygiene` job |
| Inside the daemon, an HTTP client lives only in the named modules (`remote/`, `machine_client.rs`, `browser.rs`, `computer.rs`, `preview.rs`, `quota_sources.rs`); a new one needs an allowlist entry with its reason. | `scripts/check-architecture.mjs` (`ALLOWLIST`) | `scripts/check-architecture.test.mjs` |
| The window reaches Tauri only through `apps/desktop/src/transport/` (plus the two legacy bridges), and the phone build exports the same names as refusing stubs. | `transport/nativeBridge.ts` | `transport/noDirectNative.test.ts`, `remote/remoteTauri.parity.test.ts` |
| Production code never imports the preview harness, and never opens its own WebSocket outside the transport seam. | `scripts/check-architecture.mjs` | `scripts/check-architecture.test.mjs` |
| A look stylesheet is a static import, never a dynamic `import()`. | `apps/desktop/src/main.tsx` | `ui/themeBuild.test.ts` |

## Tests and the tree

| Rule | Enforced by | Pinned by |
|---|---|---|
| A test sees only what it created: its own data folder, and the daemon's environment variables removed for its length and restored after. | `crates/retinue-daemon/src/test_env.rs` (`TestEnv`) | `test_env.rs::the_environment_is_quiet_while_the_folder_lives_and_comes_back_after`, `a_state_writes_into_the_tests_own_folder` |
| Formatting mistakes a reviewer cannot see (a BOM, a stray NUL, trailing whitespace, mixed line endings, bytes that are not UTF-8) fail the build. | `scripts/check-format.mjs` | `scripts/check-format.test.mjs` |
| Lanes never commit; the Director reviews every diff, runs every gate and commits. | process, not code | **untested by design** (`.claude/skills/retinue-development/references/pipeline.md`) |
| A test is never weakened or deleted to make a build pass. | process, not code | **untested by design** (golden rule 8 in the skill) |

## How to add a rule here

1. State it in one sentence, as it is today, not as it once was.
2. Name the file that enforces it and the test that would fail if it broke.
3. If there is no test, write **untested** and say who reviews it. A rule with no test is a candidate for one, not a fact.
4. When a rule changes, change this line in the same change; when a test is renamed, this page is part of the rename.
