// Building the one local file and the short summary. Nothing here may contain a
// prompt, a reply or a token: a scenario row keeps an outcome, a fixed evidence
// sentence and a character count, and nothing else.

/** @returns {object} the file shape documented in docs/reports/wave3-evals.md §2 */
export function buildResult({ run, caps }) {
  return {
    kind: 'retinue-evals',
    version: 1,
    startedAt: run.startedAt,
    finishedAt: run.finishedAt,
    runtime: run.runtime,
    caps: {
      attemptTimeoutMs: caps.attemptTimeoutMs,
      concurrency: caps.concurrency,
      totalTimeoutMs: caps.totalTimeoutMs,
      repeats: caps.repeats,
    },
    stopped: run.stopped,
    scenarios: run.rows.map((row) => ({
      id: row.id,
      title: row.title,
      passed: row.passed,
      repeats: row.repeats,
      attempts: row.attempts.map((attempt) => ({
        attempt: attempt.attempt,
        outcome: attempt.outcome,
        evidence: attempt.evidence,
        characters: attempt.characters,
      })),
    })),
    totals: run.totals,
  }
}

/** The human summary: one line per scenario, then the totals. */
export function summaryLines(result) {
  const model = result.runtime.model ? result.runtime.model : 'default model'
  const stamp = result.startedAt.replace(/\.\d{3}Z$/, 'Z')
  const lines = [`Retinue behavioural evals — ${stamp} — ${result.runtime.provider} (${model}), ${result.caps.repeats} repeats`]
  const width = Math.max(...result.scenarios.map((row) => row.id.length)) + 2
  for (const row of result.scenarios) {
    const failing = [...row.attempts].reverse().find((attempt) => attempt.outcome !== 'pass' && attempt.outcome !== 'skipped')
    const evidence = row.attempts[row.attempts.length - 1]?.evidence ?? ''
    lines.push(`${row.id.padEnd(width)}${row.passed}/${row.repeats}  ${failing ? failing.evidence : evidence}`)
  }
  const { attempts, passed, failed, timeout, error, skipped } = result.totals
  lines.push(
    '',
    `${passed}/${attempts} attempts passed` +
      (failed ? `, ${failed} failed` : '') +
      (timeout ? `, ${timeout} timed out` : '') +
      (error ? `, ${error} could not answer` : '') +
      (skipped ? `, ${skipped} skipped` : '') +
      (result.stopped ? ' — the run stopped early' : ''),
  )
  return lines
}
