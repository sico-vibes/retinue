// @vitest-environment happy-dom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'
import { ChangeReview } from './ExperimentalFeatures'

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

it('uses the conversation folder and gives counts a sibling column outside the path button', () => {
  const host = document.createElement('div')
  const root = createRoot(host)
  const folder = 'C:/work/site-retinue/own-copy'
  const inside = String.raw`c:\work\site-retinue\own-copy\apps\desktop\src\ui\sessionSuggestions.ts`
  const outside = String.raw`C:\work\site-retinue\own-copy-other\outside.ts`
  const diff = '@@ -1 +1,2 @@\n-old\n+new\n+test'
  const onDiff = vi.fn()
  try {
    act(() => root.render(<ChangeReview folder={folder} files={[{ id: 'in', path: inside, detail: { diff } }, { id: 'out', path: outside, detail: { diff } }]} dismissed={[]} onKeep={vi.fn()} onRevert={vi.fn()} onDiff={onDiff} />))
    const [first, second] = host.querySelectorAll('li')
    const button = first!.querySelector<HTMLButtonElement>('.review-path')!
    expect(button.querySelector('.path-tail')?.textContent).toBe('apps/desktop/src/ui/sessionSuggestions.ts')
    expect(button.title).toBe(inside)
    expect(button.getAttribute('aria-label')).toBe(inside)
    expect(button.nextElementSibling?.className).toBe('review-counts')
    expect(button.nextElementSibling?.getAttribute('aria-label')).toBe('2 added, 1 removed')
    expect(first!.children).toHaveLength(4)
    expect(second!.querySelector('.path-tail')?.textContent).toBe('C:/work/site-retinue/own-copy-other/outside.ts')
    act(() => button.click())
    expect(onDiff).toHaveBeenCalledWith(inside, diff)
  } finally {
    act(() => root.unmount())
  }
})
