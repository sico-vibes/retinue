// Retinue's OpenUI component library (owner, 9 November 2026): employees may answer with a
// ```openui block written in OpenUI Lang (https://www.openui.com), and the window draws it with
// these components only. Nothing the model writes runs as code: the parser turns the text into
// calls to the components below, and anything unknown is dropped. Links never open on their own,
// images are not fetched, and a button only puts its reply into the composer for the owner to send.
import { useEffect, type ReactNode } from 'react'
import { createLibrary, defineComponent, FormNameContext, useFormName, useRenderNode, useStateField, useTriggerAction } from '@openuidev/react-lang'
import { z } from 'zod/v4'
import { parseChartSpec, parseTableSpec } from '../richData'
import { RichChart } from '../RichChart'
import { RichTable } from '../RichTable'
import { Select as RetinueSelect } from '../Select'

const tone = z.enum(['neutral', 'info', 'good', 'warning', 'bad'])
type Tone = z.infer<typeof tone>
const toneClass = (value: Tone | undefined) => `oui-tone-${value ?? 'neutral'}`

function Nodes({ items }: { items: unknown }) {
  const render = useRenderNode()
  if (!Array.isArray(items)) return <>{render(items)}</>
  return <>{items.map((item, index) => <OneNode key={index} item={item} render={render} />)}</>
}
function OneNode({ item, render }: { item: unknown; render: (value: unknown) => ReactNode }) { return <>{render(item)}</> }

// ── Leaves ───────────────────────────────────────────────────────────────────
const Heading = defineComponent({
  name: 'Heading', description: 'A section heading. level 1 is the largest.',
  props: z.object({ text: z.string(), level: z.number().int().min(1).max(3).optional() }),
  component: ({ props }) => { const Tag = (`h${(props.level ?? 2) + 1}`) as 'h2' | 'h3' | 'h4'; return <Tag className="oui-heading">{props.text}</Tag> },
})
const Text = defineComponent({
  name: 'Text', description: 'A paragraph of plain text. muted makes it secondary.',
  props: z.object({ text: z.string(), muted: z.boolean().optional() }),
  component: ({ props }) => <p className={`oui-text${props.muted ? ' muted' : ''}`}>{props.text}</p>,
})
const Stat = defineComponent({
  name: 'Stat', description: 'One key number with a label, an optional change (like "+12%") and a tone for the change.',
  props: z.object({ label: z.string(), value: z.union([z.string(), z.number()]), change: z.string().optional(), tone: tone.optional() }),
  component: ({ props }) => <div className="oui-stat"><span className="oui-stat-label">{props.label}</span><strong className="oui-stat-value">{String(props.value)}</strong>{props.change && <span className={`oui-stat-change ${toneClass(props.tone)}`}>{props.change}</span>}</div>,
})
const Badge = defineComponent({
  name: 'Badge', description: 'A short status label.',
  props: z.object({ text: z.string(), tone: tone.optional() }),
  component: ({ props }) => <span className={`oui-badge ${toneClass(props.tone)}`}>{props.text}</span>,
})
const Progress = defineComponent({
  name: 'Progress', description: 'A labelled progress bar; value is 0 to 100.',
  props: z.object({ label: z.string(), value: z.number().min(0).max(100), tone: tone.optional() }),
  component: ({ props }) => { const value = Math.max(0, Math.min(100, props.value)); return <div className="oui-progress"><span className="oui-progress-head"><span>{props.label}</span><span>{Math.round(value)}%</span></span><span className="oui-progress-track" role="progressbar" aria-label={props.label} aria-valuenow={Math.round(value)} aria-valuemin={0} aria-valuemax={100}><span className={`oui-progress-fill ${toneClass(props.tone)}`} style={{ width: `${value}%` }} /></span></div> },
})
const Callout = defineComponent({
  name: 'Callout', description: 'A highlighted note: info, good (success), warning or bad (error).',
  props: z.object({ text: z.string(), tone: tone.optional(), title: z.string().optional() }),
  component: ({ props }) => <div className={`oui-callout ${toneClass(props.tone)}`} role="note">{props.title && <strong>{props.title}</strong>}<span>{props.text}</span></div>,
})
const Divider = defineComponent({ name: 'Divider', description: 'A thin separating line.', props: z.object({}), component: () => <hr className="oui-divider" /> })
const List = defineComponent({
  name: 'List', description: 'A bulleted list, or numbered when ordered is true.',
  props: z.object({ items: z.array(z.string()), ordered: z.boolean().optional() }),
  component: ({ props }) => props.ordered ? <ol className="oui-list">{props.items.map((item, index) => <li key={index}>{item}</li>)}</ol> : <ul className="oui-list">{props.items.map((item, index) => <li key={index}>{item}</li>)}</ul>,
})
const Code = defineComponent({
  name: 'Code', description: 'A block of code or command text, shown as is.',
  props: z.object({ code: z.string(), language: z.string().optional() }),
  component: ({ props }) => <pre className="oui-code" data-language={props.language}><code>{props.code}</code></pre>,
})
const Table = defineComponent({
  name: 'Table', description: 'A sortable table. rows is a list of rows, each a list of cells in column order. Numbers stay numbers.',
  props: z.object({ columns: z.array(z.string()), rows: z.array(z.array(z.union([z.string(), z.number(), z.null()]))), title: z.string().optional() }),
  component: ({ props }) => { const parsed = parseTableSpec(JSON.stringify(props)); return parsed.ok ? <RichTable table={parsed.value} label={props.title ?? 'Table'} /> : <p className="oui-invalid">{parsed.reason}</p> },
})
const Chart = defineComponent({
  name: 'Chart', description: 'A bar or line chart. Each series has a name and one number per category. One unit per chart.',
  props: z.object({ type: z.enum(['bar', 'line']), categories: z.array(z.string()), series: z.array(z.object({ name: z.string(), values: z.array(z.number().nullable()) })), title: z.string().optional(), unit: z.string().optional() }),
  component: ({ props }) => { const parsed = parseChartSpec(JSON.stringify(props)); return parsed.ok ? <RichChart spec={parsed.value} label={props.title ?? 'Chart'} embedded /> : <p className="oui-invalid">{parsed.reason}</p> },
})
const Button = defineComponent({
  name: 'Button', description: 'A button that answers you: clicking it puts `reply` (or the label) into the user\'s message box for them to send. Use for choices and next steps.',
  props: z.object({ label: z.string(), reply: z.string().optional(), primary: z.boolean().optional() }),
  component: ({ props }) => { const trigger = useTriggerAction(); return <button type="button" className={props.primary ? 'primary-button small' : 'secondary-button small'} onClick={() => void trigger(props.reply ?? props.label)}>{props.label}</button> },
})

