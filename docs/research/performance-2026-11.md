# Making Retinue faster and snappier without losing its visual life

Date: 21 November 2026. Research only: no product code changed, no commit made, the installed app was not started and the owner's database was not opened.

Evidence key:

- **[M]** measured in this session, on this machine, with the command or script named.
- **[V]** verified by reading the repository source in this session, with `path:line`.
- **[X]** a measured number published by another project, cited to its page or changelog.
- **[U]** unverified: an inference, a claim I could not reproduce, or something this machine cannot measure.

## 1. Verdict

Retinue does not have a slow backend any more. It has a **render-and-event problem**: one streamed token, and one typed character, re-render the whole application, re-parse every message's Markdown, rebuild the transcript's data structures and then touch the database twice. The cost is proportional to the length of the open conversation, which is exactly why a long chat and a busy turn feel heavy at once.

The three comparable apps solved it in the same two places:

1. **Coalesce streamed deltas before they reach the UI** (Pandacode: 33 ms / 8 KiB buffer in Rust; T3 Code: two 50 ms windows). [X]
2. **Make a transcript update cost the visible rows, not the whole conversation** (Pandacode loads a 100-message page and keeps 100-message pages; T3 Code's timeline is a virtualised list, `LegendList`). [X]

Retinue does neither. It also has no `React.memo` anywhere, and its only render budget currently fails on this machine (median 39 s against a 30 s budget, rising on each run). [M][V]

Recommended shape of the work: a backend lane (Ada) for delta coalescing and probe caching, and the Director for the React split, memoisation, windowing, composer and build splitting, gated by a new browser-based benchmark. Keep Haze and Stone, keep every motion, and make motion cheaper rather than smaller: only the newest row animates, off-screen rows stop laying out, and the frosted sheets stand down only while the frame budget is blown.

## 2. What I measured, and how

| Measure | Result | Method |
| --- | --- | --- |
| Markdown parse of a 1,000-message transcript | **33.2 ms** median (27.4 to 54.3) for 590 KB of text | [M] `node bench-md.mjs` bundling `parseMarkdownBlocks` + `tokenizeInlineMarkdown` out of `ui/SafeMarkdown.tsx` with esbuild, 7 runs |
| One 590-byte reply parsed | 0.02 ms median (31 runs) | [M] same script |
| Existing render gate (`npm run perf:render`, 1,000 messages, happy-dom) | **20,112 / 39,145 / 75,356 ms**, median 39,145 ms against a 30,000 ms budget: **fails**; it rises on each of the three runs | [M] `npm run perf:render` in this worktree |
| Entry JavaScript | **2,361.25 kB** (699.41 kB gzip) | [M] `npm run build --workspace @retinue/desktop` |
| Entry CSS | **555.96 kB** (91.76 kB gzip) | [M] same build |
| Lazy chunks | reply icons 596.13 kB (101.33 gzip), Lanyard/three 534.49 kB (139.97), pdf.js 409.98 kB (123.26), OpenUI 184.55 kB (57.49) | [M] same build |
| All JavaScript in `dist` | 3.95 MB | [M] same build |
| CSS source | 652,467 characters, 6,767 lines, 75 `@keyframes`, 252 `box-shadow`, 59 gradients, 13 `filter: blur`, 5 `backdrop-filter` rules in `ui/` (6 lines with the vendor-prefixed twin) | [M] counted over `apps/desktop/src/ui/**/*.css` |
| App component size | `ui/App.tsx` 4,611 lines, 377 kB, 170 `useState`, 101 `useEffect`, 4 `useMemo`, 12 `useCallback` | [M] line and call counts |
| `React.memo` in the UI | 0 uses | [M] `Select-String` over `ui/*.tsx` |
| Components that skip a re-render | none: every transcript row, every sidebar row and the composer are rebuilt on each App render | [V] `ui/App.tsx:3159`, `ui/App.tsx:3240` |

What this machine cannot measure, and why: **no Chromium-based browser is installed** (Chrome and Edge are absent from the usual paths), so the browser preview's CDP tools could not be run, and the installed app was out of bounds by the brief. So every number about the *real* WebView2 frame time, commit count and memory is marked [U] and given a method in section 8.

Two numbers from earlier waves are reused and cited rather than re-measured: the shipped database configuration and per-delta write cost (2.6 to 11.6 ms per streamed token, `synchronous=FULL`, from `references/lessons.md`, 1 November 2026), and the seed sizes (cold start 567 ms median; `app.snapshot` 1,060,106 bytes, lazy 452,586 bytes, from `perf-budgets.json`).

## 3. Where the time goes today

### 3.1 One token costs one whole-app render [V]

The chain for every streamed chunk is:

1. The adapter emits `AgentEvent::AssistantDelta`: one event per provider chunk. The Claude adapter sends every chunk straight on (`crates/retinue-adapter-claude/src/lib.rs:554-556`). The Codex and ACP adapters coalesce only as a **back-pressure valve**, when their forward queue already holds 128 events (`crates/retinue-adapter-codex/src/lib.rs:119-132,141`; `crates/retinue-adapter-acp/src/lib.rs:144-156,170`); in steady state nothing merges.
2. The daemon appends to the message row: one `UPDATE messages SET content=...` of the growing string (`crates/retinue-storage/src/lib.rs:6742` `append_message_delta`), then one `INSERT INTO app_events` (`crates/retinue-storage/src/lib.rs:2906` `push_event`). Both are autocommit on a `synchronous=FULL` connection (`crates/retinue-storage/src/lib.rs:2756`), so each token costs about two fsyncs: 2.6 to 11.6 ms measured on the owner's data (`references/lessons.md`, 1 November 2026). The written bytes grow with the reply: a 5,000-character reply in 4-character chunks rewrites about 3 MB in total.
3. The daemon broadcasts one `EventEnvelope` (`crates/retinue-daemon/src/main.rs:275` `persist_event`, `:281`), the gateway writes one WebSocket frame, and the shell emits one Tauri event per frame (`apps/desktop/src-tauri/src/lib.rs:1251,1263`).
4. The window applies it with `setSnapshot(applyEvent(...))` (`apps/desktop/src/ui/App.tsx:1185-1222`), which appends the text and returns a **new snapshot object and a new sessions array** (`apps/desktop/src/types.ts:484`, `:623-653`).
5. `App` re-renders. There is no `memo`, no split store and no windowing, so this one state change re-runs, on the full transcript:
   - `buildConversationItems(selectedSession)` (`ui/App.tsx:705`, `ui/conversation.ts:114-156`: filter, map, two sorts, three maps),
   - `groupConversationActivity(...)` (`ui/App.tsx:850`, `ui/conversation.ts:191-230`),
   - `delegationRuns(...)` (`ui/App.tsx:853`), `outlineItemsFromMessages(...)` (`ui/App.tsx:899`),
   - `parseMarkdownBlocks` and `tokenizeInlineMarkdown` **for every message** (`ui/SafeMarkdown.tsx:221-247` has no `useMemo` on the text), measured at 33 ms per 1,000 messages with no DOM at all [M],
   - React element creation and reconciliation for every row (`ui/App.tsx:3159`), then the commit and paint.
6. A layout effect then sets `scrollTop = scrollHeight` (`ui/App.tsx:1173-1177`) whenever `sessionMessages` changes, i.e. every token. Reading `scrollHeight` after a render forces a synchronous layout of the whole transcript.

So the cost of **each** update grows with the whole conversation, and a streamed reply's total cost grows with the square of its length. A 1,000-message chat is the case where the owner feels it, and it is also the case the existing gate measures.

### 3.2 A keystroke costs the same whole-app render [V]

The composer's text is `App` state (`const [composer, setComposer] = useDraft(composerKey)`, `ui/App.tsx:672`), and the textarea is controlled by it (`ui/App.tsx:3240`). Each `onChange` calls `setComposer` and then, in the same handler, `slashQuery`, `mentionQuery` and `hashQuery`. React 19 batches them into one render, but that render is still the whole `App`: the transcript, every sidebar row, the rail, the header. The draft store itself is well behaved (writes are debounced 250 ms, `ui/draftStore.ts:23,135-138`), so the cost is React, not storage.

The autosize effect also reads `scrollHeight` on every value change (`ui/useAutosize.ts:8-16`), a small forced layout, but on the composer only.

### 3.3 Conversation switching [V]

`selectSession` sets two or three pieces of state and tells the shell (`ui/App.tsx:2795-2808`); there is no RPC before paint, so the switch itself is immediate. What the owner waits for is the render of the new transcript: the same pipeline as 3.1 with a different session, plus, if the conversation was only a summary, a `session.get` fetch (`ui/App.tsx:617-648`). On the owner's data a conversation read is 0.5 ms once indexed, but 100 ms when it carries 185 tool payloads (`references/lessons.md`, 1 November 2026), and the transcript then renders every one of those rows.

This is what the other apps call "thread switching". Pandacode's "sub-1 ms" claim is about this step, but its release notes describe the work behind it as warm agent spares, background handshakes, Codex preloading and cached model catalogs (0.3.134, withdrawn and re-shipped as 0.3.135), not as a DOM technique [X]; Retinue's equivalent step has no provider in it at all (the transcript is already in the snapshot), so its cost is pure render, which is what section 6.3 and 6.4 address.

### 3.4 The sidebar and the shell [V]

The sidebar renders every project and conversation as rows on every render, and recomputes `unreadSessionIds` and `approvalSessionIds` with three `filter`+`Set` passes (`ui/App.tsx:829-833`). At 200 conversations this is cheap in isolation (a few hundred µs [U]); the problem is only that it runs 30 to 60 times a second during a stream, together with everything else.

### 3.5 CSS and motion [V][M]

- The transcript has no `content-visibility` and no `contain`; every mounted row is laid out and painted.
- Every row starts a 180 ms entry animation on mount (`.message-row { animation: msg-in ... }`, `ui/shell.css`; `@keyframes msg-in` is opacity + `translateY(4px)`, which is the cheap kind). Opening a 1,000-message conversation therefore starts 1,000 animations and 1,000 compositor layers at once. This is motion the owner likes; the cost is that *all* of it runs at once on a mount.
- Haze's backdrop is static and cheap at rest: one fixed layer of four large radial gradients with no animation (`ui/themes/haze.css:104-111`). Good.
- The frosted sheets and dialogs use `backdrop-filter: blur(28px) saturate(1.15)` (`ui/themes/haze.css:278-286`; `ui/teamPage.css:57`). A `backdrop-filter` over a region that is repainting is re-blurred; while a turn streams behind an open Settings sheet or a dialog, that is a real per-frame cost. [U] (mechanism certain, magnitude unmeasured here).
- The chat background's effects "run on the main thread (30 to 140 ms at full size)" by the design language's own note (`references/design-language.md`), which is a known, accepted cost when the owner turns it on.

### 3.6 Start-up [V]

- The window paints the launch screen at first render and is shown by the shell 30 ms later (`apps/desktop/src/main.tsx:43-45`), but the owner cannot use the app until the launch checks finish, and `LaunchIntro` holds the screen for **at least 2,000 ms** after they settle (`ui/LaunchIntro.tsx:15,49`).
- Those checks include `runtime.refresh`, which invalidates the whole model catalog and re-probes every enabled coding agent, then prewarms every catalog in the background (`crates/retinue-daemon/src/main.rs:1541-1546`; `refresh_runtime_discovery` at `:978`). There is no cross-launch cache of "this binary was version X at mtime Y", so every relaunch spawns the CLIs again. Synara measured 27% less process-tree memory and 12.8% faster relaunch-to-typing by warming only the provider a new thread would use, and Pandacode caps `--version` probes at 4 s and caches the answer per binary build (`docs/providers.md`/CHANGELOG 2cc7952). [X]
- The bundle is one entry of 2.36 MB: `App.tsx` imports every page statically (`ui/App.tsx:16-166`: Settings sheet, Analytics, Tasks, Schedules, Team, Blob page, Terminal with xterm at `ui/TerminalDock.tsx:2-3`, Document pane, Machine conversation, Compare), so all of it is parsed before the first frame, in both the main window and the companion (`ui/App.tsx:2810`), so a 344 × 62 pill pays for the whole app.

What is already right: fonts are loaded per choice by dynamic import (`ui/fonts.ts:5-17`), pdf.js, three.js/Lanyard and the reply-icon set are already lazy (the build puts them in separate chunks [M]), and the snapshot is lazy (`fetchSnapshot` → `app.snapshot {lazy:true}`, `src/tauri.ts`).

### 3.7 The companion window [V][U]

The companion runs the same `App` component, so it runs the same 170 state hooks, the same 101 effects, fetches the same snapshot and subscribes to the same event stream, then returns `<Companion>` at `ui/App.tsx:2810`. Bot art is not the problem: `BlobCanvas` sleeps until the next blink and stops its animation frame when hidden or not intersecting (`blob/BlobCanvas.tsx:212-226`), and only group-start rows mount an avatar canvas. The problem is duplicate work per event and a duplicate bundle parse. Exact memory is unmeasured [U] and is in the budget to establish.

### 3.8 SQLite per keystroke and per event [V]

- Per keystroke: nothing reaches SQLite. Drafts are localStorage, debounced 250 ms (`ui/draftStore.ts`). Good.
- Per streamed token: two autocommit writes (section 3.1), and the `UPDATE` rewrites the whole reply each time.
- Per event: `push_event` inserts the payload JSON; for `message.delta` that is only the delta, but for `session.changed` it is a whole session summary (`ui/App.tsx:988-997` relies on that row), which is fine.

### 3.9 The first requests and WebView2 settings [V][U]

The window's first requests after connect are, in order: `app.snapshot {lazy:true}` (`src/tauri.ts` `fetchSnapshot`), `events.replay`, `startDaemonEventStream`, `runtime.refresh` (through the discovery hook, section 3.6), `quota.list` once connected, `settings.get`, then the launch checks' own calls. On the seed database the existing gate measures the snapshot at 1,060,106 bytes full and 452,586 bytes lazy; on the owner's data a full snapshot was 13 MB and 1.0 s, lazy 2.1 MB and 99 ms, and `session.get` 0.5 ms except when a conversation carries large tool payloads (100 ms) (`references/lessons.md`, 1 November 2026). Those are the numbers to keep: the snapshot and the tool payloads, not the query engine.

`apps/desktop/src-tauri/tauri.conf.json` sets **no** `additionalBrowserArgs` and no WebView2 performance flags; it sets `transparent: true`, `visible: false` (shown after the first render, `main.tsx:43-45`) and a strict CSP. The relevant knobs are therefore all open, and unmeasured here: `additionalBrowserArgs` (for example to keep timers unthrottled in the companion while it is occluded, which is a behaviour choice, not a free win), the WebView2 profile's cache location, and whether the production bundle is served from the asset protocol with compression. None is a substitute for items 1 to 6; treat any of them as an experiment to measure with section 8 [U].

## 4. What must not be broken

- The two looks, their motion and the companion's feel (`references/design-language.md`). Every proposal below keeps the animation vocabulary; it changes *when* and *how often* it runs.
- `docs/reference/invariants.md`: nothing here touches remote tiers, budgets, secrets or approvals.
- The "everything is demoable" rule: a new setting (for example "solid glass while a turn streams") needs its demo entry and preview fixture in the same change.
- The lessons' traps: never make the UI eagerly import a look's CSS (`main.tsx:24-27`), and never put a per-conversation read on the writer lock (`lessons.md`, 1 November 2026).

## 5. How the others do it

### Pandacode (`aicodeking/pandacode`)

**The claim.** On 20 November 2026 the author posted: "PandaCode is about 10X faster throughout. Especially Thread switching is sub-1ms now. Mid-turn harness switching is also way faster now!" and on 13 November: "PandaCode's thread switching and opening are about 10X faster now." [X] (quoted by several mirrors; the repository's own releases are the checkable part).

**What is checkable.** Release 0.3.134 ("fast-switching": warm agent spares, background handshakes, Codex preloading, live Claude Code switches, cached model catalogs, instant composer picks) was **withdrawn the same day** and replaced by 0.3.133 as 0.3.135, with the note that installed copies never downgrade (commit `e551ffb`, 20 November 2026). [X] So the 10x is the author's claim, not a published measurement of switching; treat "sub-1 ms" as a goal, not a fact.

**What it actually ships, and the method behind the numbers:**

- `bench/t3-compare/` is a reproducible benchmark of *streaming* cost against T3 Code, with the full method published: CDP `Performance.getMetrics` (TaskDuration, ScriptDuration, LayoutDuration, RecalcStyleDuration), wall time, last-delivery-to-paint, worst delivery lateness, JS heap before/after GC, mutation records, and an idle baseline in the same page state. Published medians, 9 runs each, on an M4 Max: **task time 2,947 ms vs 8,332 ms (ratio 0.35)**, of which **script 288 ms vs 5,632 ms (0.05)**, heap after 11.5 vs 54.2 MB, DOM mutation records 7,114 vs 3,960. [X]
- The workload is 2,000 provider deltas of 25 bytes every 16 ms. **PandaCode's delta coalescer** (Rust `Coalescer`, `emit_coalesced`) opens a buffer on the first delta, flushes at 8 KiB or 33 ms, and carries only the *new* text: 2,000 deltas became **667 UI events**. T3 Code's two windows turn the same stream into 500 events carrying the whole text each. [X]
- Its README's "Resource limits" section says: only the current **100-message page** of a conversation is loaded; terminal and Markdown code load only when needed; explicit limits on history, provider frames, diffs and streamed text; **no background directory polling or recursive watchers**; agents exit after each turn. [X]
- Release 0.3.133: "When rendering stays expensive, translucent panels switch to solid fills" and "Streaming replies grow without briefly jumping back to an earlier height". [X] This is the "keep the look, degrade the effect when the frame budget is gone" pattern.
- It is plain TypeScript with no React runtime [X], which is why its script time is 20× lower; that part is not transferable to Retinue, but coalescing, paging and adaptive effects are.

### T3 Code (`pingdotgg/t3code`)

- **Server-side coalescing, verified in source**: `apps/server/src/orchestration-v2/ThreadLiveEventCoalescer.ts` keeps a `COALESCE_WINDOW = 50 ms` and `MAX_PENDING_UPDATES = 512`, superseding earlier updates per entity and flushing on any non-coalescible event. Provider text is coalesced a second time at 50 ms (`ProviderTextDeltaCoalescer`, `CODEX_ASSISTANT_DELTA_FLUSH_INTERVAL_MS = 50`). [V] (file read this session)
- **Virtualised timeline and the React Compiler**: the benchmark's description of T3's build names "React with the React Compiler" and "the timeline (LegendList)". [X]
- So T3 accepts a React-based UI but never lets a token reach React more than once per 50 ms, and never renders more rows than the viewport needs.

### Synara (`Emanuele-web04/synara`, docs at trysynara.com)

Its changelog is unusually honest, with measured before/after numbers. [X]

| Change | Before → after |
| --- | --- |
| Opening/closing the sidebar no longer re-renders chats per frame | 14 to 16 React commits → **3** |
| Chats start loading on a sidebar press; settled Markdown is reused when rows come back into view | first opens **16 to 20% less time**; repeat scroll flings **19 to 29% less scripting** |
| Remote catch-up sends only the active chat's changes | **676 KB → 2 KB**; iOS Simulator applied it in 55 ms, down from 73 ms |
| Provider discovery warms only the provider a new thread would use | process-tree memory **3,009 MB → 766 MB** (20 threads, 12 projects) |
| Startup defers unused provider work | relaunch-to-typing **1.715 s → 1.495 s** (12.8%) |
| Offscreen image previews release memory; SQLite reclaims pages in bounded passes | (no whole-app claim) |

The first two rows are the same disease as section 3.1 in a React app of similar shape (it is a T3 fork), and they are the best evidence that memoising components and caching parsed Markdown, rather than rewriting the app, is what moves this needle.

## 6. The plan, ranked

Each item: what to change, why, expected gain, cost, risk to the looks and motion, who does it, and how to prove it. "Director" is frontend, "Ada" is backend per the project's lane rules.

### 1. Coalesce streamed deltas before they reach the UI (Ada; Director for the last step)

**Change.** In the daemon, buffer `AssistantDelta` per (session, turn) and flush on the first of: 33 to 50 ms, 4 to 8 KiB, any other event for that session, turn end, or shutdown. Write the message row once per flush (append the buffered text) and emit one `message.delta` whose `delta` is the concatenated chunk; the client's existing append (`types.ts:641-645`) applies it unchanged and replay order is preserved because every non-delta event flushes first. Keep `sequence` monotonic and one event id per flush. In the shell, optionally also batch Tauri emits per 16 ms frame into one event array that `App` applies in one `setState`.

**Why.** It divides the number of renders, IPC messages, fsyncs and bytes written during a stream by roughly three at 33 ms (Pandacode's measured 2,000 → 667), and by more for fast providers. It also removes the "one render per token" pattern that makes a long conversation janky while a reply arrives.

**Gain (estimate).** Three to ten times fewer renders and database commits during a turn, and a proportionate cut in the main-thread work each token causes. For a slow stream (20 chunks/s) this alone may be enough; for a fast one it is the difference between "busy" and "smooth" once items 2 and 5 land. [U] until the benchmark exists.

**Cost.** 2 to 4 days backend plus tests; the design must respect the two constraints the 1 November lesson already identified (flush before any event of that session, and accept losing the buffered tail on a crash; `message.completed` and the persisted row are the authority).

**Risk.** No visual change. The only behaviour change is that a crash may lose up to one flush interval (33 to 50 ms) of the last partial message. Owner decision (section 9).

**Prove it.** Count daemon commits and emitted events per 1,000-token turn before and after; measure the per-token time in `snapshot-timing`-style example code (it already reports "one message-delta write").

### 2. Stop re-rendering the world: memoise the rows and split the live state (Director)

**Change.** In this order, each measurable on its own:
1. `memo(MessageItem)` and a stable avatar node; pass primitives where possible; `useCallback` the handlers it receives. Same for `ActivityItem`, `MessageActions`, sidebar rows.
2. `useMemo` the four derived transcripts on `[selectedSession?.id, messageCount, toolsCount, filesCount, turnsCount]` (`App.tsx:705, 850, 853, 899`), which removes the rebuild on every keystroke and on every unrelated state change.
3. `useMemo` the Markdown parse inside `SafeMarkdown` on `text`, and memo the component (Synara's "reuse settled Markdown").
4. Further out: move the transcript's live text into a small external store (one subscription per streaming message) so that a delta updates one row, not `App`.

**Why.** Removes the measured 33 ms of Markdown work per render on a 1,000-message transcript, and turns "one keystroke = 1,000 rows" into "one keystroke = one composer".

**Gain (estimate).** The single largest frontend win: a delta or keystroke that today costs the whole transcript becomes one row. Synara's comparable work took a sidebar toggle from 14 to 16 React commits to 3, and made first opens 16 to 20% faster. [X]

**Cost.** 3 to 6 days, largest of the frontend items, because handler identity must be stabilised without changing behaviour; the mounted tests will find regressions.

**Risk.** None visual if memoisation is correct; the risk is stale rows (a missed dependency), which the mounted tests and the benchmark will catch.

**Prove it.** Commit count per keystroke and per delta, and the time from input to the next paint, both in the new benchmark (section 8).

### 3. Window the transcript (Director)

**Change.** Render the last N items (start with 60) plus an "Earlier messages" sentinel that loads the previous page on scroll, reusing the existing `earlierWindow(current, total, step)` helper (`ui/chatScroll.ts:34`, already used by the companion at `App.tsx:4326`). Keep `stickToBottom`, the outline, annotations and `jumpToMessage` working by expanding the window when a target is outside it.

**Why.** Mount cost becomes proportional to the viewport, not the conversation: switching to a 1,000-message chat stops mounting 1,000 rows and starting 1,000 entry animations. Pandacode loads a 100-message page; T3 uses a virtualised list.

**Gain (estimate).** Switch time for a long chat falls from "the whole transcript" to "the visible page", which is where the 100 ms budget becomes reachable. [U]

**Cost.** 3 to 5 days. The dangerous parts are scroll anchoring and search jumps.

**Risk.** A behaviour change the owner will feel: scrolling to the very top loads history instead of already being there. Nothing about the visual language changes.

**Prove it.** Rows in the DOM for a 1,000-message conversation (expect about 60), first-paint time on switch, and scroll scripting time.

### 4. Make the composer local (Director)

**Change.** Move the textarea, the autosize and the slash/mention/hash scanning into a small `Composer` component with its own state, seeded from and published to `useDraft`; the App only learns about text on submit, blur and the pickers' insertion points. Keep the controller ref for focus and selection.

**Why.** A keystroke stops re-rendering the transcript, the sidebar and the rail: the 16 ms budget becomes reachable regardless of conversation length.

**Gain (estimate).** Keystroke cost becomes constant rather than proportional to the open chat. [U]

**Cost.** 2 to 3 days; many call sites read `composer`, so the seam must be deliberate (an external store is the clean version).

**Risk.** Focus and caret behaviour are the subtle part; the composer tests cover most of it.

**Prove it.** Input-to-paint p95 while a 1,000-message chat is open.

### 5. Make the bottom-scroll and the layout cheap (Director)

**Change.** Replace the per-delta `scrollTop = scrollHeight` layout effect (`App.tsx:1173-1177`) with one scroll per animation frame, only while `stickToBottom`, scheduled after paint; add `content-visibility: auto` and `contain-intrinsic-size` to `.message-row` so off-screen rows skip layout and paint. Animate `msg-in` only on the newest row (`.message-row:last-child`), or only on rows appended after the first paint.

**Why.** Removes a forced layout of the whole transcript from every token, and stops 1,000 simultaneous entry animations on a switch. Motion is kept; it just runs on the row that is new.

**Cost.** 1 to 2 days.

**Risk.** `content-visibility` interacts with scroll anchoring, the outline and `scrollIntoView`; verify in both looks. The animation change is visible only when several rows mount at once, which today is a wash of 1,000 fading rows.

**Gain (estimate).** Removes one full-transcript layout per token, and the mount-time animation storm; scroll cost becomes viewport-sized. [U]

**Prove it.** Long-task count while streaming into a 1,000-message chat, and scroll frame times.

### 6. Split the bundle and give the companion its own entry (Director)

**Change.** `React.lazy` the pages that are not the chat: Settings, Analytics, Tasks, Schedules, Team, Blob page, Document pane, Terminal dock (xterm), Machine conversation, Compare, Quick switcher, Image lightbox, Project search. Keep every look's CSS in the entry (the `main.tsx:24-27` lesson). Add a second Vite entry for the companion (or an early dynamic import of `Companion`) so the pill loads only what it draws.

**Why.** The entry is 2.36 MB today, and the same bytes are parsed again in the companion window. The lazy chunks already prove the pattern works here (pdf.js, three, icons).

**Gain (estimate).** Entry under 450 kB gzip from 699 kB is plausible once Settings, xterm and the pages leave the entry (their source alone is about a third of `ui/`); the companion gains the most because it draws almost none of it. [U] until measured.

**Cost.** 2 to 3 days plus loading skeletons.

**Risk.** A lazy chunk's first open can flash; the design already has `LoadingBlock` for exactly this. No look change.

**Prove it.** Entry gzip size, time to first frame, companion heap.

### 7. Start faster (Ada for probes, Director for the hold)

**Change.**
1. Cache the CLI version and protocol probe per binary, keyed on path + size + mtime (Pandacode's approach), so a relaunch reuses yesterday's answer and only a changed binary is probed.
2. Warm the catalog for the selected bot's provider at launch, and warm the rest on first use or in idle (Synara's approach).
3. Defer `catalog::prewarm_all` and any other work the first conversation does not need.
4. Re-examine the fixed 2,000 ms hold after the checks settle (`LaunchIntro.tsx:15`), and show the window when the first real frame is ready rather than after a fixed wait.

**Why.** Startup is where "feels heavy" starts; every measured comparable found its win in *not doing work* at launch (Synara: 12.8% faster relaunch-to-typing, 75% less process-tree memory in the 20-thread case; Pandacode: probe cap and cache).

**Gain (estimate).** Relaunch-to-typing from roughly 2.5 s or more to about 1.5 s on a warm machine, and one CLI spawn per changed binary instead of one per launch. [U]

**Cost.** 3 to 4 days across both lanes.

**Risk.** A stale probe would hide a broken CLI upgrade. Mitigate: invalidate on mtime/size change and on an explicit "Scan again"; show the age of the cached check.

**Prove it.** Relaunch-to-typing over 9 relaunches on a warm machine; process tree memory; number of CLI spawns at launch (log lines already exist per start step).

### 8. Keep the glass, drop the cost when the frame budget is gone (Director; owner sign-off)

**Change.** Two separate things:
1. Never re-blur a large surface for nothing: while a turn is streaming, the frosted sheets and dialogs (`.settings-sheet`, `.blob-dialog`, `.terminal-dock`, `.team-dialog`: `haze.css:278-286`, `teamPage.css:57`) may fall back to the opaque glass colour without `backdrop-filter`, exactly as Haze already falls back when Windows does not draw the blur. The main backdrop and its blobs never change.
2. Watch the frame budget (a rolling count of frames over 33 ms) and, after a sustained miss, apply the same fallback until it recovers. This is Pandacode's "translucent panels switch to solid fills" (`0.3.133`).

**Why.** Blur is the one effect whose cost scales with what is moving behind it, and the sheets sit over the transcript.

**Gain (estimate).** Only when a sheet is open over a streaming turn, which is exactly when the app is at its worst; no change at rest. [U]

**Cost.** 1 to 2 days plus a demo fixture for the fallback state.

**Risk.** This is the only proposal that changes what the owner sees in Haze, and only while the app is already struggling. It must be a decision, not a silent behaviour (section 9). Stone is untouched.

**Prove it.** Frame times with Settings open while a turn streams; a baseline measure of the same frame times is needed first.

### 9. Trim the render path's constant costs (Director)

- `messageClock` builds a new `Date` and `toLocaleTimeString` per row per render (`App.tsx:3493-3494`): memoise per `createdAt` (it is also called for the action bar). Small, but it is per row per token.
- `new Set(...)` of unread and approval ids per render (`App.tsx:829-833`): memoise on `[sessions, seenSessions, permissions]`.
- The per-row booleans in the item loop (`hasUserOrAssistantMessage`, `filesUnderReply`) and the `new Date(...)`/`toLocaleTimeString` pair behind each stamp (`App.tsx:3493-3494`): cheap alone, repeated 1,000 times a second today. Memoise per message.
- The message list's `onScroll` sets state on every crossing (`App.tsx:3156`); it is already boolean-guarded by React's bail-out, so keep it but move it behind a `requestAnimationFrame` if the benchmark shows it.

**Why.** Each is a few µs; together they are the difference between "just under" and "comfortably under" 16 ms once 1 to 5 are done.

**Cost.** 1 day.

### 10. A budget and a benchmark the gates can run (Director, with Ada for the daemon side)

Detailed in section 8. This is the item that keeps the other nine from regressing, and the one that would have caught the current render gate's failure earlier.

### Held back, and why

- **React Compiler.** T3 Code and Synara both use it; it would auto-memoise components and may deliver part of item 2 cheaply. But it hides rather than removes the per-token `App` render, it changes every component's semantics at once, and it would make the before/after of items 2 to 5 hard to attribute. Try it *after* items 2 to 5, with the benchmark, as its own change. [U] on the gain in this codebase.
- **Rewriting the transcript in vanilla DOM (Pandacode's route).** The measured gap is real (script 288 ms vs 5,632 ms) but the cost is the whole UI, the looks and the motion system. Not worth it for the owner's goal.
- **A canvas-drawn transcript.** Same objection, and it would lose text selection.
- **Removing Haze's native blur.** The owner asked for it; it is not on the CPU path the window repaints (it is a DWM effect), and the CSS blobs are static. Leave it.
- **Image and bot-art loading.** Measured as a non-problem: art is drawn on canvases that sleep when idle, only group starts mount one, the only pictures in the transcript are small WebP profile photos (256 px) and 132 px thumbnails of sent images, and large generated images and chat backgrounds already have their own paths. Nothing to win here beyond ensuring off-screen rows stop painting (item 5).

## 7. The performance budget

Numbers are proposals with a reason, not round figures. "Today" is what this session could establish; "unknown" means the method in section 8 must establish it first, and the budget should be set from a median of nine runs plus a margin, as `perf-budgets.json` already says.

| Budget | Target | Today |
| --- | --- | --- |
| Switch to a 1,000-message conversation, warm, to the first frame that shows the transcript's last row | **≤ 100 ms** | unknown; the render gate's 20 to 75 s is a test DOM, not the app |
| Keystroke to paint, 1,000-message conversation open | **≤ 16 ms p95** | structurally above 16 ms: one keystroke rebuilds every row and re-parses 33 ms of Markdown [M] |
| Streaming into a 1,000-message conversation | **≤ 4 ms** main-thread work per update, **≤ 1 React commit per 50 ms**, worst delivery lateness **≤ 16 ms** | unknown; one commit per token today [V] |
| First window frame (launch screen visible) | **≤ 300 ms** from process start | unknown |
| Relaunch to typing (checks settled, app usable) | **≤ 1.5 s** warm with three CLIs, median of 9 | unknown; the fixed hold alone is 2.0 s [V] |
| Long-chat scroll, 1,000 messages | median **≥ 55 fps**, worst frame **≤ 33 ms** | unknown |
| Companion, idle 60 s, nothing happening | **0** animation frames, **≤ 0.5%** one core, heap recorded as a number to watch | unknown |
| Entry JavaScript | **≤ 700 kB gzip** today, **≤ 450 kB gzip** after item 6 | 699.41 kB [M] |
| Entry CSS | **≤ 100 kB gzip** | 91.76 kB [M] |
| Daemon commits for a streamed reply | one commit pair per 33–50 ms flush, **≤ 5%** of today's | one pair per chunk: about 2,000 for a 1,000-chunk reply [V] |
| `app.snapshot` (lazy), seed database | ≤ 700,000 bytes | 452,586 measured 31 October; unchanged |
| Cold start, seed database | ≤ 4,000 ms | 567 ms median measured 31 October |

Budgets mark a regression alarm, not a promise about every machine; the existing file says so, and the same wording applies.

## 8. A repeatable benchmark

**What to build: `scripts/perf-ui.mjs`**, modelled on Pandacode's published harness (`bench/t3-compare/README.md` and `run.mjs`), because it is the only method I found that measures the thing the owner feels rather than a test DOM.

**Where it runs.** Against the **browser preview** (`apps/desktop/vite.preview.config.ts`, which already swaps `../tauri` for fixture data), served as a production build (`vite build` + `vite preview`), in headless system Chrome through CDP. The repository's own verification notes already describe this pattern: read `/json/version`, connect to `webSocketDebuggerUrl` with a small Node script, `Runtime.evaluate` with `returnByValue` (`references/verification.md`, browser preview section). Node 24 has a global `WebSocket`, so no dependency is needed.

**Fixtures it needs** (test-only, in `src/preview`, never shipped):
- a conversation with 1,000 mixed messages, including a long Markdown reply with headings, a code fence, a list and a table (the shape the existing render gate uses);
- a way to deliver daemon events exactly as the app receives them: the preview already has `emit(name, payload)` in `fixtureTauriApi.ts`, so expose a bench-only `window.__benchEmit('daemon-event', event)` behind the preview flag, the way Pandacode exposes `window.__previewEmit`.

**Workloads (each a median of 9 runs, alternating order, fresh page per run):**

- **W0 idle baseline.** Same page, nothing delivered, same duration. Subtracting it separates fixed background work from the cost of the workload (Pandacode's method).
- **W1 switch.** Click a sidebar row for the 1,000-message conversation; measure time to the first animation frame whose DOM holds the last row.
- **W2 type.** 60 synthetic keystrokes 50 ms apart into the composer with the 1,000-message conversation open; measure input-to-next-paint p50/p95 and long tasks above 50 ms.
- **W3 stream.** 2,000 deltas of 25 bytes every 16 ms into the open conversation, through `__benchEmit` with the event shape the daemon sends; measure CDP TaskDuration (and its script/layout/style parts), worst delivery lateness, mutation records, heap before/after GC, and React commits.
- **W4 scroll.** 20 programmatic flings of the transcript; measure scripting time and worst frame.
- **W5 companion.** Open the companion route, idle 60 s, measure heap, CDP TaskDuration and animation-frame count.

**React commit counting.** Pandacode counts DOM mutations; commits are better. Two options: a `window.__REACT_DEVTOOLS_GLOBAL_HOOK__` shim installed before the bundle loads that increments on `onCommitFiberRoot` [U] (needs a probe), or a temporary counter in a `useEffect` with no dependency array in `App` (a bench-only build flag). The first is preferred; if it proves fragile, count mutation records as Pandacode does and set the threshold on those.

**Wiring.** Add a `ui` section to `perf-budgets.json`, `npm run perf:ui`, and a step in the CI `perf` job (it already exists, `references/verification.md`, 17 November 2026). Fail when a median exceeds its budget; print min/max and the run count, as the existing gate does. Keep it out of `npm test` (it needs Chrome and takes minutes), like `perf:render`.

**Also fix the existing gate.** `npm run perf:render` fails here (median 39.1 s against 30 s) and its times climb run after run (20 → 39 → 75 s), which means the harness accumulates something between mounts; a gate that only goes up is not measuring the app. Either run each measurement in its own process, or retire it in favour of W1 in the browser benchmark, and say which in `perf-budgets.json`. Do not raise the 30 s budget to make it green: the file's own comment forbids that without a reason.

## 9. Owner decisions

1. **The 2 s launch hold.** Keep the designed moment, or let the app open as soon as the checks settle (about 0.3 to 0.6 s on a warm launch) with the launch screen only while a check is still running? This is a felt trade between ceremony and speed.
2. **Haze's glass during a struggle.** May Haze fall back to a solid glass colour (no `backdrop-filter`) for sheets and dialogs while frames are long, and recover automatically? The main backdrop, blobs and every other look stay as they are.
3. **A windowed transcript.** Scrolling to the very top of a long conversation will load history rather than having it already mounted. Accept, or keep the full mount behind a setting?
4. **A crash may lose up to 50 ms of a streamed reply** under delta coalescing (the row and `message.completed` remain the authority; a reload shows the last flushed text). Accept?
5. **React Compiler**: try it as a later experiment, or leave it out?

## 10. What I could not measure

- Real WebView2 frame times, commit counts and heap: no Chromium-based browser installed here; the method is section 8.
- The browser preview: same reason.
- This machine's daemon binary (Rust) build: not built, so the per-delta write cost is quoted from the 1 November measurement rather than reproduced. Building here also risks the Smart App Control blocks recorded on 19 November.
- Companion memory and CPU: no way to attach to WebView2 without launching the installed app, which was out of bounds.
- Whether the render gate's growth is a leak in the app or in the harness: it needs a per-run process split and a heap dump to say.

## 11. Sources

Repository (read this session): `apps/desktop/src/ui/App.tsx`, `ui/SafeMarkdown.tsx`, `ui/conversation.ts`, `ui/chatScroll.ts`, `ui/draftStore.ts`, `ui/useAutosize.ts`, `ui/launchChecks.ts`, `ui/LaunchIntro.tsx`, `ui/runtimeDiscovery.ts`, `ui/fonts.ts`, `ui/themes/haze.css`, `ui/shell.css`, `ui/settings.css`, `ui/teamPage.css`, `src/types.ts`, `src/tauri.ts`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `apps/desktop/vite.config.ts`, `apps/desktop/vite.preview.config.ts`, `apps/desktop/package.json`, `crates/retinue-daemon/src/main.rs`, `crates/retinue-storage/src/lib.rs`, `crates/retinue-adapter-{claude,codex,acp}/src/lib.rs`, `scripts/perf-gate.mjs`, `scripts/perf-render.mjs`, `apps/desktop/src/ui/perf.mounted.test.tsx`, `perf-budgets.json`, `.claude/skills/retinue-development/references/{architecture,design-language,lessons,verification}.md`, `docs/reference/invariants.md`.

Pandacode:

- https://github.com/aicodeking/pandacode (repository, README "Resource limits", released 0.3.136 on 21 November 2026)
- https://github.com/aicodeking/pandacode/tree/main/bench/t3-compare and https://raw.githubusercontent.com/aicodeking/pandacode/main/bench/t3-compare/README.md
- https://raw.githubusercontent.com/aicodeking/pandacode/main/bench/t3-compare/results/summary.md (W1 results, 29 October 2026)
- https://raw.githubusercontent.com/aicodeking/pandacode/main/releases/0.3.133.md ("Smoother startup and less background work")
- commit `e551ffb6c00594f5a50d2881b598cf79dce9a2aa` ("revert: undo the 0.3.134 fast-switching release…"), and `2cc7952e3050d276ac7ac0cce348139853e5c27e` ("cap and cache CLI probes…")
- @AICodeKing, 13 and 20 November 2026, quoted by mirrors (xitter.gq/aicodeking, teapawt.net/aicodeking): "PandaCode's thread switching and opening are about 10X faster now" / "about 10X faster throughout. Especially Thread switching is sub-1ms now".

T3 Code:

- https://raw.githubusercontent.com/pingdotgg/t3code/main/apps/server/src/orchestration-v2/ThreadLiveEventCoalescer.ts (read this session: `COALESCE_WINDOW = 50 ms`, `MAX_PENDING_UPDATES = 512`)
- T3's two-stage coalescing and the `LegendList` timeline, as described in Pandacode's benchmark README (section 5 above).

Synara:

- https://raw.githubusercontent.com/Emanuele-web04/synara/main/CHANGELOG.md (entries for 1.6.4, 1.6.3, 1.5.x and 1.4.x read this session; quotes in section 5)
- https://www.trysynara.com/docs (what the app is; fetched this session)

## Appendix: raw measurements

Markdown parse, Node 24.20, this machine (`bench-md.mjs`, 590-byte reply, 6 blocks, 1,000-message transcript = 590 KB of text):

```
oneReplyMs:        median 0.02 ms   (min 0.0192, max 0.2415, 31 runs)
transcript1000Ms:  median 33.20 ms  (min 27.39, max 54.34, 7 runs)
transcriptOnceNoMemo10x: median 524.79 ms (10 passes over the 1,000 messages)
```

Render gate (`npm run perf:render` in this worktree, budget 30,000 ms):

```
mount times: 20112.3, 39145.5, 75356.4 ms (median 39145.5 ms) -> FAIL
```

Build (`npm run build --workspace @retinue/desktop`):

```
dist/assets/index-CmD2KWnZ.js   2,361.25 kB │ gzip: 699.41 kB   (entry)
dist/assets/index-DzNOPJp-.css    555.96 kB │ gzip:  91.76 kB   (entry)
dist/assets/index-BurdUap1.js     596.13 kB │ gzip: 101.33 kB   (lazy: reply icons)
dist/assets/Lanyard-4m3-SVop.js   534.49 kB │ gzip: 139.97 kB   (lazy)
dist/assets/pdf-DEyMsg58.js       409.98 kB │ gzip: 123.26 kB   (lazy)
dist/assets/OpenUiBlock-*.js      184.55 kB │ gzip:  57.49 kB   (lazy)
all JavaScript in dist: 3.95 MB
```

CSS source (`apps/desktop/src/ui/**/*.css`): 652,467 characters, 6,767 lines, 75 `@keyframes`, 252 `box-shadow`, 59 gradients, 13 `filter: blur`, 5 `backdrop-filter` rules (6 lines with the vendor-prefixed twin).
