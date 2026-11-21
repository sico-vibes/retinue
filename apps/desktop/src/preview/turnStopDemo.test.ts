import { describe, expect, it } from 'vitest'
import type { Session } from '../types'
import { latestTurnStop, turnStopCopy } from '../ui/conversation'
import { sceneFor } from './fixtureScenes'
import { SCENARIOS } from './inspector/scenarios'

describe('turn stop demo coverage', () => {
  it.each(['turn-failed-before-reply', 'turn-failed', 'turn-stalled', 'turn-cancelled'])('registers %s with a visible reason', (id) => {
    const scenario = SCENARIOS.find((entry) => entry.id === `scene-${id}`)
    expect(scenario?.expect).toBeTruthy()
    const fixture = sceneFor(id)!
    const stop = latestTurnStop({ id, runtimeId: 'demo', messages: fixture.messages, ...fixture.session } as Session)
    expect(stop?.activity && turnStopCopy(stop.activity).reason).toContain(scenario!.expect)
  })
})
