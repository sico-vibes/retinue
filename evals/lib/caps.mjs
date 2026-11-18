// The caps this runner brings, because the work can spend the owner's usage
// (`AGENTS.md`: unattended work brings its own caps; budgets are retired, so
// this file says caps and never budget).

/** Fixed evidence sentences for outcomes no reply produced. */
export const STOP_EVIDENCE = 'the run stopped before this attempt'
export const TIMEOUT_EVIDENCE = 'the runtime did not answer within the limit'
export const ERROR_EVIDENCE = 'the runtime could not answer'

export const CAPS = {
  /** One scenario attempt may run this long before the CLI is killed. */
  attemptTimeoutMs: 180_000,
  /** The whole run stops after this; the remaining attempts are skipped. */
  totalTimeoutMs: 20 * 60_000,
  /** One attempt in flight, ever. Enforced by the serial loop in suite.mjs. */
  concurrency: 1,
  repeats: 3,
}

export class EvalRefused extends Error {
  constructor(code) {
    super(code)
    this.code = code
  }
}

/** Never in CI: a workflow setting `CI` makes this exit instead of spending. */
export function refuseWhenCi(env = process.env) {
  if (env.CI) throw new EvalRefused('ci')
}

/** Three repeats is the owner's shape; keep a hand-typed run small. */
export function validateRepeats(value) {
  const repeats = Number(value)
  if (!Number.isInteger(repeats) || repeats < 1 || repeats > 10) throw new EvalRefused('repeats')
  return repeats
}

export function validateMinutes(value) {
  const minutes = Number(value)
  if (!Number.isFinite(minutes) || minutes <= 0 || minutes > 240) throw new EvalRefused('minutes')
  return Math.round(minutes * 60_000)
}

export function validateSeconds(value) {
  const seconds = Number(value)
  if (!Number.isFinite(seconds) || seconds < 10 || seconds > 3_600) throw new EvalRefused('timeout')
  return Math.round(seconds * 1_000)
}
