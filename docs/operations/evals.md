# Behavioural evals

Retinue has five small behavioural evals of prompt behaviour. They are the only
thing that measures whether a prompt change made a bot better or worse, and the
only part of the repository that spends the owner's usage.

- **Run on request only.** Never in CI, on a schedule or from the app: the
  runner refuses to start when `CI` is set, and a real run needs `--yes`.
- **Small:** five scenarios, one provider, one model, three repeats each.
- **The owner's own CLIs,** with their own sign-in: nothing reads, copies or
  passes a credential. OpenCode may only use `opencode-go/…` or a `-free` model.
- **Caps, not budgets** (budgets are retired): 180 s per attempt, one attempt at
  a time, 20 minutes for the whole run, a stop file or Ctrl+C, and never a
  permission bypass.

## Run it

```powershell
npm run evals -- --fake                                  # prove the harness, no model, no usage
npm run evals -- --provider claude --repeats 3 --yes     # a real run
npm run evals:selftest                                   # the harness's tests
```

Results: `evals/results/latest.json` and `latest.txt`, each with a stamped copy.
They hold no prompt, reply or token: one outcome, one fixed evidence sentence
and a character count per attempt.

Details: [`evals/README.md`](../../evals/README.md) and the contract in
[`docs/reports/wave3-evals.md`](../reports/wave3-evals.md).

## What is verified and what is not

The harness is proven by scripted replies: one passing and one failing reply per
scenario, a timeout, a stop, the caps, the refusal of bypass flags and paid Zen
models, and no reply text in the output files (`npm run evals:selftest`). No real
model run was made on the owner's machine by the lane that built it, because
that spends the owner's usage. The CLI command shapes were read from each CLI's
`--help` and from `references/pipeline.md`; the first real run should confirm
them.
