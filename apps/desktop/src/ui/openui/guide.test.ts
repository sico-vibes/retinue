// The guide the daemon gives bots (`ui_guide` tool) is generated from the same library the
// window draws with, so the two never disagree. Regenerate with UPDATE_OPENUI_GUIDE=1.
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { createParser } from '@openuidev/react-lang'
import { retinueOpenUi } from './library'
import { replyText } from './OpenUiBlock'

const GUIDE = fileURLToPath(new URL('../../../../../crates/retinue-daemon/src/openui_guide.md', import.meta.url))
const EXAMPLE = `root = Stack([title, stats, card])
title = Heading("Test run", 1)
stats = Row([Stat("Passed", 1687, "+12", "good"), Stat("Failed", 2, "-1", "good"), Badge("main", "info")])
card = Card("Next step", [Text("Two tests fail on Windows only."), Row([Button("Fix them", "Please fix the two failing tests.", true), Button("Show the log")])])`

export function guideText() {
  return retinueOpenUi.prompt({
    preamble: 'Retinue draws a ```openui fenced block in your reply as an interactive view, using only the components below. Write OpenUI Lang inside the fence: one statement per line, `name = Component(args)` with arguments in the order listed, `root` first. Keep a one-line plain-text summary outside the block. Use a view only when it helps (dashboards, comparisons, choices, short forms); never invent data.',
    additionalRules: [
      'Buttons and forms never act on their own: they put a reply into the user\'s message box, and the user decides whether to send it. Give a Button a clear `reply` text.',
      'No links, images or HTML: they are not supported. Use Text, List, Table and Chart for content.',
      'Keep a view small: at most about 40 statements.',
    ],
    examples: [EXAMPLE],
  })
}

describe('OpenUI guide', () => {
  it('parses its own example with no errors', () => {
    const result = createParser(retinueOpenUi.toJSONSchema(), retinueOpenUi.root).parse(EXAMPLE)
    expect(result.meta.errors).toEqual([])
    expect(result.root).toBeTruthy()
  })
  it('matches the guide the daemon serves', () => {
    const text = `${guideText().trim()}\n`
    if (process.env.UPDATE_OPENUI_GUIDE) writeFileSync(GUIDE, text)
    expect(readFileSync(GUIDE, 'utf8').replace(/\r\n/g, '\n')).toBe(text)
  })
  it('turns a form into a plain reply', () => {
    expect(replyText({ humanFriendlyMessage: 'Book it', formState: { date: 'Friday', urgent: true, __tab: 1, empty: '' }, params: {} })).toBe('Book it\ndate: Friday\nurgent: yes')
  })
})
