# Claude turn steering evidence inventory (11 November 2026)

> **What this is:** Evidence inventory for steering a running Claude turn, 11 November 2026.
> **Status:** evidence; immutable record
> **Last updated:** 2026-11-11

Read-only inventory for making `session.steer` work on Claude, which until now
answered `unsupported`. The capability row still reports `evidence: "none"`: the
request shape is documented by Anthropic and present in the installed CLI's own
input schema, but no real mid-turn steer has been accepted on this machine yet.

## Source of the request shape

The Agent SDK TypeScript reference documents `SDKUserMessage.priority`
(https://docs.claude.com/en/api/agent-sdk/typescript, fetched 11 November 2026):

> * `'next'`, or no `priority` field: Claude reads the message in the same turn,
>   as soon as the tool calls it is running finish. If the turn ends first, the
>   message starts the next turn.
> * `'later'`: Claude Code holds the message until the turn ends and sends it as
>   a new turn.
> * `'now'`: interrupts the turn and Claude reads the message next (with
>   `origin: {kind: "human"}` on v2.1.286 or later, work that can move to the
>   background moves there instead).

The documented example sends `{type: "user", message: {role: "user", content},
parent_tool_use_id: null, priority: "now", origin: {kind: "human"}}`, so the
envelope and `parent_tool_use_id` placement are the provider's own.

The same field is in the installed CLI's own input schema: `claude.exe`, Claude
Code **2.1.322** at `%USERPROFILE%\.local\bin\claude.exe`, contains the
stream-json user-message schema fragment

```
priority: B(["now", "next", "later"]).optional()
```

beside `parent_tool_use_id`, so the version in use parses the field rather than
dropping it. Nothing generated is committed.

## Implementation

- `crates/retinue-adapter-claude/src/lib.rs`: `ClaudeAdapter::steer` writes one
  more line to the live process's stdin:
  `{"type":"user","message":{"role":"user","content":<text>},
  "parent_tool_use_id":null,"priority":"next"}`. It writes nothing and answers
  `SteerOutcome::Stale` when the tracked turn is not running (`active_turn`) or
  when `SteerRequest.turn_id` is not the tracked turn id, because a message
  written after the turn ends would start a turn nobody asked for. Empty text is
  `Unsupported`; no open session is `Process`.
- `crates/retinue-daemon/src/main.rs`: `provider_can_steer` now matches `codex`
  and `claude`, so the `runtime.capabilities().settings.steer` row, the
  `exec_gate.<provider>.steer` default and `session.steer` all agree. Claude has
  no seeded gate row; the stored `exec_gate.claude.steer` value stays the
  off-switch.
- OpenCode and the ACP providers are unchanged and still answer `unsupported`.

## What the fixture proves (request shape only)

`crates/retinue-adapter-claude/src/bin/fake_claude.rs` gained
`--fake-hold-turn` (the first user message starts a turn that stays open, a
later one joins it) and `--fake-steer-audit=<path>` (every inbound user message
is appended as JSONL). The adapter tests prove:

- `steering_joins_the_running_turn_without_ending_it`: the steer is `Accepted`,
  the provider sees a second `user` line with `priority: "next"`,
  `parent_tool_use_id: null` and the text, and the held turn answers it.
- `steering_a_turn_that_has_ended_is_stale_and_writes_nothing` and
  `steering_a_turn_id_that_is_not_running_is_stale`: nothing is written, and the
  right turn is still steerable afterwards.
- `steering_refuses_a_missing_session_and_empty_text`.

The daemon tests prove the capability row, the default and the stored
off-switch, and that a steer with nothing running answers `conflict`.

This proves only that Retinue sends the documented message to a fake peer and
routes its answer. It says nothing about the real CLI's acceptance.

## Not verified live

- **Acceptance by a real Claude turn.** An attempt on 11 November 2026 could not
  run: the account answered `You've hit your weekly limit · resets 5pm
  (Europe/Isle_of_Man)`, so `cargo test -p retinue-adapter-claude --test
  live_suite claude_live_permission_change_and_steer -- --nocapture` with
  `RETINUE_LIVE_TESTS=1` skipped at the first turn.
- **Ordering against streamed output.** Where the steered text lands relative to
  already-streamed assistant text, and whether the CLI emits its own echo of the
  message, is unknown; the adapter ignores non-`tool_result` user echoes either
  way.
- **A turn that ends between the check and the write.** `priority: "next"` turns
  such a message into a new turn. The window is two statements wide in the
  adapter; the daemon serialises turn completion through the per-session gate,
  which narrows but does not close it.
- **Steering while a permission prompt is open.** Untested; the docs allow the
  message to wait for the tool calls in flight, and a prompt is not a tool call.
