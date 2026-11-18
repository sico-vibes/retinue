// One serial loop over scenarios and repeats. Serial on purpose: the cap is one
// attempt in flight, so a model call, a CLI's own child and the machine's load
// never overlap, and a stopped run leaves nothing running behind it.

import { existsSync } from 'node:fs'
import { ERROR_EVIDENCE, STOP_EVIDENCE, TIMEOUT_EVIDENCE } from './caps.mjs'
import { EvalTimeout, promptFor } from './runtime.mjs'

/**
 * The attempt cap belongs to the suite, not only to the runtime: a runtime that
 * forgets its own timer still cannot hold the run open. The CLI runtime also
 * kills its child at the same limit.
 */
function withTimeout(promise, ms) {
  let timer
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new EvalTimeout()), ms)
  })
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer))
}

/**
 * @param {{
 *   scenarios: Array<{id: string, title: string, check: (text: string) => {pass: boolean, evidence: string}}>,
 *   runtime: {name: string, model: string|null, ask: Function},
 *   repeats: number,
 *   attemptTimeoutMs: number,
 *   totalTimeoutMs: number,
 *   stopFile?: string|null,
 *   instructions?: string|null,
 *   now?: () => number,
 *   onAttempt?: (row: object, attempt: object) => void,
 * }} options
 */
export async function runSuite({
  scenarios,
  runtime,
  repeats,
  attemptTimeoutMs,
  totalTimeoutMs,
  stopFile = null,
  instructions = null,
  now = Date.now,
  onAttempt = () => {},
}) {
  const startedMs = now()
  const startedAt = new Date(startedMs).toISOString()
  const totals = { attempts: 0, passed: 0, failed: 0, timeout: 0, error: 0, skipped: 0 }
  let stopped = false
  let inFlight = 0
  let maxInFlight = 0
  const rows = []

  for (const scenario of scenarios) {
    const row = { id: scenario.id, title: scenario.title, passed: 0, repeats, attempts: [] }
    for (let attempt = 1; attempt <= repeats; attempt += 1) {
      if (stopFile && existsSync(stopFile)) stopped = true
      if (!stopped && now() - startedMs >= totalTimeoutMs) stopped = true

      let result
      if (stopped) {
        result = { attempt, outcome: 'skipped', evidence: STOP_EVIDENCE, characters: 0 }
      } else {
        const prompt = promptFor(scenario, instructions)
        inFlight += 1
        maxInFlight = Math.max(maxInFlight, inFlight)
        try {
          const text = await withTimeout(
            runtime.ask(prompt, {
              timeoutMs: attemptTimeoutMs,
              scenarioId: scenario.id,
              attempt,
              repeats,
            }),
            attemptTimeoutMs,
          )
          const reply = typeof text === 'string' ? text : ''
          const verdict = scenario.check(reply)
          result = { attempt, outcome: verdict.pass ? 'pass' : 'fail', evidence: verdict.evidence, characters: reply.length }
        } catch (error) {
          const timeout = error instanceof EvalTimeout || error?.code === 'timeout'
          result = {
            attempt,
            outcome: timeout ? 'timeout' : 'error',
            evidence: timeout ? TIMEOUT_EVIDENCE : ERROR_EVIDENCE,
            characters: 0,
          }
        } finally {
          inFlight -= 1
        }
      }

      row.attempts.push(result)
      if (result.outcome === 'pass') row.passed += 1
      if (result.outcome === 'skipped') totals.skipped += 1
      else {
        totals.attempts += 1
        if (result.outcome === 'pass') totals.passed += 1
        if (result.outcome === 'fail') totals.failed += 1
        if (result.outcome === 'timeout') totals.timeout += 1
        if (result.outcome === 'error') totals.error += 1
      }
      onAttempt(row, result)
    }
    rows.push(row)
  }

  return {
    startedAt,
    finishedAt: new Date(now()).toISOString(),
    runtime: { provider: runtime.name, model: runtime.model ?? null },
    rows,
    totals,
    stopped,
    maxInFlight,
  }
}
