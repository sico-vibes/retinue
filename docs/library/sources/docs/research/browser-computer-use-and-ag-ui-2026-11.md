# Browser use, computer use, server preview, and AG-UI

> **What this is:** research for the owner's question: should Retinue give its bots computer use, browser use, and a preview of servers the bots start, and is the AG-UI protocol useful to Retinue?
> **Status:** research. No code changed, nothing committed.
> **Date:** 11 November 2026. Research bot: Grey.

**How to read the claim markers.** Each claim ends with one of:

- **[code]** verified by reading the source at the pinned commit in "Sources".
- **[docs]** taken from project documentation (the project's own prose, not the code).
- **[public]** public material: a repository page, API metadata, a product page. Not read in full.
- **[unchecked]** not verified this round. Named in "What I could not verify" where it matters.

**What was read.** `AGENTS.md`, `.claude/skills/retinue-development/SKILL.md` and its `codebase-map.md`. Two repositories cloned shallow into `%TEMP%\opencode\ctx` and removed at the end: `CopilotKit/OpenBot` at `b01ac8c` and `pingdotgg/t3code` at `d2c2595`. The AG-UI documentation site (docs.ag-ui.com) and its repository metadata. Retinue's own code and docs in this checkout.

---

## The ten findings that matter most

Written last, after the four parts.

1. **OpenBot's "computer use" is not desktop control.** Its bot computer is a Linux container running one Chromium driven by Playwright 1.62.1. `computer_click`, `computer_type`, `computer_key` and `computer_scroll` are page verbs addressed by element reference, and a screenshot is a page screenshot with sensitive fields masked. There is no mouse, no keyboard and no window for another application. **[code]** (`agent-computer/src/index.ts:105-130`, `:1725-1745`; `agent-computer/package.json`)
2. **OpenBot keeps the decision outside the thing that acts.** The `agent-computer` process has "no policy engine and no audit trail of its own"; the server gateway resolves the target, evaluates a CEL policy, writes an audit row, and only then calls the computer. A computer holds a per-computer HMAC token, never the deployment's. **[docs]** (`docs/architecture.md:43-51`, `:88-100`) **[code]** (`agent-computer/src/index.ts:105-110`, `authorisation.ts`)
3. **OpenBot's takeover is a "who has the wheel" state machine, not a pause button.** While a person holds control, bot actions are refused with a sentence, not queued; a handback forces a fresh snapshot before the next action; credentials never pass through the bot because a private form types them into the page from the server side. **[code]** (`agent-computer/src/control.ts:38-56`, `sign-in.ts:1-23`) **[docs]** (`docs/architecture.md:154-172`)
4. **OpenBot reaches its providers through AG-UI frontend tools**, registered with CopilotKit's `useFrontendTool` and backed by server-side copies of the same contract for runs with no browser attached. Not MCP, not per provider. **[code]** (`app/src/lib/copilot/computer-tools.tsx:336-500`, `server/src/computer/headless-tools.ts:117-270`)
5. **CopilotKit/OpenBot and nightly-labs/openbot are two different projects.** Different code (Bun/Hono/PostgreSQL/Tauri versus Electron/SQLite), different licences (MIT versus PolyForm Noncommercial), both active on 11 November 2026. I found no stated lineage. The earlier report read `nightly-labs/openbot@712e797`; this one reads `CopilotKit/OpenBot@b01ac8c`. **[code]** (both `LICENSE`; repository metadata) **[unchecked]** (whether the two share people)
6. **T3 Code makes the preview the browser, and the agent an MCP client of it.** A headless Chromium in the *server* (`playwright-core`) is driven over CDP; the desktop shares the tab the person is looking at through a CDP relay into Electron `webContents`; the agent gets 12 MCP tools, including `preview_snapshot` (visible text, `aria-ref` elements, console, network, screenshot) and `preview_act`. **[code]** (`apps/server/src/mcp/toolkits/preview/tools.ts:53-238`, `apps/desktop/src/preview/CdpRelay.ts:1-31`, `apps/server/src/preview/ServerBrowser.ts:957-1009`)
7. **T3 Code finds a dev server by process, not by port.** It reads listeners (`netstat.exe` on Windows, `/proc` on Linux, `lsof` on macOS), keeps only ports whose HTTP probe answers HTML, and attributes a listener to the terminal or agent that started it, using process ids it registered and a `T3CODE_THREAD_ID` environment variable baked into agent launches so children inherit it. **[code]** (`apps/server/src/preview/PortScanner.ts:64-70`, `:644-663`, `:822-841`)
8. **T3 Code's remote story is the one Retinue lacks:** "browser tabs belong to the environment", other devices stream them, `localhost` resolves to the host, and SSH environments get a real `ssh` port forward. A single control lease decides whether the person or the agent may type. **[docs]** (`docs/user/remote-access.md:239-260`, `:208-213`) **[code]** (`packages/ssh/src/tunnel.ts`, `apps/server/src/preview/SessionControl.ts:25-50`)
9. **AG-UI is an event stream, not a UI kit, and not a competitor to ACP or MCP.** It standardises how an agent run surfaces to a front end: lifecycle, text deltas, tool calls, state snapshots and JSON Patch deltas, interrupts, subagents. Transport is not mandated; its reference client speaks SSE. Retinue's daemon already carries a superset of the parts that matter per turn (streamed text, tool rows, approvals). **[docs]** (AG-UI architecture and events pages)
10. **My recommendation, in one line each.** Build a **server preview** (small, high value). Give bots **browser use only through a gated, separate-process browser server, and only after the preview exists** (large). **Do not build desktop computer use** in any form that sends screenshots of the owner's screen to a provider. **Ignore AG-UI for now**, with the inward direction (an AG-UI agent hired as an employee) named as the one to revisit if "bring your own agent" becomes a product goal. **[judgement]**

---

## Part 1: computer use in OpenBot

### 1.0 Which repository this is

The owner pointed at `https://github.com/CopilotKit/OpenBot`. The earlier round read `https://github.com/nightly-labs/openbot`. They are **not the same project**:

| | CopilotKit/OpenBot (this report) | nightly-labs/openbot (earlier round) |
|---|---|---|
| Commit read | `b01ac8c7cfcca170b097c07cb4c2f331934fa0c2` | `712e797303c5d41ee256c8360645718a0beb13b2` |
| Licence | MIT (`LICENSE:1-3`) **[code]** | PolyForm Noncommercial 1.0.0 **[code]** |
| Shape | Bun workspace: `app` (React/Vite), `server` (Hono), `worker`, `agent-computer`, `desktop` (Tauri 2), `extension`, PostgreSQL | Electron app: `src/backend`, `src/renderer`, SQLite |
| Agent protocol | AG-UI endpoints; "Bring any AG-UI agent" **[docs]** (`README:3`, `docs/architecture.md:3`) | provider descriptors (13 ids), per-provider clients |
| Stars, last push | 10,962, 2026-11-11 **[public]** | 6,993, 2026-11-11 **[public]** |

The two do not share file layout, database or licence, and CopilotKit's repository never mentions nightly-labs (searched `README.md`, `CHANGELOG.md`, `docs/*.md`). **[code]** I cannot say whether the same people are behind both. **[unchecked]**

### 1.1 What OpenBot uses for computer use

**The driver is Playwright, and the "computer" is a browser.** `agent-computer` is a Bun HTTP process (`PORT=4100`) that owns one long-lived Chromium per bot. **[code]** `agent-computer/src/index.ts:105-130` says it in its own words:

> "The Bot's computer: one long-lived browser, reachable over HTTP. Acting on a page lives in this process because only this process holds the browser."

- **Library:** `"playwright": "1.62.1"` pinned in `agent-computer/package.json`. **[code]**
- **Browser:** Chromium installed inside the image with `playwright install --with-deps --no-shell chromium`, from an `ubuntu:24.04` base. **[code]** (`agent-computer/Dockerfile:29`, `:50-54`)
- **Screenshots:** `GET /screenshot` returns base64 PNG, with Playwright's `mask` over sensitive fields and fields a person filled. **[code]** (`agent-computer/src/index.ts:1725-1745`)
- **Reading a page back:** two ways. Readable text (`GET /read`), and an element list built from Playwright's `ariaSnapshot({ mode: "ai" })`, parsed into `role "name" [ref=eN]` descriptors, capped at 200 elements, filtered to an ARIA role allow-list. **[code]** (`agent-computer/src/aria-snapshot.ts:1-49`, `index.ts:598-620`)
- **Input:** addressed by reference, not pixels. `/click`, `/type`, `/key` and `/scroll` take a `ref` that came from a snapshot, and a stale ref is refused ("take a fresh snapshot"). **[code]** (`index.ts:116-120`, `:2054-2210`)
- **The operator's own input** (when the person has taken over the live screen) goes through `Input.dispatchKeyEvent` and mouse CDP calls in `screencast.ts` and the `/human/*` routes. **[code]** (`agent-computer/src/screencast.ts:15-45`, `index.ts:1474-1493`)
- **A virtual display exists for headedness:** `Xvfb -screen 0 1280x800x24 -nolisten tcp` is started when the browser mode is not headless, because headed Chromium passes more bot checks. **[code]** (`agent-computer/src/virtual-display.ts:84-99`; the reason is in `docs/architecture.md:164`)

**What it is not.** There is no desktop automation: no OS-level mouse or keyboard, no window management, no "open an app". Every one of the 14 `computer_*` tools operates on pages, files in `/workspace`, or a bash shell inside the container. **[code]** (`server/src/computer/headless-tools.ts:117-270`; the tool names are enumerated in `shared/tool-name.ts:609-681`)

**Operating systems.** The bot's computer is always Linux, because it is a container. The *app* ships for macOS (arm64, x64) and Windows x64; on Windows the desktop app runs the deployment through WSL 2 and installs a container engine itself. **[docs]** (`README:18-20`, `docs/desktop-install.md:12-21`) So OpenBot works on Windows, but the thing doing "computer use" runs under WSL as Linux. **[docs]** The person's own browser is the exception: it is their installed Chrome or Edge opened as a native window on a dedicated profile, which is genuinely Windows-native. **[code]** (`agent-computer/src/person-browser.ts:1-25`, `native-browser.ts:1-28`)

### 1.2 How it is exposed to the coding agents

**Through AG-UI frontend tools, in two flavours sharing one contract.**

1. **With a browser attached (a person is watching a thread):** the React app registers the tools with CopilotKit's `useFrontendTool`, so they are client-provided tools passed in `RunAgentInput.tools` and executed in the renderer, which calls the server. **[code]** (`app/src/lib/copilot/computer-tools.tsx:24`, `:336-500`; the mapping table at `:308-312` decides which tools a bot is offered)
2. **Headless (a routine, a goal, a group turn, no browser):** the server builds the *same* tools as `HeadlessTool` values and binds the bot and the person "before the model supplies any arguments". **[code]** (`server/src/computer/headless-tools.ts:36-50`, `:117-270`) **[docs]** (`docs/architecture.md:138-140`)
3. **Every action then crosses one gateway.** `server/src/computer/gateway.ts` is the action boundary: resolve target → evaluate policy → write an audit row → call the computer → write a second row if the forwarded action fails. **[docs]** (`docs/architecture.md:43-51`)

There is no MCP browser server and no per-provider browser integration in this repository. The MCP code in `server/src/plugins/mcp.ts` and `transport.ts` is for external apps (Composio, Arcade, Google Drive, Notion, the built-in Routines server), and the computer tools are never registered there; the bot's provider only has to be an AG-UI agent. **[code]** (`server/src/plugins/` listing; the tool registrations cited above)

### 1.3 Consent and safety

**What the person must approve.** Two separate gates, and only the second is per action. **[docs]**

*Gate 1, the policy engine* (`server/src/computer/policy.ts`, described in `docs/architecture.md:43-79`): CEL expressions over `tool.name`, page URL and host, element role and name, key, command, file path, MCP server and tool, and the initiator. Deny rules first; a missing or empty policy permits nothing; a broken deny rule denies; a broken allow rule does not permit; a policy in `dry-run` decides and records but lets the action through. **[docs]** The shipped default is `deny: []`, `allow: ["true"]`, which permits everything, so the *real* brake is the next gate. **[docs]**

*Gate 2, approvals* (`server/src/approvals/policy.ts`, `docs/approvals.md`): five kinds of action always ask — paying, changing a password or security setting, deleting, sending to other people, sharing, posting — recognised by words in the tool name, the page and the button label. **[docs]** The answers are Allow once / Allow for this conversation / Always allow here / Deny, and a conversation answer is scoped to the same bot, tool, place and account. **[docs]** A credential change is not offered at all: it is handed to the person ("I did it myself"). **[docs]** (`docs/approvals.md:48-51`)

**Takeover and stop.** The `control` state machine (agent-computer/src/control.ts) holds `request`, `owner` and `heldBy`. While a person holds the wheel, every bot action is *refused* rather than queued, with one of three fixed sentences ("A person has control or has been asked to take control. Wait for them to hand it back before acting."). A handback raises `SnapshotRequiredError`, so the bot must re-read the page before acting into whatever the person left. Stop and reset drop the wheel and end any recording. **[code]** (`agent-computer/src/control.ts:29-56`) The audit trail records `computer.help_requested`, `computer.control_taken` and `computer.control_released`. **[docs]** (`docs/architecture.md:156-164`)

**What is refused.** Cloud metadata addresses in every network mode; anything outbound when no network policy has arrived ("until one arrives nothing may leave"); private hosts unless `AGENT_COMPUTER_ALLOW_PRIVATE_HOSTS=true`, which production refuses to start with; private or reserved addresses in the *published preview* path; a sign-in whose page is not on the exact origin asked for, or whose two-step login moves origin; solving a bot check on the bot's behalf (it must ask a person). **[docs]** (`docs/architecture.md:96`, `:431-433`; `agent-computer/src/sign-in.ts:14-19`)

**What is logged.** Every decision is an audit row with the actor, the initiator kind (`person`, `routine`, `handoff`, `memory`, `responsibility`, `deployment`), and a redacted payload; an action held for approval is written when the person answers. Secret requests record "that a secret was requested or supplied and the character count, not the secret value". The Activity list is a *view* over tool results, not a record. **[docs]** (`docs/architecture.md:102-152`, `:166-172`)

### 1.4 How a bot's browser differs from computer use

In OpenBot they are the same object, with three profiles of use:

| | The bot's browser | The person's browser | Computer use |
|---|---|---|---|
| What it is | A persistent Chromium context inside the bot's container | The person's installed Chrome or Edge, started on a DevTools pipe and attached with `connectOverCDP({noDefaults: true})` | Not present as desktop control |
| Isolation | One user-data directory per bot; "a profile is not a container" | One per person; ephemeral unless recorded | — |
| Tools | The 12 `computer_*` page/file/command tools | The Recorder extension (teaching, recording) | — |
| Sign-in | The bot's own cookies persist across restarts; session cookies are saved and replayed | The person is signed in already | — |

**[code]** (`agent-computer/src/profiles.ts:1-34`, `person-browser.ts:1-25`, `native-browser.ts:1-28`)

**Profiles per agent.** One profile per bot, deliberately, because "two Bots sharing a profile share their logins, which makes 'this Bot may reach Salesforce' unenforceable". They use `launchPersistentContext` rather than exported storage state, so IndexedDB and service workers survive. **[code]** (`profiles.ts:4-25`)

**Sign-in handling.** The bot never receives a credential. `computer_request_sign_in` posts a private form to the person; the server posts the username and password to the computer; `sign-in.ts` types them into the page after checking the origin, that a login form exists, and that the login did not move origin. A verification code is asked for only after the site accepts the password and shows a code field. The bot is told "only whether it worked". **[code]** (`sign-in.ts:1-23`, `headless-tools.ts:33-34`, `:271-311`)

### 1.5 Licence, and what Retinue may reuse

MIT, "Copyright (c) 2026 CopilotKit". **[code]** (`LICENSE:1-3`)

- **Code reuse is permitted**, with the copyright notice and permission text retained. Retinue's `docs/legal/third-party-notices.md` would need a line, and the match to Retinue's stack would be poor: TypeScript services on Bun, PostgreSQL, a container runtime, Playwright, and AG-UI.
- **The transferable value is the design, not the files:** the ref-addressed element model, the refusal-not-queue control machine, the private sign-in form, and the "policy lives in the server, the actor holds only a per-actor token" split.
- **One caution.** Playwright itself is Apache-2.0 and its bundled Chromium has its own licences; if Retinue ever ships a browser, that is a second notice and ~120–150 MB of installer. **[unchecked]** (exact byte cost in Retinue's packaging)

---

## Part 2: browser use and server preview in T3 Code

Read at `d2c25950b4d9183ea342d5d5fc14c326b9e0172a`. MIT (`LICENSE:1-3`). Desktop is **Electron**, not Tauri (`README:3`), which matters for what Retinue can copy. **[code]**

### 2.1 How an agent uses a browser

**Four layers, and the seam between them is CDP.**

1. **A headless Chromium in the server.** `apps/server/src/preview/ServerBrowser.ts` (109 KB) imports `playwright-core` types and drives `Page`, `BrowserContext`, `CDPSession`, `Dialog`, `Download` and `FileChooser` directly. **[code]** (`ServerBrowser.ts:66-74`)
2. **The desktop shows its own environment's tabs in an Electron `webContents`**, and the server attaches to *that* tab through a relay. Electron allows one debugger per `webContents`, so the manager holds the attachment and `CdpRelay` shares it; the relay understands `Input.*`, `Page.navigate`, `Runtime.evaluate` and `Runtime.callFunctionOn` as "agent input" and bounds each payload at 64 MiB. **[code]** (`apps/desktop/src/preview/CdpRelay.ts:1-60`, `DesktopBrowserHost.ts:24-44`)
3. **Every other device streams.** `ServerBrowserStream.ts` and `packages/client-runtime/src/preview/serverBrowserStream.ts` carry a screencast to the web and mobile clients. **[code]** (file names and the `PreviewEvent` types they import)
4. **The agent's verbs are 12 MCP tools**, in `apps/server/src/mcp/toolkits/preview/tools.ts` **[code]**:

| Tool | What it does | Annotation |
|---|---|---|
| `preview_status` | owner, pending dialog, URL, title, loading, viewport, tabs, profiles | readonly, idempotent |
| `preview_open` | open a tab; `target:{kind:'environment-port',port}` for a dev server | open-world, not destructive |
| `preview_navigate` | URL or environment port | open-world |
| `preview_snapshot` | page state, elements, **diagnostics**, screenshot size | open-world (writes a file when `save`) |
| `preview_act` | click, type, hover, select, drag, upload, press, scroll | open-world, **destructive** |
| `preview_evaluate` | run JavaScript, 64 KB result | open-world, destructive |
| `preview_wait_for` | wait on locator/selector/text/URL conditions | readonly, idempotent |
| `preview_dialog`, `preview_resize`, `preview_set_appearance` | dialogs, viewport, `prefers-color-scheme` | safe |
| `preview_recording_start` / `_stop` | record the tab to a file, 50 MiB cap | safe |

**How a page is read back.** All three ways at once, in one snapshot: `visibleText`, `interactiveElements` (tag, role, name, selector, x, y, width, height), `accessibilityTree`, `consoleEntries`, `networkEntries`, `actionTimeline`, and a PNG (with an optional uncapped `fullScreenshot`). Elements are addressed as `locator='aria-ref=<ref>'` and refs die on navigation, a new snapshot, or a handoff. **[code]** (`packages/contracts/src/previewAutomation.ts:534-560`; the tool text at `tools.ts:140`, `:163`)

**A deliberate nudge against gratuitous verification.** `preview_snapshot`'s description says: "Use it only when the current request asks for browser work… Never use it unasked to verify your own edit, or to repeat a snapshot from an earlier turn." **[code]** (`tools.ts:140`) That is a prompt-level cost control Retinue would need too.

### 2.2 How a preview of a running dev server works

**Starting and tracking.** T3 Code does not start the dev server; the *person* or the *agent* starts it in a terminal, and the preview discovers it. `PortDiscovery` reads listeners, probes them, attributes them, and publishes a list with an optional stop that kills the process group. **[code]** (`apps/server/src/preview/PortScanner.ts:30-64`)

**Finding the port.** Per platform: `netstat.exe` parsed on Windows (`:822-841`), a bounded `/proc` walk on Linux (`:843-871`), `lsof -F pcn` on macOS with a `/proc` fallback (`:846-877`). Listeners are then **probed** and kept only if the HTTP answer is 2xx/3xx with an HTML content type (`:644-663`), with a probe cache so idle polling is cheap. `T3CODE_PORT_DISCOVERY=0` turns discovery off. **[code]** (`:72`)

**Attribution.** Agent processes launch with `T3CODE_THREAD_ID` set to their thread, "so a server stays attributed after its launching shell exits or the agent settles"; terminal processes are registered by pid. On Windows agent attribution is disabled (`hostPlatform !== "win32"` at `:813`), so Windows gets terminal-owned listeners only. **[code]** (`:66-70`, `:800-821`)

**Showing it.** A right-panel tab (`PreviewPanelShell` has `inline`, `sheet`, `sidebar`, `embedded` modes) or a floating mini player over the chat; an agent-opened tab floats by default and a setting moves it to the panel. Links from the thread open there. The person can annotate a page (`PickPreload.ts`, `Annotation.css`) and the annotation becomes composer context. **[code]** (`apps/web/src/components/preview/PreviewPanelShell.tsx:21-30`, `PreviewView.tsx`, `apps/desktop/src/preview/PickPreload.ts`) **[docs]** (`docs/user/default-browser.md:4-12`)

**Another machine.** "Browser tabs belong to the environment, so you and your agents see the same tabs from any device." The server holds the browser, so `localhost` in a tab means the *host's* localhost; other devices stream the tab. For a desktop-managed SSH environment, "T3 Code starts or reuses a server there and opens the port forward for you" — a real `ssh` local forward, waited on until the local port listens before the app is told it is ready. **[docs]** (`docs/user/remote-access.md:239-245`, `:208-213`) **[code]** (`packages/ssh/src/tunnel.ts:1-60` and its readiness tests)

**How person and agent share one view.** A `SessionControl` lease: one owner string, an epoch that changes whenever control moves, a queue that drains running work before the new owner acts, and a distinct path for "human input" (which is applied through the same page). A person must "take control" before typing into an agent's tab and "release" it back. **[code]** (`apps/server/src/preview/SessionControl.ts:25-50`) **[docs]** (`docs/user/remote-access.md:252-260`)

### 2.3 Site permissions, sign-in and what is refused

- **A fixed permission allow-list per preview session**, not a prompt storm: clipboard write, geolocation, fullscreen, pointer lock, loopback network. Clipboard *reads* ask once per origin and the answer lasts the session. Local fonts are denied (font fingerprinting); notifications are denied (a page could impersonate the app). **[code]** (`apps/desktop/src/preview/BrowserSession.ts:21-32`, `:292-345`)
- **Sign-in is imported, not typed by the agent.** `BrowserImport/` reads cookies out of the person's installed browsers (Chromium key decryption, Firefox, Safari with a permission check) into the preview profile, and `preview_status` lists the profiles so `preview_open` can pick a signed-in one. **[code]** (`apps/desktop/src/preview/BrowserImport/`, `tools.ts:55`)
- **Passkeys are bridged**, so a WebAuthn ceremony in the preview reaches the OS (Touch ID on macOS through a keychain group). **[code]** (`apps/desktop/src/preview/Passkeys.ts:1-40`, `PasskeyBackendMac.ts`)
- **Refused:** external protocols are filtered by a deny-list (`about:`, `blob:`, `file:`, `javascript:`, `view-source:` and others, `BrowserSession.ts:34-45`); a popup such as a sign-in window opens as its own preview tab rather than an OS window (`docs/user/remote-access.md:258-260`); the *published HTML* path (a different feature) blocks names resolving to private or reserved addresses, and its browser is a separate 120 MB download. **[code]** (`BrowserSession.ts:34-45`) **[docs]** (`docs/user/html-renders.md:9-11`)

### 2.4 Console errors and network failures

Captured per tab and handed to the agent inside the snapshot, bounded, with the page's own error text intact. **[code]**

- `page.on("console")` → `{level, text (2 000 chars), timestamp}` (`ServerBrowser.ts:1006-1010`).
- `page.on("response")` → `{url, method, status (may be null), failed:false, timestamp}` (`:997-1005`).
- `page.on("requestfailed")` → `{url, method, status:null, failed:true, errorText, timestamp}`, and a *main-frame* failure additionally raises `LoadFailed` with a parsed Chrome net error code and description, so the panel can say "connection refused" rather than "blank page" (`:957-996`).
- Both arrays are `pushBounded`, and the snapshot carries them all (`packages/contracts/src/previewAutomation.ts:506-543`).

### 2.5 Other open projects with a better preview

**Nothing verified this round.** The earlier reports in `docs/research/` cover Monocode, Orca, Sculptor and Superset for queues, review and delegation only; none of them records how those products preview a dev server, and I did not re-read any of their code. Treat "someone else does it better" as **unchecked**. What I can say from this reading is that T3 Code's combination — environment-owned tabs, a shared control lease, an MCP tool surface with console and network in the snapshot, and a real port forward for remote environments — is strong enough that copying its shape is cheaper than shopping further.

---

## Part 3: AG-UI

Read from `https://docs.ag-ui.com` on 11 November 2026: introduction, architecture, events, state, tools, generative UI specs, and the clients quickstart. The repository is `ag-ui-protocol/ag-ui`, MIT, 17,116 stars, last push 2026-11-11. **[public]**

### 3.1 What AG-UI is, in plain words

**Who speaks it.** An *agent* (a backend that runs a model loop) on one side, and a *front end* on the other. The front end can be a web app, a terminal, a phone app or a Slack bridge — "AG-UI describes an event stream rather than a rendering target". **[docs]** (introduction; clients quickstart)

**The transport.** Not mandated: "SSE, webhooks, WebSockets, and more". The reference client `HttpAgent` POSTs a `RunAgentInput` body and reads back a stream of `BaseEvent`s, over SSE or a binary protocol. Message shape is `run(input: RunAgentInput) -> Observable<BaseEvent>`. **[docs]** (architecture)

**The event types** (the docs' own taxonomy) **[docs]** (events):

| Category | Events | What a client does |
|---|---|---|
| Lifecycle | `RunStarted`, `StepStarted`, `StepFinished`, `RunFinished`, `RunError` | boundary of one run; `RunFinished.outcome` may be `{type:"interrupt"}` |
| Text | `TextMessageStart/Content/End` (+ `Chunk`) | append deltas by `messageId` |
| Tool calls | `ToolCallStart/Args/End`, `ToolCallResult` (+ `Chunk`) | accumulate JSON deltas, execute, return a `tool` message |
| State | `StateSnapshot`, `StateDelta`, `MessagesSnapshot` | replace, or apply JSON Patch (RFC 6902) |
| Activity | `ActivitySnapshot`, `ActivityDelta` | a structured object between messages (a plan, a search) |
| Reasoning | `ReasoningStart/…/End`, `ReasoningEncryptedValue` | visible summaries; opaque encrypted state carried back |
| Subagents | `SubagentStarted/Finished/Error`, plus `subagentRunId` on most events | attribute output to a child run |
| Special | `Raw`, `Custom` | pass-through, and app-defined events |

**How state travels.** One run-scoped JSON document; a snapshot replaces it, and deltas are RFC 6902 patches applied in order. **[docs]** (state)

**How tool calls travel.** Two kinds. *Backend* tools stay in the agent's own framework. *Client* tools are passed in `RunAgentInput.tools` (JSON Schema), and the agent's `ToolCallStart/Args/End` is answered with a `tool` role message carrying `toolCallId`, and an `error` field when the call failed. **[docs]** (tools) That is precisely how OpenBot hands `computer_click` to its UI.

**Generative UI.** AG-UI "is not a generative UI specification" — it is the pipe. It can carry A2UI, MCP-UI, Open-JSON-UI or a private convention. **[docs]** (generative-ui-specs)

### 3.2 How it relates to what Retinue already has

| Retinue thing | What it is | Overlap with AG-UI | Verdict |
|---|---|---|---|
| **ACP** (OpenCode, and the `provider-acp-registry` rows in T3) | agent ↔ *editor* protocol: sessions, prompts, permission requests, tool updates | both are "drive an agent and observe its work", but ACP is a control protocol for a process that acts, AG-UI is a presentation protocol for a run | Different layer. ACP already gives Retinue `session/prompt`, `session/update` and permission requests, which it maps to its own events |
| **MCP** (`crates/retinue-daemon/src/mcp_bridge.rs:1-30`, `external.rs`) | agent ↔ *tools*, over stdio JSON-RPC to the daemon | no overlap | Already Retinue's tool plane for outside agents, at three tiers |
| **Daemon events over WebSocket** (`session.changed`, `turn.*`, `permission.requested`, `external.changed`, …) | Retinue's own event bus, one per session, redacted per tier before it leaves the host | conceptual twin of AG-UI's stream: text deltas, tool rows, approvals, state | The daemon's model is *narrower in vocabulary, wider in meaning*: approvals, queues, worktrees, usage and machines have no AG-UI event |
| **`openui` blocks** (`crates/retinue-daemon/src/openui_guide.md`, `apps/desktop/src/ui/openui/OpenUiBlock.tsx`, `library.tsx`) | a fenced block a bot writes; the UI parses it and draws Retinue's own components; buttons raise `retinue:openui-reply` into the composer, never send | this is Retinue's generative UI, and it is *declarative and validated*, closest to A2UI | It solves the part of AG-UI's story Retinue actually wanted: a bot answering with a view it cannot script |

**Where AG-UI would fit if anywhere:** it is the one layer Retinue does *not* have — a presentation contract for a run — and Retinue currently has its own. Nothing in AG-UI's event list is missing from Retinue's per-turn data; what is missing is anybody else speaking either protocol.

### 3.3 An honest verdict

**Option A — ignore it.** Cost: nothing. Risk: if "bring your own agent" becomes a product requirement, the seam has to be built later, and a competitor's integration list (LangGraph, CrewAI, Mastra, Pydantic AI, Google ADK, Microsoft Agent Framework, Claude Agent SDK, AWS Strands) becomes their hiring pool rather than ours.

**Option B — speak it outward** (an AG-UI endpoint inside the daemon that exposes a Retinue bot to any AG-UI client). Cost: a new loopback listener or an SSE route on the existing one, a mapping from daemon events to ~20 AG-UI event types, thread/run identity, `interrupts` for approvals, and a client-tool bridge for `openui`. Also a second authentication story next to the capability token, a new row in `tiers.json` for every method, and a threat-model section, because *any AG-UI client* now drives bots on the host — the exact reach `AGENTS.md` restricts. Benefit: a third-party front end, a Slack bridge, or a "use Retinue from someone else's app" story. Retinue already has a second front end (the remote PWA, tier-gated), so the marginal benefit is small.

**Option C — accept it inward** (an AG-UI agent hired as an employee). Cost: a new adapter beside `retinue-adapter-{claude,codex,acp,opencode}` that runs `HttpAgent.runAgent` and turns AG-UI events into `AgentEvent`s. That seam exists and is well worn. The problems are elsewhere: **a remote AG-UI agent brings its own tools**, so the daemon's permission broker never sees them; it needs its own browser; and it has no worktree or terminal discipline. OpenBot hit this and wrote it down: a remote bot is "told to ask you itself before one of its own tools does something that cannot be undone, because its own tools don't go through this policy" (`docs/approvals.md:74-77`, **[docs]**). Benefit: real — the preset catalogue is personas on three CLIs, and this would let a builder's LangGraph or Mastra agent be hired.

**I would pick A, for now**, and write down C as the single revisit. Reasons, in order:

1. Neither direction is a problem Retinue has today. The daemon's per-turn event model already carries everything the UI needs, and the remote PWA already covers "a second front end".
2. B is the one that touches the rules that keep Retinue safe (new reach, new auth, new tier rows). It buys an audience Retinue has not asked for.
3. C is architecturally cheap but *weakens the permission model* unless the accepted agent is treated as a bot with no host tools at all — and that is a product decision about what "employee" means, not an integration detail.
4. The trigger to revisit is concrete: the moment the owner wants a bot that is not one of the three CLIs, or a second front end that is not the phone. At that point C first, in the "no host tools, ask the person itself" posture OpenBot documents.

**If it is ever done, the mapping is small and known.** Approvals are the interesting part, and AG-UI's interrupt model (`RunFinished.outcome:{type:"interrupt",interrupts:[…]}` then a new run with `resume[]`) is a real fit for `permission.requested` / `permission.reply`. Text deltas map to `turn.delta`; tool rows map to `ToolCallStart/Args/End` plus `ToolCallResult`; `openui` would travel as a `Custom` event with the fence text, which the AG-UI client renders or ignores. **[docs]** (events; interrupts)

---

## Part 4: what Retinue should build

### 4.0 What Retinue has today

- **A terminal dock** at the bottom of the main window, per conversation, resizable, up to 6 sessions, PowerShell / PowerShell 7 / cmd only, started inside the conversation's project folder, sessions in memory only (`apps/desktop/src-tauri/src/pty.rs:1-35`; mounted at `apps/desktop/src/ui/App.tsx:3226`; toggle at `:3086`). **[code]**
- **Media preview**: images, PDFs (pdf.js, lazily), audio, video, text, served by the daemon only from inside the conversation's folder (`ui/MediaPreview.tsx:1-30`). **[code]**
- **One non-scripting frame**: bot-authored HTML or SVG is drawn in an `iframe` with `sandbox=""` and a `default-src 'none'` policy (`ui/VisualReply.tsx:1-26`). This is the closest thing to a preview surface and it deliberately cannot run anything. **[code]**
- **The MCP bridge**: two stdio servers forwarding to the daemon — the per-session team bridge and the outside-session Retinue MCP (`crates/retinue-daemon/src/mcp_bridge.rs:1-30`). The outside MCP's catalogue is exactly 20 tools at three tiers, `viewer` / `chat` / `operator` (`external.rs:87-133`, `:388-430`). **[code]**
- **Permissions**: four modes (`ask`, `auto`, `accept_edits`, `bypass`), a classifier that assigns `EDIT` / `READ` / other categories, a "remember this decision" rule store, and a decision recorded per permission (`crates/retinue-daemon/src/main.rs:5196-5203`). **[code]**
- **Remote tiers**: `apps/desktop/src/remote/tiers.json`, one row per method with `class`, `minTier` and a `why`; anything unlisted fails the parity test. `session.repository` is `never` because diff text cannot be redacted safely. **[code]**
- **The threat model**: "Remote access to prompts is remote code execution on the computer, bounded by approvals" (`docs/remote/THREAT_MODEL.md:16-21`); asset list at `:23-35`; abuse rows T1–T15 at `:58-76`; defaults at `:78-86` (Remote off, approvals from remote off, never Funnel, never a non-loopback bind). **[code/docs]**
- **The window's CSP** is `default-src 'self'` with no `frame-src`, and `connect-src` limited to `'self'`, `ipc:`, `http://ipc.localhost` and `127.0.0.1:1420` (`apps/desktop/src-tauri/tauri.conf.json`). Tauri is built with `features = ["tray-icon"]` only (`Cargo.toml`), so **multi-webview is not enabled**. Window capabilities are `core:event:allow-listen`, `core:event:allow-unlisten`, `core:window:allow-start-dragging` (`capabilities/default.json`). **[code]**

### 4.1 A browser preview pane for servers a bot starts

**Where it should sit.** The dock, not the rail and not a page. Reasons: a server belongs to one conversation and lives as long as that conversation's process; the dock is already the per-conversation "things my bot started" surface, already resizable, already survives conversation switches; and a new dock tab costs no new navigation. Concretely: a fourth element beside the terminal tabs — a **Preview** tab with a port chip and an **Open** button — plus a row in `SessionRail` ("Server on :5173 — Preview") when the conversation has a live listener.

**How the port is discovered.** In the daemon, in Rust, with attribution — copy T3 Code's *shape*, not its code:

1. The daemon already spawns every bot CLI (`retinue-process`) and holds its handle, so it can record a bot's pid and its descendants; the shell holds its PTY children (`pty.rs` keeps `Child` handles), but the *daemon* cannot see them today. Two options: (a) have `pty.rs` report the child pid to the UI and have the UI tell the daemon; (b) move the port list's attribution to "any listener under any of my child trees", which works for bot-started servers and leaves terminal-started ones unattributed at first.
2. Read listeners: `netstat -ano` parsed in Rust is enough for Windows and needs no new dependency; a `GetExtendedTcpTable` call would be cleaner but is more native code.
3. Probe each loopback listener for an HTML answer (2xx/3xx, `text/html`) so a database port is not offered as a page. A 400 ms timeout, a few seconds' cache.
4. Publish `preview.ports {sessionId}` → `[{port, pid, process, title, owner}]` and one event, `preview.changed {sessionId}`, throttled. Never return the URL of a non-loopback listener.

**What Tauri and WebView2 allow on Windows.** Three candidates, with the constraint that is easy to miss: the main window's CSP has no `frame-src`, so `default-src 'self'` blocks an `iframe` to `http://127.0.0.1:5173`, and `connect-src` does not include other localhost ports. **[code]**

| Option | Works today? | Cost | Verdict |
|---|---|---|---|
| **`iframe` inside the main window** | No. Blocked by CSP; would need `frame-src http://127.0.0.1:*`, which widens the app's own policy and lets any local page render inside Retinue's tree | small but unsafe | No |
| **A second Tauri window with `WebviewUrl::External("http://127.0.0.1:<port>")`** | Yes. `make_companion` / `make_dictation` already prove the window pattern (`src-tauri/src/lib.rs:3012-3135`); an external URL is not subject to the app's CSP and gets no Tauri IPC unless a capability names it | medium: window placement and persistence, a second surface in a single-window product, focus and z-order rules, and a "close the window when the conversation changes" rule | **Recommended for v1** |
| **A child webview inside the main window** | Not without `tauri` `unstable` feature; not enabled in `Cargo.toml` | large: new feature flag, new CSP and capability reasoning, risk to drag regions and the Haze backdrop | Not now; revisit if the preview proves central |

Two consequences worth stating: the preview window is **one browser profile, shared with the app's own WebView2 environment**, so cookies a previewed site sets live in the same WebView2 user-data folder as Retinue's own storage — a real concern for a preview pointed at a site rather than a local dev server. **[unchecked]** (whether Tauri 2 lets a single window use a different WebView2 data folder; needs a probe). And a preview window **cannot be shown on a phone**: the remote PWA would have to stream it, which is the T3 Code layering, out of scope for v1. Every new RPC therefore goes into `tiers.json` as `never` until a streaming design exists.

**The smallest useful version.** One dock tab, one list, one window:

1. Daemon: `preview.ports` + `preview.changed`, loopback only, HTML probe, attribution best effort.
2. UI: a **Preview** tab in the dock that lists discovered ports with the process name; a click opens a second window at that URL; a **Refresh** and a **Copy URL**.
3. Nothing else: no agent access, no screenshots, no evaluate, no recording, no remote.

That is a day or two of daemon work and a day of UI, and it answers the question the owner actually asked ("may I see what my bot started"). **[judgement]**

### 4.2 Browser use for bots

| Option | What the bot gets | Security cost | Judgement |
|---|---|---|---|
| **Ship an MCP browser server** (Playwright or a small custom one) that the adapters mount per session | 10–14 verbs over an isolated Chromium with its own profile | A new open-world tool in the agent's hands: it can act as whoever is signed in to that profile; every action needs a permission category or it lands in `auto`/`bypass` silently; a second Chromium (~120–150 MB) or a download on first use; a new process for the daemon to supervise; a new place where a credential could be typed | The only honest option. **Large**, and only after the preview exists |
| **Drive the preview pane over CDP** | The agent uses the tab the person is looking at | The agent and the person share one session; without a control lease it will type over the person; WebView2's CDP is Edge's, not Chromium's, so the Playwright surface differs; pointing the agent at a WebView2 holding the app's own cookies is a bridge into Retinue's storage | Tempting (zero new download) but the control-lease work is the expensive half, and the profile-sharing problem above makes it the *least* safe. No |
| **Rely on the provider's own browser tools** | Whatever Claude Code, Codex or OpenCode ships | none | Today: nothing. None of the three exposes a browser to the daemon; what they do expose is web search/fetch through their own accounts. Keep that, and say so in copy |

**Recommendation.** Do not start with 4.2. The preview (4.1) covers the ordinary case ("look at what my bot built") without handing any agent a browser. If the owner wants bots that *use* a site — fill a form, read a dashboard, check a deploy — build an MCP browser server with these rules, which are Retinue's rules applied rather than new ones:

- A **separate browser profile** with no imported cookies; the profile is Retinue's, not the person's. Sign-in only through the existing secret request (`request_secret`, `crates/retinue-daemon/src/secrets.rs`), typed by the person, never seen by the bot — the OpenBot `sign_in.ts` pattern, and it keeps `AGENTS.md`'s "no copied provider authentication" rule intact.
- A **new permission category** (`BROWSER`) classified in the same place as `EDIT`/`READ` (`main.rs:5196-5203`), denied under `ask` unless approved once, and **never** auto-allowed by `accept_edits`. Payments, sends, deletes and posts follow the existing "Always allow" rule semantics.
- **Refusals that do not need a policy engine:** no `file:`, `javascript:`, `data:` navigation; no non-loopback private ranges; no cloud metadata; a hard cap on page text and element count.
- **New reach means new rows:** the tools are the bot's own MCP tools (in-process), so no `tiers.json` row is needed for phones — but the *events* (`preview.*`, and any browser step row) do need rows, and `never` is the right first answer.

### 4.3 Computer use

**Should Retinue offer it at all?** Not in the sense the industry means (screenshots of the desktop plus mouse and keyboard). The narrowest safe form, if the owner insists, is **"the bot drives one browser it owns"**, which is 4.2 — not computer use. The reasons this is not timidity:

1. **A screenshot is unredactable content leaving the machine.** Retinue's rule is that message content, paths, credentials and tool output do not leave the host except as the prompt the owner is sending (`THREAT_MODEL.md:88-93`; T12 "Remote adds no new tool or egress"). A desktop screenshot of the owner's screen goes to Anthropic/OpenAI/OpenCode's provider as prompt content, could contain a password manager, private mail or another person's data, and there is no classifier that can redact it before the model sees it. The only safe version is "the person sees the exact frame before it is sent", which is a consent flow per frame.
2. **No per-action classification.** Approvals work because the daemon classifies a *tool call* (`EDIT`, `READ`, a command). "Click at 812,431" cannot be classified; a desktop driver would be an open-world tool in the bot's hands with no brake except `bypass` being off.
3. **The existing mode model does not cover it.** `accept_edits` means "file edits and reads inside the project". There is no category for "drive the owner's desktop", and adding one is a product decision, not a setting.

**If it is built anyway, the narrowest form I would defend:**

- **One window, chosen by the person, shared explicitly** ("Share this window with <bot>"), with a persistent border and a stop hotkey; never the whole desktop, never a second monitor, never a hidden window.
- **Windows only, in the shell**: a native helper captures *that window's* client area (Windows Graphics Capture or `PrintWindow`), never the full screen, and clips to the window rectangle at capture time.
- **Every frame is reviewed before it leaves**: the frame is shown in the conversation with "Send this frame / Retake", or the bot works against a local-only model. Retinue already has the pattern for a deliberate send (`secret_requests`, one-use grants).
- **Input is confined** to that window's rectangle, and refused while the window is not foreground.
- **A new permission category `SCREEN`**, denied by default, never granted by a conversation-level rule, refused outright in unattended runs (`is_unattended`), and refused for a teammate's reply-woken turn (`depth_of > 0`).
- **`tiers.json`: `never` for every new method** until a streaming design exists, and no event may carry a frame.

**What the owner would have to decide.** (a) Whether any image of the screen may go to a provider at all — this is the load-bearing question, and "no" kills the feature. (b) If yes, whether per-frame consent or a session-long consent with a stop hotkey. (c) Whether a bot may drive another application on the owner's machine while the owner is using it, which is a shared-machine question the threat model does not yet have a row for. (d) Whether `AGENTS.md` gets a new amendment, because this adds both a capability and an egress path.

### 4.4 Parts, effort, risks

| Build | Daemon | UI | Effort | Risks |
|---|---|---|---|---|
| **4.1 Server preview** | new `preview.rs` (`preview.ports`, `preview.changed`, loopback-only, HTML probe, attribution); one `tiers.json` row pair (`never`); `docs/ipc-contract.md` (protected path); a `doctor.run` check is optional | `TerminalDock.tsx` gets a tab; a new `ui/PreviewPane.tsx`; `tauri.ts` + `preview/fixtureBridge.ts` + `remote/remoteTauri.ts` (a refusing stub) for the window call; one native command (`open_preview_window`); `SessionRail` row; design-language pass on the new tab and window chrome | **S–M** (daemon S, window M) | The second window's profile sharing (4.1); a preview that opens a *non*-dev-server port as if it were a page; the CSP decision being quietly widened later; port churn in a busy project (filter by attribution, not by "everything listening") |
| **4.2 Browser MCP for bots** | a supervised browser process; a tool bridge in `mcp_bridge.rs`/the session's tool set; the `BROWSER` permission category and classifier arm; secret entry through `secrets.rs`; a settings switch under `experimental.*` via `experiments::on`; `tiers.json` rows for events (`never`) | the dock needs a live "the bot is driving a page" row (a step line plus a thumbnail would be the honest minimum); Settings › Approvals copy for the new category; `settingsIndex.ts` entry | **L** | The browser profile holds real logins (a stolen profile is account takeover); the agent typing over the person; a big installer; prompt injection from a page driving a tool that then acts elsewhere; `accept_edits` accidentally covering `BROWSER` |
| **4.3 Computer use** | a native capture and input helper in `src-tauri`; a `SCREEN` category; per-frame consent plumbing; unattended refusals | a consent card, a shared-window border and indicator, a stop hotkey, a frame viewer | **L+**, plus a policy decision and an `AGENTS.md` amendment | Frames leaving the machine; no per-action classification; the owner's screen content in a provider's logs; a new egress path that the threat model's T12 says exists nowhere today |

**Order I would run them:** 4.1, then *stop and live with it for a week*, then decide 4.2 on evidence (do bots actually need to act on pages, or only to build them?), and treat 4.3 as closed unless the owner answers the screenshot question with "yes" and accepts a new amendment.

**One thing to fix before 4.2 makes any sense:** the dock should show what the bot is doing to a page — a step line ("Browsing: localhost:5173/login") drawn from the same tool rows that already exist, because `doingNow.ts` already turns tool records into a status line. That is small and it is the difference between a bot with a browser and a bot with a browser the owner can watch.

---

## Sources

**Repositories read in code** (shallow clones, removed after reading):

| Project | URL | Commit | Date | Licence |
|---|---|---|---|---|
| OpenBot | https://github.com/CopilotKit/OpenBot | `b01ac8c7cfcca170b097c07cb4c2f331934fa0c2` | 2026-11-11 | MIT (`LICENSE:1-3`) |
| T3 Code | https://github.com/pingdotgg/t3code | `d2c25950b4d9183ea342d5d5fc14c326b9e0172a` | 2026-11-11 | MIT (`LICENSE:1-3`) |
| OpenBot (earlier round, not re-read) | https://github.com/nightly-labs/openbot | `712e797303c5d41ee256c8360645718a0beb13b2` | 2026-11-10 | PolyForm Noncommercial 1.0.0 |
| AG-UI | https://github.com/ag-ui-protocol/ag-ui | not cloned; documentation read | 2026-11-11 | MIT |

**Files cited, OpenBot `b01ac8c`:** `README.md:1-30`, `LICENSE`, `CHANGELOG.md`, `docs/architecture.md` (§Browser action governance, §Computers, §Human control and secrets, §Watching a Bot work, §Security boundaries), `docs/approvals.md`, `docs/personal-edition.md`, `docs/desktop-install.md`, `agent-computer/package.json`, `agent-computer/Dockerfile`, `agent-computer/src/index.ts:105-130,598-620,1474-1493,1624-1755,2054-2210`, `agent-computer/src/aria-snapshot.ts:1-60`, `agent-computer/src/virtual-display.ts:84-99`, `agent-computer/src/profiles.ts:1-34`, `agent-computer/src/person-browser.ts:1-25`, `agent-computer/src/native-browser.ts:1-50`, `agent-computer/src/sign-in.ts:1-70`, `agent-computer/src/control.ts:29-56`, `agent-computer/src/authorisation.ts`, `agent-computer/src/screencast.ts:15-45`, `server/src/computer/headless-tools.ts:33-338`, `server/src/computer/gateway.ts`, `app/src/lib/copilot/computer-tools.tsx:1-120,300-500`, `app/src/lib/boundaries/rules.ts:25-64`, `shared/tool-name.ts:320-681`, `desktop/src-tauri/Cargo.toml`.

**Files cited, T3 Code `d2c2595`:** `README.md:1-3`, `LICENSE`, `apps/server/src/mcp/toolkits/preview/tools.ts:1-254`, `apps/server/src/preview/PortScanner.ts:30-70,640-694,795-877`, `apps/server/src/preview/ServerBrowser.ts:66-80,301-302,930-1010`, `apps/server/src/preview/SessionControl.ts:1-50`, `apps/desktop/src/preview/CdpRelay.ts:1-60`, `apps/desktop/src/preview/DesktopBrowserHost.ts:1-55`, `apps/desktop/src/preview/BrowserSession.ts:1-45,280-350`, `apps/desktop/src/preview/Passkeys.ts:1-40`, `apps/desktop/src/preview/BrowserImport/`, `apps/web/src/components/preview/PreviewPanelShell.tsx:21-30`, `packages/contracts/src/previewAutomation.ts:495-560`, `packages/ssh/src/tunnel.ts:1-60`, `packages/shared/src/preview.ts:1-60`, `docs/user/remote-access.md:195-278`, `docs/user/devices.md:55-127`, `docs/user/default-browser.md`, `docs/user/html-renders.md`, `docs/internals/composer-context-references.md:60-102`.

**Documentation read on the web (11 November 2026):**

- https://docs.ag-ui.com/introduction — the three-protocol table, building blocks, integration list, OpenBot named as a first-party client.
- https://docs.ag-ui.com/concepts/architecture — event categories, transport agnosticism, `HttpAgent`, SSE and binary transport.
- https://docs.ag-ui.com/concepts/events — the event catalogue, metadata, interrupt-aware `RunFinished`, subagents, deprecated thinking events, draft meta events.
- https://docs.ag-ui.com/concepts/state — snapshots, JSON Patch deltas, `MessagesSnapshot`.
- https://docs.ag-ui.com/concepts/tools — frontend tools in `RunAgentInput.tools`, the `tool` message, the `error` field.
- https://docs.ag-ui.com/concepts/generative-ui-specs — AG-UI is not a generative UI specification.
- https://docs.ag-ui.com/quickstart/clients — a client can be a terminal; streaming event handlers.

**Repository metadata (GitHub API, 11 November 2026):** `CopilotKit/OpenBot` (MIT, 10,962 stars, created 2026-08-17, pushed 2026-11-11), `nightly-labs/openbot` (NOASSERTION, 6,993 stars, pushed 2026-11-11), `ag-ui-protocol/ag-ui` (MIT, 17,116 stars, pushed 2026-11-11).

**Retinue files cited:** `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/capabilities/default.json`, `apps/desktop/src-tauri/src/lib.rs:3012-3135`, `apps/desktop/src-tauri/src/pty.rs:1-35`, `apps/desktop/src/ui/App.tsx:3086,3226`, `apps/desktop/src/ui/TerminalDock.tsx`, `apps/desktop/src/ui/terminalHub.ts:1-25`, `apps/desktop/src/ui/MediaPreview.tsx:1-30`, `apps/desktop/src/ui/VisualReply.tsx:1-26`, `apps/desktop/src/ui/openui/`, `apps/desktop/src/remote/tiers.json`, `crates/retinue-daemon/src/mcp_bridge.rs:1-30`, `crates/retinue-daemon/src/external.rs:32-133,388-430`, `crates/retinue-daemon/src/main.rs:5196-5203`, `crates/retinue-daemon/src/experiments.rs:8-14`, `crates/retinue-daemon/src/openui_guide.md`, `docs/remote/THREAT_MODEL.md:16-95`, `docs/research/competitor-adaptation-2026-11.md`, `.claude/skills/retinue-development/references/codebase-map.md`.

---

## What I could not verify

1. **Whether the two OpenBot projects share people.** Different code, licence and layout, no cross-reference in either repository that I found; lineage is unknown. **[unchecked]**
2. **Exact Chromium/Playwright installer cost in Retinue's packaging.** T3 Code says "about 120 MB" for its headless Chrome and OpenBot's image drops a 340 MB shell build; neither number is Retinue's. **[unchecked]**
3. **Whether Tauri 2 can give one window a different WebView2 user-data folder.** The preview-window design depends on it: if it cannot, a preview pointed at a website shares cookies with Retinue's own WebView2 storage. Needs a probe before 4.1 is specified. **[unchecked]**
4. **Whether an `iframe` to a localhost port can be made to work under a *narrowed* CSP** (an explicit `frame-src` for `http://127.0.0.1:*`) without practical problems in WebView2 — I rejected it on policy grounds without testing it. **[unchecked]**
5. **OpenBot on Windows end to end.** I read that the desktop app supports Windows x64 through WSL 2 and that the person's browser is native, but I ran none of it. **[unchecked]**
6. **OpenBot's actual default posture for approvals** is described in its docs as "opens these by default; closing them is the organization's call" (`docs/approvals.md:53-55`), which contradicts the earlier round's reading of nightly-labs as "full access, no confirmations". Two different products, so both may be true. **[docs]**
7. **T3 Code's preview against a live dev server.** All claims are from source and docs; nothing was run. **[unchecked]**
8. **How Monocode, Orca, Sculptor or Superset preview a dev server.** Not read this round; the earlier reports do not cover it. **[unchecked]**
9. **AG-UI's wire-level details** (the exact `RunAgentInput` fields, SSE framing, binary transport encoding) — read as documentation and examples, not from the specification source. **[docs]**
10. **Whether any adapter could accept an AG-UI agent without a permission-model change.** My reading of `AgentEvent` and the permission handler says no, because a remote agent's tools never reach `main.rs`; I did not write a prototype. **[unchecked]**

---

## Proposed skill update (not applied, because this run was research-only)

For the Director to apply, if wanted, to `.claude/skills/retinue-development/references/codebase-map.md` under "Docs" → Research:

> Research (2026-11-11): `docs/research/browser-computer-use-and-ag-ui-2026-11.md` — OpenBot (CopilotKit's, MIT, not nightly-labs'), T3 Code and AG-UI read in code and docs, with a recommendation for a server preview (dock tab + a second Tauri window because the main CSP has no `frame-src`), gated browser use, no desktop computer use, and AG-UI deferred. Its claim markers (`[code]` / `[docs]` / `[public]` / `[unchecked]`) show which parts were verified.
