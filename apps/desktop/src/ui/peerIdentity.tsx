// Who a teammate is shown as. In Professional mode (owner, 25 November 2026: "I don't want to see any of the employees names")
// a teammate is its provider: "OpenCode" with the OpenCode logo, never "Pololo" with a cartoon. Employees mode is unchanged.
import { createContext, useContext, type ReactNode } from 'react'
import type { Agent, Runtime } from '../types'
import { RuntimeMark, providerBrand } from './providerBrand'
import { useWorkMode } from './workMode'

export const RuntimesContext = createContext<readonly Runtime[]>([])

/** The provider name Professional mode shows for an employee, or the fallback when its runtime is unknown. */
export function providerNameOf(agent: Pick<Agent, 'runtimeId'> | null | undefined, runtimes: readonly Runtime[], fallback = 'Subagent'): string {
  const provider = runtimes.find((runtime) => runtime.id === agent?.runtimeId)?.provider
  return provider ? providerBrand(provider).name : fallback
}

export function usePeerIdentity(agent: Agent | null | undefined, name: string): { professional: boolean; name: string; logo: (size: number) => ReactNode } {
  const professional = useWorkMode() === 'professional'
  const runtimes = useContext(RuntimesContext)
  const provider = runtimes.find((runtime) => runtime.id === agent?.runtimeId)?.provider
  return {
    professional,
    name: professional ? providerNameOf(agent, runtimes) : name,
    logo: (size) => <span className="peer-logo" style={{ width: size, height: size }} aria-hidden="true"><RuntimeMark provider={provider} size={Math.round(size * 0.78)} /></span>,
  }
}