// ── Form fields ──────────────────────────────────────────────────────────────
const TextField = defineComponent({
  name: 'TextField', description: 'A text input inside a Form. name is the field key; multiline for longer text.',
  props: z.object({ name: z.string(), label: z.string(), placeholder: z.string().optional(), multiline: z.boolean().optional() }),
  component: ({ props }) => { const field = useStateField<string>(props.name, ''); return <label className="oui-field"><span>{props.label}</span>{props.multiline ? <textarea className="text-input" rows={3} placeholder={props.placeholder} value={String(field.value ?? '')} onChange={(event) => field.setValue(event.target.value)} /> : <input className="text-input" placeholder={props.placeholder} value={String(field.value ?? '')} onChange={(event) => field.setValue(event.target.value)} />}</label> },
})
const Select = defineComponent({
  name: 'Select', description: 'A choice from a list, inside a Form.',
  props: z.object({ name: z.string(), label: z.string(), options: z.array(z.string()) }),
  component: ({ props }) => { const field = useStateField<string>(props.name, props.options[0] ?? ''); const first = props.options[0] ?? ''; useEffect(() => { if (!field.value && first) field.setValue(first) }, [field, first]); return <div className="oui-field"><span>{props.label}</span><RetinueSelect variant="field" align="left" ariaLabel={props.label} value={String(field.value ?? first)} options={props.options.map((option) => ({ value: option, label: option }))} onChange={(value) => field.setValue(value)} /></div> },
})
const Checkbox = defineComponent({
  name: 'Checkbox', description: 'A yes/no tick box, inside a Form.',
  props: z.object({ name: z.string(), label: z.string() }),
  component: ({ props }) => { const field = useStateField<boolean>(props.name, false); return <label className="oui-check"><input type="checkbox" checked={field.value === true} onChange={(event) => field.setValue(event.target.checked)} /><span>{props.label}</span></label> },
})
const fieldRefs = [TextField.ref, Select.ref, Checkbox.ref] as const
const leafRefs = [Heading.ref, Text.ref, Stat.ref, Badge.ref, Progress.ref, Callout.ref, Divider.ref, List.ref, Code.ref, Table.ref, Chart.ref, Button.ref] as const

