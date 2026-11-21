import type { Step } from './scenarios'

export const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms))
export async function until<T>(read: () => T | null | undefined | false, ms: number): Promise<T | null> {
  const end = Date.now() + ms
  for (;;) {
    const value = read()
    if (value) return value
    if (Date.now() > end) return null
    await wait(120)
  }
}
const byText = (doc: Document, text: string) => {
  const shown = [...doc.querySelectorAll<HTMLElement>('button, [role="button"], [role="menuitem"], a, summary')].filter((element) => element.getClientRects().length > 0)
  return shown.find((element) => element.textContent?.trim().startsWith(text)) ?? shown.find((element) => element.textContent?.includes(text))
}

/** Clicks and typing inside a scenario's frame, so a state only a person's action reaches opens by itself. */
export async function runSteps(doc: () => Document | null, steps: readonly Step[]): Promise<string | null> {
  for (const step of steps) {
    if ('click' in step) {
      const element = await until(() => doc()?.querySelector<HTMLElement>(step.click), 8000)
      if (!element) return `no element ${step.click}`
      element.click()
    } else if ('clickText' in step) {
      const element = await until(() => { const current = doc(); return current ? byText(current, step.clickText) : null }, 8000)
      if (!element) return `nothing to click called "${step.clickText}"`
      element.click()
    } else if ('hover' in step) {
      const element = await until(() => doc()?.querySelector<HTMLElement>(step.hover), 8000)
      if (!element) return `no element ${step.hover}`
      element.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    } else {
      const [selector, text] = step.type
      const element = await until(() => doc()?.querySelector<HTMLInputElement | HTMLTextAreaElement>(selector), 8000)
      if (!element) return `no field ${selector}`
      const win = element.ownerDocument.defaultView
      const proto = Object.getPrototypeOf(element)
      element.focus()
      Object.getOwnPropertyDescriptor(proto, 'value')?.set?.call(element, text)
      if (element instanceof (win?.HTMLTextAreaElement ?? HTMLTextAreaElement)) element.setSelectionRange(text.length, text.length)
      element.dispatchEvent(new (win?.Event ?? Event)('input', { bubbles: true }))
    }
    await wait(250)
  }
  return null
}
