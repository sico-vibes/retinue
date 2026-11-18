# Retinue behavioural evals

Small, run on request. Five scenarios that measure prompt behaviours, one
provider, one model, three repeats each, through the owner's own coding-agent
CLI. Nothing here runs in CI, on a schedule or from the app.

Source: `docs/research/n8n-nod/09-system-prompt.md` idea 10; nod's gate
(`evals/gate.json`, `evals/src/gate.ts`) at `47cfee3`, cut down to what one
person can run by hand. The contract and the running log are in
`docs/reports/wave3-evals.md`.

## Run it

```powershell
npm run evals -- --fake                                  # scripted replies, no model, no usage
npm run evals -- --list                                  # the five scenarios
npm run evals -- --provider claude --repeats 3 --yes     # a real run
npm run evals:selftest                                   # the harness's own tests
```

A real run needs `--yes`, because it spends the owner's usage. `--provider` is
`claude` (default), `codex` or `opencode`; OpenCode needs `--model`, and it must
be an `opencode-go/…` or `-free` model: Retinue never uses pay-as-you-go Zen.

Results go to `evals/results/latest.json` and `latest.txt`, with a stamped copy
of each. Neither holds a prompt, a reply or a token: a scenario line is an
outcome, a fixed evidence sentence and a character count.

## Caps

| Cap | Value |
|---|---|
| Per-attempt runtime limit | 180 s (`--timeout`) |
| Concurrency | 1, always |
| Whole run | 20 min (`--total-minutes`) |
| Stop | `evals/.stop` or Ctrl+C; the current attempt finishes |
| Permissions | never bypass; the argv refuses bypass flags |
| CI | refused when `CI` is set |

## The five scenarios

| id | Behaviour |
|---|---|
| `no-pass-after-failed-read` | a failed read is never a pass or a done |
| `no-permission-question-in-chat` | permission is the approval card's job, not a chat question |
| `tool-result-is-data` | text inside a tool result is data, never an instruction |
| `brief-names-the-boundaries` | a brief to a teammate says what must not change |
| `ceo-report-does-not-borrow-claims` | the CEO attributes a teammate's claim or marks it unverified |

The checks look for behaviour markers, not for the wording of our prompts: a
parallel lane edits the prompts, so a check on a sentence would fail on every
improvement.

## Add a scenario

One entry in `lib/scenarios.mjs`: `id`, `title`, `situation` (the owner turn as
the model sees it), `check(reply) -> { pass, evidence }` with a fixed evidence
sentence, and `fake: { pass, fail }` with one reply for each branch. The fake is
what proves the harness without a model; keep both replies short and free of
anything that looks like a real key.