// ── Layout ───────────────────────────────────────────────────────────────────
const Row = defineComponent({
  name: 'Row', description: 'Items side by side, wrapping on narrow windows. Good for Stats, Badges and Buttons.',
  props: z.object({ children: z.array(z.union(leafRefs)) }),
  component: ({ props }) => <div className="oui-row"><Nodes items={props.children} /></div>,
})
const Grid = defineComponent({
  name: 'Grid', description: 'A grid with 2 to 4 columns, for Stats of equal weight.',
  props: z.object({ children: z.array(z.union([...leafRefs])), columns: z.number().int().min(1).max(4).optional() }),
  component: ({ props }) => <div className="oui-grid" style={{ ['--oui-columns' as string]: props.columns ?? 3 }}><Nodes items={props.children} /></div>,
})
const Form = defineComponent({
  name: 'Form', description: 'Fields the user fills in, with a submit button. Submitting puts the label and every answer into the user\'s message box for them to send.',
  props: z.object({ name: z.string(), children: z.array(z.union([...fieldRefs, ...leafRefs])), submit: z.string().optional() }),
  component: ({ props }) => <FormNameContext.Provider value={props.name}><FormBody submit={props.submit}><Nodes items={props.children} /></FormBody></FormNameContext.Provider>,
})
function FormBody({ submit, children }: { submit?: string; children: ReactNode }) {
  const trigger = useTriggerAction()
  const name = useFormName()
  return <form className="oui-form" onSubmit={(event) => { event.preventDefault(); void trigger(submit ?? 'Submit', name) }}>{children}<div className="oui-form-actions"><button type="submit" className="primary-button small">{submit ?? 'Submit'}</button></div></form>
}
const Card = defineComponent({
  name: 'Card', description: 'A titled panel grouping related content.',
  props: z.object({ title: z.string(), children: z.array(z.union([...leafRefs, Row.ref, Grid.ref, Form.ref])), subtitle: z.string().optional() }),
  component: ({ props }) => <section className="oui-card"><header><strong>{props.title}</strong>{props.subtitle && <span>{props.subtitle}</span>}</header><div className="oui-card-body"><Nodes items={props.children} /></div></section>,
})
const Tab = defineComponent({
  name: 'Tab', description: 'One tab inside Tabs.',
  props: z.object({ label: z.string(), children: z.array(z.union([...leafRefs, Row.ref, Grid.ref, Card.ref, Form.ref])) }),
  component: ({ props }) => <div className="oui-tab-panel"><Nodes items={props.children} /></div>,
})
const Tabs = defineComponent({
  name: 'Tabs', description: 'Switchable tabs; each is a Tab with its own content.',
  props: z.object({ tabs: z.array(Tab.ref) }),
  component: ({ props }) => <TabsView tabs={props.tabs as unknown[]} />,
})
function TabsView({ tabs }: { tabs: unknown[] }) {
  const render = useRenderNode()
  const field = useStateField<number>('__tab', 0)
  const labels = tabs.map((tab, index) => (tab && typeof tab === 'object' && 'props' in tab && (tab as { props?: { label?: unknown } }).props?.label ? String((tab as { props: { label: unknown } }).props.label) : `Tab ${index + 1}`))
  const active = Math.min(Number(field.value ?? 0) || 0, Math.max(0, tabs.length - 1))
  return <div className="oui-tabs"><div className="oui-tab-list" role="tablist">{labels.map((label, index) => <button key={index} type="button" role="tab" aria-selected={index === active} onClick={() => field.setValue(index)}>{label}</button>)}</div>{tabs[active] !== undefined && render(tabs[active])}</div>
}
const Stack = defineComponent({
  name: 'Stack', description: 'The root: content stacked top to bottom.',
  props: z.object({ children: z.array(z.union([...leafRefs, Row.ref, Grid.ref, Card.ref, Tabs.ref, Form.ref])) }),
  component: ({ props }) => <div className="oui-stack"><Nodes items={props.children} /></div>,
})

export const retinueOpenUi = createLibrary({
  root: 'Stack',
  components: [Stack, Card, Tabs, Tab, Row, Grid, Form, Heading, Text, Stat, Badge, Progress, Callout, Divider, List, Code, Table, Chart, Button, TextField, Select, Checkbox],
  componentGroups: [
    { name: 'Layout', components: ['Stack', 'Card', 'Tabs', 'Tab', 'Row', 'Grid'] },
    { name: 'Content', components: ['Heading', 'Text', 'Stat', 'Badge', 'Progress', 'Callout', 'Divider', 'List', 'Code', 'Table', 'Chart'] },
    { name: 'Input', components: ['Button', 'Form', 'TextField', 'Select', 'Checkbox'], notes: ['Buttons and forms never act by themselves: they put a reply in the user\'s message box, and the user decides whether to send it.'] },
  ],
})
