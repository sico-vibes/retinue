# Moving the Bitbots UI to native Rust: is it worth it?

Research only, 8 November 2026. No code was changed. The question from the owner: could the UI leave Tauri 2 + React/TypeScript (WebView2) for a native Rust stack, mainly GPUI, to be faster and use less memory, while keeping the Haze look and every feature?

## Summary

- **The real, cited gain is memory and cold start, not speed.** An idle Tauri 2 window on Windows costs about 250 MB private memory (330–350 MiB resident) across one app process and about six WebView2 processes. The same kind of window in a native Rust toolkit costs about 20–25 MiB. Most of that WebView2 cost is fixed, not caused by our code. ([zsui matrix](https://docs.rs/crate/zsui/latest/source/docs/ui-performance-matrix.md), [strand PR #30](https://github.com/danielss-dev/strand/pull/30), [tauri#5889](https://github.com/tauri-apps/tauri/issues/5889))
- **The slowness the owner has felt so far was in the daemon, not the UI.** Every measured slowdown in `lessons.md` (1 November 2026) was SQLite locking and query shape: lazy snapshots from 8.1 s down to 99 ms, `session.get` from 81 ms down to 0.5 ms. A new UI stack would not have changed any of those. What is still slow is also backend: tool payloads (96 of the 100 ms of the slowest conversation) and an fsync per streamed token (2.6–11.6 ms each).
- **The UI's own render speed in WebView2 has never been measured.** The only number we have is a 1,000-message conversation taking about 8 s to mount in happy-dom (`perf-budgets.json`), which is a test DOM, not WebView2. The UI has no memoised rows, no list virtualisation and no deferred rendering (none found in `apps/desktop/src`), so there are cheap fixes we haven't tried.
- **GPUI is the strongest native candidate**, because it ships Zed on Windows and has window-level Mica, Mica Alt and blur. It is also the riskiest on accessibility, has no stable API, and draws SVG as still images only. None of the frameworks reproduces Haze's animated SVG portraits, in-window backdrop blur and CSS-driven styling without re-authoring them.
- **A full port is roughly 6–9 months of focused work for one experienced Rust UI developer.** That means rewriting about 37k lines of UI, 7k lines of CSS and 25k lines of DOM tests, and keeping the remote PWA in React, so there would be two UIs.
- **Recommendation: don't port now.** First measure WebView2 as it really runs and make the cheap fixes listed below (about 2–3 weeks). If memory is still the complaint after that, run a bounded GPUI spike on the companion window only, with go/no-go criteria, before committing to anything larger.

## 1. Feature inventory: what a native stack must match

Measured in this checkout (`main` at `d5708cf`): 280 non-test TS/TSX files (~37k dense lines; `ui/App.tsx` alone is ~3,900), 219 test files (~25.6k lines), 89 CSS files (~7.1k lines) with 239 `box-shadow`, 65+ `@keyframes`, 165 `transition`, and about 2,050 `aria-*`/`role=` uses across 229 files. The native shell (`apps/desktop/src-tauri/src`) is ~7.5k lines of Rust, and most of it stays whatever the UI becomes.

| Capability | Where it lives now | What a native port needs | Difficulty |
|---|---|---|---|
| Translucent acrylic window, blur fallback | `src-tauri/src/backdrop.rs` (`window-vibrancy` `apply_acrylic`, then `apply_blur`; `EnableTransparency` check), `ui/windowBackdrop.ts` | Window-level DWM backdrop. GPUI has `Blurred`, `MicaBackdrop` and `MicaAltBackdrop`; others can reuse `window-vibrancy` on the raw window handle | Low–medium |
| In-window frosted surfaces | `backdrop-filter`/`blur()` in 10 CSS files (5 in `ui/themes/haze.css`) | Blurring the window's own content behind a panel. Not a standard primitive in GPUI, Iced, Slint or egui (to verify per framework); needs a custom shader pass or dropping the effect | High |
| Rounded rectangles, shadows, gradients, masks, conic gradients | `ui/theme.css`, `ui/themes/haze.css`, `ui/polish.css`, `ui/accent.css` | Every native stack draws rounded rectangles and shadows. Masks, conic gradients and blend modes vary | Medium |
| Animated character portraits | `blob/haze/characters/<slug>/{avatar,animated}.svg`: 29 characters, 58 files, 1.46 MB. The animation is **CSS `@keyframes` inside the SVG** (for example `bot-body`, `bot-eye` in `ada-lovelace/animated.svg`), transforms only, no SMIL, no filters | Native toolkits rasterise SVG once (resvg or similar) and don't run CSS inside it. Each animated portrait has to be split into parts and animated in code, or moved to Rive or Lottie | High (29 assets) |
| Canvas art | `blob/blobEngine.ts` (glossy blob), `blob/BitbotCanvas.tsx` + `vendor/robot-heads/` (3D robot), `blob/art/PaintedBlob.tsx` | Rewrite as GPU drawing code (paths, or a small wgpu pass for the 3D robot) | Medium–high |
| Markdown, tables, charts | `ui/SafeMarkdown.tsx` (own parser, no library), `ui/RichTable.tsx`, `ui/RichChart.tsx`, `ui/richData.ts` | A markdown renderer with selectable rich text. gpui-component has one, plus charts | Medium |
| Syntax highlighting | **None today** (`<pre><code>` only in `SafeMarkdown.tsx:127`) | Optional. Tree-sitter or syntect if wanted | Not a parity item |
| Diffs and review | `ui/DiffBody.tsx`, `ui/ChangesView.tsx` | Virtualised text grid with line comments | Medium |
| PDF and media preview | `ui/PdfViewer.tsx` (pdf.js), `ui/MediaPreview.tsx` | pdfium or MuPDF bindings, image and video decoding | Medium–high |
| Terminal dock | `@xterm/xterm` 6 in `ui/TerminalDock.tsx`, `ui/terminalHub.ts`, native `src-tauri/src/pty.rs` | A terminal grid. `alacritty_terminal` (used by Zed) is the usual choice; the PTY side stays | Medium |
| Text input, IME, dictation | Composer in `ui/TeamMessages.tsx`/`ui/ComposerControls.tsx`; dictation island (`?dictation=1`), `bloblex-speech` | A multi-line editor with full IME (CJK composition, emoji picker), mentions (`#` project, `@` bot), drafts, pasted images. Dictation's audio side stays in Rust | High (IME is where most Rust toolkits fall short) |
| Accessibility | ~2,050 ARIA attributes and roles; Narrator works through WebView2's UIA tree today | AccessKit (UIA on Windows) wired into every control | High, the main risk |
| Three windows | Main, companion and dictation (`WebviewWindowBuilder`, undecorated, transparent, always-on-top, skip-taskbar; `src-tauri/src/lib.rs` ~2942 and ~3057), companion resize commands | Multi-window with per-window transparency and z-order. GPUI and Slint support this | Medium |
| Drag and drop | File drop on the companion, task cards in `ui/DelegateBoard.tsx` | OS file drop plus in-app drag | Medium |
| Tray, global shortcut, single instance, updater, notifications | Tauri plugins in `lib.rs` | Replace with `tray-icon`, `global-hotkey`, own updater and toast code | Medium |
| 24 bundled fonts, font picker | `ui/fonts.ts`, `@fontsource*` | Load the font files into the text system | Low |
| Remote phone client | `apps/desktop/src/remote` (PWA, reuses React UI and `tiers.json`) | Stays web. A native desktop UI means two UI codebases | Ongoing cost |
| Browser preview and DOM tests | `apps/desktop/src/preview/`, ~25k lines of `*.test.ts(x)` and `*.mounted.test.tsx` | Rebuild a test harness. GPUI has `TestAppContext`; visual checks move from the browser to screenshots | High |
| macOS later | Tauri covers it today | GPUI is strongest on macOS (Zed's home). Others vary | Varies |

The daemon (`crates/bloblex-daemon`) and its loopback HTTP + WebSocket API (`docs/ipc-contract.md`) are already a clean seam: a native UI would talk to the same `POST /v1/rpc` and `GET /v1/events`, so nothing behind it changes.

## 2. Framework comparison

| | GPUI (+ gpui-component) | Slint | Iced | egui | Dioxus Native (Blitz) | Makepad | Xilem / Masonry |
|---|---|---|---|---|---|---|---|
| Maturity | Ships Zed on macOS, Linux and Windows. No semver; used as a pinned git dependency | 1.x, commercial company, stable | 0.14, used by COSMIC | Mature, widely used | Pre-alpha (`blitz-html` 0.3.0-alpha.4, May 2026) | Active, small team, few production apps | Experimental |
| Windows | Yes (Zed for Windows); DWM backdrops | Yes | Yes | Yes | Yes, with gaps | DX11 backend | Yes, via winit |
| Licence | Apache-2.0 | GPL-3 / royalty-free (newer terms require an "uses Slint" badge) / commercial | MIT | MIT / Apache-2.0 | MIT / Apache-2.0 | MIT / Apache-2.0 (check repo LICENSE) | Apache-2.0 |
| Window blur / Mica | Built in: `Blurred`, `MicaBackdrop`, `MicaAltBackdrop` | Via raw handle + `window-vibrancy` | Same | Same | Same | Same | Same |
| SVG | Static (rasterised); `svg()` is icon-oriented, `img()` for full colour (to verify) | Static | Static (resvg) | Via `egui_extras` (static) | SVG as image, CSS animation inside SVG not supported (to verify) | Partial | Via Vello (static) |
| Rich text and IME | Strong editor (Zed); IME works in Zed on Windows | Good, IME supported | Improving; IME was partial in 2025 | Basic rich text; IME partial | Parley text; IME partial | Own editor | Parley; IME being tested |
| Animation | Animation helpers; per-frame redraw | Declarative animations in the DSL, strong | Animation API new in 0.14 | Manual, immediate mode | CSS transitions partly | Shader-driven, strong | Basic |
| Accessibility | AccessKit work in progress upstream; forks ship it. **Unclear for upstream on Windows today** | AccessKit since 1.1, on all desktop platforms | Not shipped | AccessKit | AccessKit planned | Not documented | AccessKit (core design goal) |
| Markdown, highlighting, charts | gpui-component: markdown + HTML, Tree-sitter highlighting, editor, charts, optional Wry webview | Build yourself | Community crates | `egui_commonmark` | It's HTML, so yes | Build yourself | Build yourself |
| How much of Haze it can reproduce | ~80%: shapes, shadows, gradients, Mica/acrylic, fonts. Not: in-window backdrop blur (needs a custom pass), CSS-animated SVG | ~75%, in Slint's own DSL; same gaps | ~65%; styling is coarser | ~50%; immediate-mode look is hard to make feel like Haze | Could reuse much of the CSS if coverage were complete; today it isn't | ~75% with shader work; everything re-authored | ~50%, too early |

Notes:

- **GPUI.** Zed moved its Windows backdrops to the documented DWM API (`DwmSetWindowAttribute`) and added Mica and Mica Alt, with a Windows 10 fallback ([Zed commit for #41842](https://git.secluded.site/zed/commit/917148c5ce6b4801dee7ab191b6fb5144f84d0ea); [`WindowBackgroundAppearance` docs](https://docs.rs/gpui-ce/0.3.2/gpui/enum.WindowBackgroundAppearance.html)). Accessibility is the open question. Forks such as open-gpui and rgpui document AccessKit with a UIA adapter on Windows ([open-gpui a11y source](https://docs.rs/crate/open-gpui/0.2.0/source/src/window/a11y.rs), [rgpui accessibility docs](https://docs.rs/crate/rgpui/0.2.2/source/src/_accessibility.rs)). A third-party note reports both that a Zed AccessKit PR was closed in May 2026 and that `gpui_windows` depends on AccessKit ([Syrinx wiki note](https://github.com/Fledge-Software/Syrinx-app/wiki/Background-08-gpui-and-wry-on-windows)). Those two claims conflict, so check Zed's repository before relying on GPUI for Narrator support. [gpui-component](https://github.com/longbridge/gpui-component) supplies the widget layer Zed doesn't publish: markdown and HTML rendering, Tree-sitter highlighting, charts, and an experimental Wry-based WebView element.
- **Slint.** It has accessibility since 1.1 through AccessKit ([Slint 1.1 post](https://slint.dev/blog/slint-1.1-released)). The royalty-free licence now asks you to show that you use Slint ([licence terms in 1.17](https://components.espressif.com/components/slint/slint/versions/1.17.0/license?language=en)). Its `.slint` DSL is a third language for the team, next to Rust and TypeScript.
- **Iced.** 0.14 added an animation API ([release notes](https://github.com/iced-rs/iced/releases/tag/0.14.0)). Accessibility is not in the release notes.
- **Dioxus Native / Blitz.** It is the only option that could reuse our CSS. It renders HTML and CSS with Stylo, Taffy, Parley and Vello, without a webview. Its own README still says not to build apps with it yet, and the Dioxus 0.7 post says not every CSS feature works ([Blitz repo](https://github.com/DioxusLabs/blitz), [Dioxus 0.7](https://dioxuslabs.com/blog/release-070), [blitz-html releases](https://lib.rs/crates/blitz-html)). Worth watching, not ready.
- **Makepad** styles everything with shaders and has a live design tool ([repo](https://github.com/makepad/makepad/)). It is powerful for motion, but all of Haze would be re-authored in its DSL, and its accessibility isn't documented.
- **Xilem / Masonry** puts accessibility first through AccessKit, but it is experimental and has a small team ([Linebender, March 2025](https://linebender.org/blog/tmil-15/), [2024 post-mortem](https://poignardazur.github.io/2025/03/24/plan-for-linebender-post-mortem)).
- **Across the field.** The April 2025 survey of 43 Rust GUI crates found most of them failed accessibility or IME at least partly. Readers noted that if Windows + accessibility + IME is the baseline, what remains is essentially webview-based ([survey coverage](https://weeklyrust.substack.com/p/the-state-of-rust-gui-the-good-and), [discussion](https://redlib.hackliberty.org/r/rust/comments/1jyy8u2/2025_survey_of_rust_gui_libraries)). The Dioxus team said in that thread that they chose a webview because of IME and accessibility. Things have improved since (Slint, Zed's Windows port), but this is still the main gap.

## 3. Performance: what is measured and what is assumed

### Memory (cited, external)

| Stack | Idle memory on Windows | Source |
|---|---|---|
| Tauri 2 / WebView2, empty window | ~332–346 MiB RSS, ~82–85 MiB private for the app's own process; 1 app process + 6 WebView2 processes | [zsui UI performance matrix](https://docs.rs/crate/zsui/latest/source/docs/ui-performance-matrix.md) |
| Tauri 2 / WebView2, empty shell | ~248 MB private, "the six-process floor" | [strand PR #30](https://github.com/danielss-dev/strand/pull/30) |
| Tauri on heavy pages | ~370–399 MB, similar to Electron | [tauri#5889](https://github.com/tauri-apps/tauri/issues/5889) |
| Slint, comparable window | ~21–25 MiB, one process | zsui matrix (above) |

These are other projects' numbers, measured in different ways (resident vs private). Bitbots runs **three** WebView2 windows (main, companion, dictation), which share the WebView2 browser process but add renderer memory each. We haven't measured our own process tree. The first step is to do that on the installed app (Task Manager's "Details" view, or `Get-Process msedgewebview2 | Measure-Object WorkingSet64,PrivateMemorySize64 -Sum`).

A native GPUI or Slint UI would probably bring the UI side to tens of megabytes. The saving is about 200 MB or more, and it is real. The daemon's memory doesn't change.

### Speed (measured, ours)

- Every slowdown we have measured was in the daemon and storage (`.claude/skills/bitbots-development/references/lessons.md`, 1 November 2026): one SQLite mutex, a 12–22 s full snapshot, and a correlated subquery that scanned the event log once per turn. After the fixes: lazy snapshot 99 ms, `session.get` 0.5 ms, an effort save 3.5 ms. A native UI would have inherited the old numbers unchanged.
- Still open, and also backend: tool payloads (96 of 100 ms on the slowest conversation; needs tool output on demand), and one fsync per streamed token (2.6–11.6 ms).
- The UI side is **unmeasured in WebView2**. `perf-budgets.json` records 4.5–14.5 s to mount a 1,000-message conversation in happy-dom, and says itself that this is a regression alarm, not a WebView2 figure.
- The code has known render costs: no `memo(` on message rows, no virtualised conversation list, no `useDeferredValue`/`startTransition`, and a ~3,900-line `App.tsx` whose state changes re-render large trees. GPUI would avoid some of this by design (retained, GPU-drawn, only dirty views redraw), but a virtualised, memoised React list gets most of the same effect.

### Cheaper fixes inside the current stack

| Fix | Expected gain | Effort |
|---|---|---|
| Measure first: the WebView2 process tree's private bytes with all three windows; a Performance trace (Edge DevTools on WebView2) of opening and scrolling a 1,000-message conversation and of streaming | Tells us whether there is a UI problem at all | 1–2 days |
| Virtualise the conversation log and memoise message rows (`React.memo`, stable props); split `App.tsx` state so a streamed token re-renders one row | Large for long conversations and streaming | 1–2 weeks |
| Tool output on demand (already planned in `lessons.md`) | Removes most of the remaining conversation-open cost | ~1 week (daemon) |
| Batch streamed deltas before writing (keeping replay order) | Removes an fsync per token | ~1 week (daemon) |
| Create the dictation window only when needed and close it afterwards; hide the companion's animations when it's collapsed or off screen (portraits already pause with `IntersectionObserver` in `BlobCanvas.tsx`) | Less memory and less idle GPU | Days |
| Set WebView2 `MemoryUsageTargetLevel` to `Low` for hidden windows and back to `Normal` when shown ([Microsoft docs](https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2.memoryusagetargetlevel)); reachable through the native WebView2 handle | Lower working set while the app is in the background | Days (needs a native call through `webview2-com`) |

The fixed WebView2 floor (~250 MB) stays whatever we do. Everything else above is likely to get most of the *felt* speed, at a small fraction of a port's cost.

## 4. Effort, phasing and recommendation

### Rough effort for a full GPUI port

About 37k lines of UI, 7k lines of CSS and 29 animated portraits to re-author, plus the test harness. Estimate: **6–9 months for one experienced Rust UI developer**, longer with lanes that are new to GPUI. GPUI's API changes without notice, so every Zed update would cost time.

### Staffing (owner decision, 8 November 2026)

If the port goes ahead, the owner's decision is: *"for this remake to rust we will have opencode go deepseek v4.1 flash on high do the converting, while if anything in my task require frontend changes you will do them."*

- **Conversion: the Gogo lane** (OpenCode Go, DeepSeek V4.1 Flash, high). It turns React and TypeScript screens into GPUI Rust, one self-contained brief at a time. Several conversations may run at once, each in its own worktree at a short path, as the 31 October amendment in `AGENTS.md` allows. Lanes never run `git commit`.
- **Frontend and visuals: the Director** (Claude). This covers Haze styling (tokens, shadows, radii, Mica and acrylic tuning, the in-window blur pass), re-authoring the animated portraits, motion, copy, and the visual-fidelity check of every converted screen against the current app. The Director also writes the briefs, reviews every diff, runs the gates, merges and commits.

What this split implies:

- **The Director builds the visual base before any conversion.** Gogo can only reproduce Haze faithfully if it has something to copy. In Phase 1 the Director sets up a `theme` module that turns `ui/theme.css` and `ui/themes/haze.css` tokens into Rust constants, plus a small kit of styled primitives (surface, row, button, segmented control, pill-free badge, dialog). From then on, a brief tells the lane to use only the kit and tokens, never raw colours or sizes. This keeps the design language in one place, as `references/design-language.md` requires.
- **Each brief is a vertical slice.** One screen or component, its `tauri.ts` and daemon calls (the RPC client is shared), its state, and its tests. It names the source files to port, what done looks like, and a parity checklist taken from the existing `*.mounted.test.tsx`. Briefs avoid shared files where possible, so parallel Gogo conversations don't collide. The Director owns the app shell, routing and the kit.
- **Each slice ends with a Director gate.** The Director compares screenshots of the old and new screens in Haze dark and light, then fixes spacing, alignment and motion, before the slice merges. A converted screen isn't done until it passes.
- **GPUI is new to the lane.** Expect several rounds of corrections on the first briefs. A worked example in the kit (one fully converted screen) and a short GPUI patterns note in the skill should come before the lane starts in bulk.

### Phases

"Gogo" marks a conversion brief the lane can take on alone. "Director" marks work that stays with the Director.

| Phase | Work | Owner | Time | Exit criteria |
|---|---|---|---|---|
| 0. Measure and fix in place | The fixes table in section 3. The daemon parts can be Gogo briefs; the React render fixes are frontend | Gogo (daemon), Director (React) | 2–3 weeks | Our own numbers for memory, open time and streaming frame times |
| 1a. Foundations | GPUI app skeleton, window creation (main, companion, dictation; transparency, always-on-top, skip-taskbar), the daemon RPC and WebSocket client in Rust, tray, global shortcut, single instance | Gogo | 2–3 weeks | Windows open, events arrive, existing `src-tauri` logic reused |
| 1b. Visual base | Theme tokens, primitive kit, Mica/acrylic tuning, fonts, the first portrait re-done in code, the worked example screen | Director | 2–3 weeks, alongside 1a | The kit matches Haze in dark and light |
| 1c. Spike gate (go/no-go) | Companion window: chat list, composer with IME (Japanese and Chinese input), Narrator check | Gogo builds; Director checks looks, IME and Narrator | 2 weeks | Narrator and IME pass; looks like Haze next to the current companion; memory and frames measured against Phase 0 |
| 2. Main shell | One brief each: sidebar (bots and projects views), conversations column, conversation view and message items, markdown renderer (or gpui-component's), approval cards, model and effort controls, composer | Gogo per brief; Director owns the shell layout and checks each slice | 2–3 months | Daily use possible |
| 3. The rest | One brief each: every Settings page (one or two per brief), tasks board and dialogs, review and diffs, terminal dock (`alacritty_terminal` over the existing `pty.rs`), PDF and media, schedules, machines, usage, onboarding | Gogo per brief; Director for onboarding visuals, the look editor, and each slice's check | 2–3 months | The section 1 parity checklist |
| 3b. Art and motion | The remaining 28 animated portraits, canvas art (glossy blob, 3D robot, painted), the companion's motion, the in-window blur pass | Director | Alongside Phase 3 | Side-by-side match with the current art |
| 4. Parity and switch | Accessibility audit (Gogo can wire AccessKit roles per screen from a list; the Director checks with Narrator), the test harness, removing WebView2 from the desktop bundle, a final visual pass | Gogo and Director | 1 month | Release |

The remote PWA stays React. From Phase 2 on, there are two UIs to keep in step, and changes to the phone client stay frontend work.

**Hybrid option:** move only the companion and dictation windows to GPUI and keep the main window in WebView2. This removes the always-on-top windows' renderers, but the WebView2 browser process and the main window's renderer stay, so most of the floor remains, and we would keep two desktop UI stacks. It only makes sense if the companion alone turns out to be the memory or smoothness problem.

### Recommendation

1. **Don't start a port now.** The slowness we have evidence for was backend, and it is mostly fixed. The UI's own performance in WebView2 hasn't been measured, and the cheap React fixes haven't been tried.
2. **Do Phase 0** (measure, virtualise and memoise, tool output on demand, batched deltas, lighter hidden windows).
3. **If the ~250 MB floor still matters after that**, run the Phase 1 GPUI companion spike, with accessibility and IME as hard gates. Choose GPUI over the others for its Windows shipping record, built-in Mica and acrylic, editor and text quality, and gpui-component. Choose Slint instead if the spike shows GPUI's accessibility isn't ready upstream and the licence badge is acceptable.

## 5. Risks

- **Accessibility and IME regressions.** WebView2 gives us UIA and IME for free today. GPUI's upstream AccessKit status is unclear (section 2). This is the main blocker.
- **API churn.** GPUI has no semver and lives in Zed's monorepo. We would pin a revision and pay to update it.
- **The art.** The 29 animated portraits use CSS inside SVG, which no native toolkit runs. Re-authoring them (in code, or in Rive or Lottie) is design work, not just engineering.
- **In-window frosted glass.** `backdrop-filter` on panels has no standard native equivalent. It needs a custom blur pass or a design change.
- **Losing the browser preview and DOM tests.** The way the Director checks UI today (preview presets, mounted tests, the browser pane) doesn't carry over. Verification becomes slower until a new harness exists.
- **Two UIs.** The phone client stays React, and so does any shared presentation logic.
- **Design workflow.** Styling moves from CSS tokens to Rust code. Every visual tweak becomes a recompile.
- **PDF, charts, media.** pdf.js and the SVG charts need native replacements.
- **Licence.** Slint's royalty-free terms need a visible "uses Slint" badge. GPUI, Iced, egui and Makepad are permissive.
- **macOS later.** GPUI is strongest on macOS, so this one counts in its favour.
- **The conversion lane is new to GPUI.** A fast model converting tens of thousands of lines can produce code that compiles but drifts from Haze, or that misuses GPUI's entity and context model. The visual base, the worked example and a Director check on every slice are what contain this. Without them, the visual-fidelity work piles up at the end.

## Sources

- Zed, DWM backdrops and Mica (#41842): https://git.secluded.site/zed/commit/917148c5ce6b4801dee7ab191b6fb5144f84d0ea
- GPUI `WindowBackgroundAppearance`: https://docs.rs/gpui-ce/0.3.2/gpui/enum.WindowBackgroundAppearance.html
- GPUI on Windows with Wry, AccessKit notes (third party): https://github.com/Fledge-Software/Syrinx-app/wiki/Background-08-gpui-and-wry-on-windows
- open-gpui accessibility (fork): https://docs.rs/crate/open-gpui/0.2.0/source/src/window/a11y.rs
- rgpui accessibility docs (fork): https://docs.rs/crate/rgpui/0.2.2/source/src/_accessibility.rs
- gpui-component: https://github.com/longbridge/gpui-component and https://docs.rs/crate/gpui-component/0.4.0/source/README.md
- zsui UI performance matrix (Tauri vs Slint and others): https://docs.rs/crate/zsui/latest/source/docs/ui-performance-matrix.md
- strand, WebView2 idle floor: https://github.com/danielss-dev/strand/pull/30
- Tauri memory on WebView2: https://github.com/tauri-apps/tauri/issues/5889
- Blitz: https://github.com/DioxusLabs/blitz, https://lib.rs/crates/blitz-html
- Dioxus 0.7 (Dioxus Native): https://dioxuslabs.com/blog/release-070
- Slint 1.1 (AccessKit, royalty-free licence): https://slint.dev/blog/slint-1.1-released
- Slint licence in 1.17: https://components.espressif.com/components/slint/slint/versions/1.17.0/license?language=en
- Iced 0.14: https://github.com/iced-rs/iced/releases/tag/0.14.0
- Makepad: https://github.com/makepad/makepad/
- Linebender / Xilem: https://linebender.org/blog/tmil-15/, https://poignardazur.github.io/2025/03/24/plan-for-linebender-post-mortem
- 2025 Rust GUI survey, coverage and discussion: https://weeklyrust.substack.com/p/the-state-of-rust-gui-the-good-and, https://redlib.hackliberty.org/r/rust/comments/1jyy8u2/2025_survey_of_rust_gui_libraries
- WebView2 `MemoryUsageTargetLevel`: https://learn.microsoft.com/en-us/dotnet/api/microsoft.web.webview2.core.corewebview2.memoryusagetargetlevel
- In this repository: `.claude/skills/bitbots-development/references/lessons.md` (1 November 2026 performance entries), `perf-budgets.json`, `apps/desktop/src-tauri/src/backdrop.rs`, `apps/desktop/src-tauri/src/lib.rs`, `apps/desktop/src/blob/haze/characters/`, `apps/desktop/src/ui/SafeMarkdown.tsx`.

Claims marked "to verify" come from forks, third-party notes or secondary coverage and weren't checked against the upstream source. GPUI's upstream accessibility status matters most among them.
