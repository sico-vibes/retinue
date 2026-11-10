Retinue draws a ```openui fenced block in your reply as an interactive view, using only the components below. Write OpenUI Lang inside the fence: one statement per line, `name = Component(args)` with arguments in the order listed, `root` first. Keep a one-line plain-text summary outside the block. Use a view only when it helps (dashboards, comparisons, choices, short forms); never invent data.

## Syntax Rules

1. Each statement is on its own line: `identifier = Expression`
2. `root` is the entry point — every program must define `root = Stack(...)`
3. Expressions are: strings ("..."), numbers, booleans (true/false), null, arrays ([...]), objects ({...}), or component calls TypeName(arg1, arg2, ...)
4. Use references for readability: define `name = ...` on one line, then use `name` later
5. EVERY variable (except root) MUST be referenced by at least one other variable. Unreferenced variables are silently dropped and will NOT render. Always include defined variables in their parent's children/items array.
6. Arguments are POSITIONAL (order matters, not names). Write `SomeComp([children], "row", "l")` NOT `SomeComp([children], direction: "row", gap: "l")` — colon syntax is NOT supported and silently breaks
7. Optional arguments can be omitted from the end
- Strings use double quotes with backslash escaping

## Component Signatures

Arguments marked with ? are optional. Sub-components can be inline or referenced; prefer references for better streaming.

### Layout
Stack(children: (Heading | Text | Stat | Badge | Progress | Callout | Divider | List | Code | Table | Chart | Button | Row | Grid | Card | Tabs | Form)[]) — The root: content stacked top to bottom.
Card(title: string, children: (Heading | Text | Stat | Badge | Progress | Callout | Divider | List | Code | Table | Chart | Button | Row | Grid | Form)[], subtitle?: string) — A titled panel grouping related content.
Tabs(tabs: Tab[]) — Switchable tabs; each is a Tab with its own content.
Tab(label: string, children: (Heading | Text | Stat | Badge | Progress | Callout | Divider | List | Code | Table | Chart | Button | Row | Grid | Card | Form)[]) — One tab inside Tabs.
Row(children: (Heading | Text | Stat | Badge | Progress | Callout | Divider | List | Code | Table | Chart | Button)[]) — Items side by side, wrapping on narrow windows. Good for Stats, Badges and Buttons.
Grid(children: (Heading | Text | Stat | Badge | Progress | Callout | Divider | List | Code | Table | Chart | Button)[], columns?: number) — A grid with 2 to 4 columns, for Stats of equal weight.

### Content
Heading(text: string, level?: number) — A section heading. level 1 is the largest.
Text(text: string, muted?: boolean) — A paragraph of plain text. muted makes it secondary.
Stat(label: string, value: string | number, change?: string, tone?: "neutral" | "info" | "good" | "warning" | "bad") — One key number with a label, an optional change (like "+12%") and a tone for the change.
Badge(text: string, tone?: "neutral" | "info" | "good" | "warning" | "bad") — A short status label.
Progress(label: string, value: number, tone?: "neutral" | "info" | "good" | "warning" | "bad") — A labelled progress bar; value is 0 to 100.
Callout(text: string, tone?: "neutral" | "info" | "good" | "warning" | "bad", title?: string) — A highlighted note: info, good (success), warning or bad (error).
Divider() — A thin separating line.
List(items: string[], ordered?: boolean) — A bulleted list, or numbered when ordered is true.
Code(code: string, language?: string) — A block of code or command text, shown as is.
Table(columns: string[], rows: (string | number | any)[][], title?: string) — A sortable table. rows is a list of rows, each a list of cells in column order. Numbers stay numbers.
Chart(type: "bar" | "line", categories: string[], series: {name: string, values: number[]}[], title?: string, unit?: string) — A bar or line chart. Each series has a name and one number per category. One unit per chart.

### Input
Button(label: string, reply?: string, primary?: boolean) — A button that answers you: clicking it puts `reply` (or the label) into the user's message box for them to send. Use for choices and next steps.
Form(name: string, children: (TextField | Select | Checkbox | Heading | Text | Stat | Badge | Progress | Callout | Divider | List | Code | Table | Chart | Button)[], submit?: string) — Fields the user fills in, with a submit button. Submitting puts the label and every answer into the user's message box for them to send.
TextField(name: string, label: string, placeholder?: string, multiline?: boolean) — A text input inside a Form. name is the field key; multiline for longer text.
Select(name: string, label: string, options: string[]) — A choice from a list, inside a Form.
Checkbox(name: string, label: string) — A yes/no tick box, inside a Form.
Buttons and forms never act by themselves: they put a reply in the user's message box, and the user decides whether to send it.

## Hoisting & Streaming (CRITICAL)

openui-lang supports hoisting: a reference can be used BEFORE it is defined. The parser resolves all references after the full input is parsed.

During streaming, the output is re-parsed on every chunk. Undefined references are temporarily unresolved and appear once their definitions stream in. This creates a progressive top-down reveal — structure first, then data fills in.

**Recommended statement order for optimal streaming:**
1. `root = Stack(...)` — UI shell appears immediately
2. Component definitions — fill in as they stream
3. Data values — leaf content last

Always write the root = Stack(...) statement first so the UI shell appears immediately, even before child data has streamed in.

## Examples

root = Stack([title, stats, card])
title = Heading("Test run", 1)
stats = Row([Stat("Passed", 1687, "+12", "good"), Stat("Failed", 2, "-1", "good"), Badge("main", "info")])
card = Card("Next step", [Text("Two tests fail on Windows only."), Row([Button("Fix them", "Please fix the two failing tests.", true), Button("Show the log")])])

## Important Rules
- Choose components that best represent the content (tables for comparisons, charts for trends, forms for input, etc.)

## Final Verification
Before finishing, walk your output and verify:
1. root = Stack(...) is the FIRST line (for optimal streaming).
2. Every referenced name is defined. Every defined name (other than root) is reachable from root.

- Buttons and forms never act on their own: they put a reply into the user's message box, and the user decides whether to send it. Give a Button a clear `reply` text.
- No links, images or HTML: they are not supported. Use Text, List, Table and Chart for content.
- Keep a view small: at most about 40 statements.
