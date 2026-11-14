# The driver for the owner's Windows computer (Phase C), 14 November 2026

Research for the Director, from the owner's approval of the "browsers and computer control" amendment in `AGENTS.md`. **Research only: no code, no configuration and no dependency was changed.** Every claim cites either a place in this repository (`file:line`) or an outside source (URL, and a path inside a repository where the claim is a line of code). Anything I could not check is marked **unverified**.

Order of work: section 1, then 2, then 3, then 4. Each section was written as it was finished.

## What this report settles, in one paragraph

The earlier report (`docs/research/owner-requests-2026-11-14.md:455`) said three things about the Windows driver: that `open-computer-use`'s Windows runtime might be text-only, that its screenshot support was unknown, and that the driver choice was open. **All three are now settled.** The Windows runtime does take screenshots, per window, through `PrintWindow(PW_RENDERFULLCONTENT)` (`docs/windows-targeting.md`, in `iFurySt/open-codex-computer-use`); it masks password controls on a fail-closed rule (`apps/OpenComputerUseWindows/runtime.ps1:1043-1050`); and it runs as a stdio MCP child whose default action path never moves the real pointer (`apps/OpenComputerUseWindows/main.go:41`, `docs/windows-input.md`). That combination is unusually close to the amendment's rule, so the recommendation is to use it for Phase C's first cut, pinned by hash, behind Retinue's own tools — with a specific list of things Retinue must force rather than trust it to do.

---

## 1. Candidate drivers for Windows

### 1.1 How I judged them

The amendment (`AGENTS.md`, "Amendment (14 November 2026): browsers and computer control") fixes the shape before any product choice:

- the default answer to "what is on screen" is the **accessibility tree with password fields masked** (`AGENTS.md`, bullet "Screens are sent only when the owner sends them");
- a screenshot goes to a provider **only after the owner sees that exact frame and approves it**;
- the driver runs as **Retinue's supervised child process over stdio**, so consent, the indicator, the log and the tier rules stay in the daemon (`AGENTS.md`, bullet "A third-party driver is a supervised child");
- the driver's licence and pinned version are recorded in `docs/operations/`.

So five properties matter more than features: does it read the UI Automation tree, does it mask `IsPassword`, does it take Windows screenshots at all (needed later, on the approved-frame path), does it speak MCP over stdio, and how big is the surface it puts next to the owner's desktop.

### 1.2 The shortlist

| Driver | Licence | Language | Last release | Tools | Reads UIA tree | Masks `IsPassword` | Windows screenshots | stdio child |
|---|---|---|---|---|---|---|---|---|
| **`open-computer-use`** (`iFurySt/open-codex-computer-use`, mirrored as `anomalyco/computer-use`) | MIT, "Copyright (c) 2026 Leo" | Go (Windows runtime) + PowerShell bridge; Swift for the macOS app | **v1.1.4**, 2026-11-07 | 9, app-scoped | **Yes**, per app, 1,200 nodes / 64 levels / 500 chars by default | **Yes, fail-closed** | **Yes**, per window, `PrintWindow(PW_RENDERFULLCONTENT)` | **Yes**: `open-computer-use.exe mcp` |
| **`Windows-MCP`** (`CursorTouch/Windows-MCP`) | MIT | Python 3.14+ | **v0.8.9**, 2026-11-12 | **21**, whole desktop, **including `PowerShell`, `FileSystem`, `Registry`, `Process`** | Yes, whole desktop | Marks the field, **but still emits the value** (§1.4) | Yes, whole desktop or a region (dxcam → mss → pillow) | Yes: `windows-mcp serve` (also SSE/HTTP) |
| **`FlaUI-MCP`** (`shanselman/FlaUI-MCP`) | MIT | C# (.NET 8, self-contained ZIP) | **v0.2.1**, 2026-11-07 | 12, window-scoped, `ref`-based | Yes (FlaUI/UIA3), depth 10 by default | **No handling found** in its snapshot code | **Yes**: element, window, background window, or full screen | Yes: `FlaUI-MCP.exe` |
| **`nuphus-mcp`** (`mrpulor-gh/nuphus-mcp`) | MIT | **Rust** | **v0.3.4**, 2026-11-08 | 7 semantic UIA tools + pixel tools + browser + OCR/vision | Yes (own `windows_uia.rs`) | **Unverified** | Yes (`xcap`), plus OCR/vision that **sends frames to your own model** | Yes, "one binary", JSON-RPC over stdio |
| **`Python-UIAutomation-for-Windows`** (`yinkaisheng`) — a library, not a server | Apache-2.0 | Python | last push 2026-06-02 | none (building block) | Yes | the UIA property set is exposed; **not read in detail** | no | no |

Sources: GitHub API `https://api.github.com/repos/<owner>/<name>` and `/releases` for each row, fetched 14 November 2026; per-repo detail below.

Rejected without a full table:

- **`shuyu-labs/Windows-MCP.Net`** (.NET, MIT, 240 stars): last push **2025-11-27**, a year stale.
- **`mario-andreschak/mcp-windows-desktop-automation`** (MIT, TypeScript): drives **AutoIt**, a separate automation runtime with its own window-matching language. Two scripting hosts in the trust path is one too many.
- **`dddabtc/winremote-mcp`** (MIT, Python, "40+ tools … process management, file operations"): a remote-control server. The amendment makes this path never-remote; a server built for remote use is the wrong starting point.
- **`civyk-official/civyk-winwright`** (PowerShell, 52 tools): GitHub reports licence `NOASSERTION` — there is no licence a release can be shipped under.
- **`dividduang/uiautomation_mcp`** (Apache-2.0): 22 stars, a thin wrapper over `yinkaisheng`'s library. Interesting as reading material, not as a dependency.
- **`helloHub1212/dsh-windows-mcp`** (MIT, JavaScript, 1,152 stars): its own description says it is "the in-box MCP client bridge mounted against `CursorTouch/Windows-MCP`" — a derivative, so it inherits every limit in §1.4 and adds a distributor.
- **Anthropic's `computer-use-demo`**, **Bytebot**, **UI-TARS-desktop**: not Windows desktop drivers in MCP form (Docker/X11, container, and a full app with its own vision model respectively). Out of scope rather than unsuitable.

### 1.3 `open-computer-use`: what the Windows runtime actually does

This is the one the earlier report recommended, on paper. Now that I have read the source, several of its properties are better than the earlier report could confirm, and two need care.

**Packaging and installation.** Two paths, and they are not equivalent.

- `npm i -g open-computer-use` installs a Node package with four bin shims (`bin/ocu`, `bin/open-computer-use`, `bin/open-computer-use-mcp`, `bin/open-codex-computer-use-mcp`) that exec a platform binary. The package is 39,065,226 bytes unpacked over 57 files, and it carries `dist/windows/amd64/open-computer-use.exe` (**2,467,328 bytes**) and `dist/windows/arm64/open-computer-use.exe` (2,388,992 bytes) — `https://unpkg.com/open-computer-use@1.1.4/?meta`. Registry metadata: latest `1.1.4`, published `2026-11-07T07:02:17Z`, `license: MIT` (`https://registry.npmjs.org/open-computer-use`).
- The GitHub release `v1.1.4` (2026-11-07) ships `open-computer-use-cli-1.1.4-windows-amd64.zip`, `…-windows-arm64.zip` and **`SHA256SUMS`** (`https://api.github.com/repos/iFurySt/open-codex-computer-use/releases`).

For Retinue the second path is the right one: **the `.exe` needs no Node**. It is built with `GOOS=windows GOARCH=amd64 CGO_ENABLED=0` (`scripts/build-open-computer-use-windows.sh`), so it is a static Go binary, and the PowerShell script it runs is compiled into it — `//go:embed runtime.ps1` at `apps/OpenComputerUseWindows/main.go:38`. Nothing has to be on the owner's PATH except `powershell.exe`, which Windows always has.

**It is a stdio MCP server, and it is small.** `open-computer-use.exe mcp` calls `runMCP(os.Stdin, stdout)` (`apps/OpenComputerUseWindows/main.go:1136-1137`, `:1453-1508`). The loop is `json.NewDecoder(stdin)` / `json.NewEncoder(stdout)` — **newline-delimited JSON, not LSP-style `Content-Length` framing**. That matters for a Rust supervisor: write one JSON object per line and read one per line. `initialize` answers `protocolVersion: "2025-03-26"` (`:1482`); `tools/list` and `tools/call` are the only other methods, plus `ping`.

**The nine tools** (`apps/OpenComputerUseWindows/main.go:959-1054`):

| Tool | Required arguments | Notes from its own schema |
|---|---|---|
| `get_app_state` | `app` | `include_ax` (default **true**), `include_screenshot` (default **true**), `text_limit` (default 500, or `"max"`), `max_tree_nodes` (1200), `max_tree_depth` (64). Read-only. |
| `list_apps` | — | Currently running apps **plus any used in the last 14 days, with usage frequency** (`:1002`). |
| `click` | `app` | By `element_index` or by `x`/`y` in screenshot pixels; `click_method` = `auto` / `accessibility` / `app_post` / `global`. |
| `type_text` | `app`, `text` | No global mode: writes into the focused `Edit`/`RichEdit` control. |
| `press_key` | `app`, `key` | `key_method` = `auto` (window messages) / `global` (real keys). |
| `scroll` | `app`, `element_index`, `direction` | `pages` may be fractional. |
| `drag` | `app`, four coordinates | Pixel coordinates. |
| `set_value` | `app`, `element_index`, `value` | **Verifies the write and errors when the read-back does not match** (`:1039`). |
| `perform_secondary_action` | `app`, `element_index`, `action` | Invokes a secondary UIA action. |

Every tool is **app-scoped**: there is no "the whole desktop" primitive, and no shell, no file access, no registry, no process control. That is the whole list, and it is the strongest single reason to prefer it over `Windows-MCP`.

**The password rule is fail-closed, which is rarer than it should be.** `apps/OpenComputerUseWindows/runtime.ps1`:

- `function Test-PasswordElement($element) { try { return ($element.Current.IsPassword -ne $false) } catch { return $true } }` (`:1044-1046`). Read that literally: an element is treated as a password **unless it says it is not one**, and a failure to answer counts as a password. Unknown is guilty.
- `Get-ElementValue` returns `""` for such an element before it even asks for the value (`:1048-1049`).
- The tree cache asks for `IsPassword` as a cached property (`:1105`), and the value is only read when `$info.IsPassword -eq $false` (`:1136-1138`).
- Selected text is refused for password elements and for embedded Chromium controls (`Read-SafeSelectedText`, `:1320-1322`).

**Screenshots work on Windows, and they are per window.** `docs/windows-targeting.md` states the mechanism and its limits plainly:

> Windows screenshots use `PrintWindow(PW_RENDERFULLCONTENT)` on the same top-level HWND used by the accessibility snapshot. The bitmap includes the full window, so screenshot coordinates remain physical pixels relative to `GetWindowRect`, including at 200% display scaling. An occluding application's pixels are not used as a fallback.

and

> `PrintWindow` relies on the application rendering its window. Hardware-accelerated, protected or otherwise unsupported surfaces may still return blank/incomplete content even when the API reports success.

The implementation is `[OCUWin32]::PrintWindow($hwnd, $hdc, 2)` (`runtime.ps1:1278`), guarded by a cloaked-window check (`DwmGetWindowAttribute(hwnd, 14, …)`, `:460-464`) and a post-capture bounds check (`Assert-CurrentWindowBounds`, `:1280`). The Go side throws away degenerate frames — under 64 px on a side or under 20,000 px² (`main.go:34-35`, `:179-186`) — and validates that the payload really is a PNG of usable size before returning it (`:188-203`).

The important consequence for us: **the unit of a screenshot is one window, chosen by name, not the desktop.** A "frame" the owner approves can therefore be one app's window, which is exactly the shape of the approval card in the amendment.

**The default action path does not move the real pointer.** This is the property that makes Phase C realistic, and it is easy to miss. From the server's own instruction string (`main.go:41`):

> Windows actions use UI Automation patterns first and fall back to window messages when an app does not expose the needed pattern. The Windows runtime does not auto-launch apps, perform SetFocus, or use UIA text fallback by default, so background-capable actions do not intentionally steal the user's foreground focus.

`docs/windows-input.md` is explicit about the switch: `click_method: global`, real-pointer scroll/drag and `key_method: global` are all **refused** unless the process environment carries `OPEN_COMPUTER_USE_ALLOW_GLOBAL_POINTER_FALLBACKS=1`. Retinue simply never sets it. Keyboard input by window message goes to the control that holds keyboard focus inside the target window (`GetGUIThreadInfo`), and key *combinations* are refused on that path because posted messages cannot carry modifier state.

**Two properties that need care.**

1. **`get_app_state` returns a screenshot by default.** `include_screenshot` defaults to `true` (`main.go:993`). A naive supervisor call therefore sends a picture of the owner's window on the first observation, which the amendment forbids without a per-frame approval. Retinue must send `include_screenshot: false` explicitly on every default call — the flag exists, but the default is against us, so the code must be written so that "no screenshot" is what happens when a field is missing.
2. **Each tool call costs a PowerShell start.** `runPowerShell` writes the request to a temporary script and runs `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File <script> <request>` (`main.go:702`), hiding the console window, with a 30-second bridge timeout (`docs/windows-targeting.md`: "bounds an unresponsive renderer to 30 seconds"). On this machine, a bare `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command exit` took **798 ms, 978 ms and 853 ms** over three runs (measured today, `Measure-Command`). Expect roughly a second of overhead per action before the app does anything — which is tolerable for a 5-minute grant, but means Phase C's 60-second idle rule must count *tool calls*, not wall-clock silence, or a slow app will expire the grant mid-task.

**No network.** I found no `net/http` import, no URL and no updater in the Windows runtime (`main.go`); the module is a bare `go.mod` with no dependencies (`apps/OpenComputerUseWindows/go.mod`: `go 1.22`, no `require` block). The npm *shim* is JavaScript and I did not audit it — which is another reason to take the `.exe` and not the npm package. Whether the published `.exe` is Authenticode-signed is **unverified**; I did not download it, and no signature is mentioned anywhere in the repository.

**The mirror.** `anomalyco/computer-use` has the same description as upstream, language Swift, 10 stars, last push 2026-11-11, and **no releases at all** (`https://api.github.com/repos/anomalyco/computer-use/releases` returns `[]`). So a pin cannot come from the mirror; it must come from `iFurySt/open-codex-computer-use`, which is also where `SHA256SUMS` is published.

One more thing worth recording, because it constrains how we talk to it: element indexes are **invalidated by each observation**. `include_ax: false` "skips AX collection and invalidates prior element indexes" (`main.go:992`), and the usage guide says actions re-read the tree before acting (`skills/open-computer-use/references/usage.md`). So Retinue's own tool contract needs an element handle with a generation number, or the bot will click stale indexes after an unrelated call.

### 1.4 `Windows-MCP`: bigger, and the size is the problem

`CursorTouch/Windows-MCP` is the most popular Windows computer-use MCP server (11,657 stars, MIT, v0.8.9 on 2026-11-12) and technically capable. It is not suitable for this path, for three reasons.

**The tool list includes code execution.** From its README (`README.md:856-888`), the tools are `Click`, `Type`, `Scroll`, `Move`, `Shortcut`, `Wait`, `WaitFor`, `ControlStatus`, `DisplayInventory`, `Screenshot`, `Snapshot`, `App`, **`PowerShell`**, **`FileSystem`**, `Scrape`, `MultiSelect`, `MultiEdit`, `Clipboard`, **`Process`**, `Notification`, **`Registry`**. A driver whose MCP surface includes `PowerShell` and `FileSystem` makes Retinue's permission categories decorative: the bot could write files and run commands through a tool the daemon classified as "computer use". It can be trimmed (`--tools "Screenshot,Click,Snapshot"`, `WINDOWS_MCP_TOOLS` at `README.md:791`), but the capabilities would still be compiled in and one config drift away.

**The password guard does not cover the value.** In `src/windows_mcp/tree/service.py:711-729`, the field is marked and then the value is read regardless:

```python
if isinstance(node,(EditControl,DocumentControl,ImageControl)):
    is_password = False
    try:
        is_password = bool(node.CachedIsPassword)
        if is_password:
            metadata['is_password']=True
    except Exception:
        pass

    try:
        value = node.GetCachedPropertyValue(PropertyId.LegacyIAccessibleValueProperty)
        metadata['value']=value.strip() if value else '(empty)'
        if not is_password and not is_dom:
            words=node.GetAllWordBoundingBoxes()
```

`metadata['value']` is assigned outside the `if not is_password` guard; only the word boxes and the selection are suppressed. Whether Windows returns real text for a password `Edit` through `LegacyIAccessibleValueProperty` is **unverified** here (I did not test it), but the code does not depend on the answer, and a masking rule that says "unless the app volunteers it" is not the rule the amendment asks for.

**Screenshots are whole-desktop by default.** `Screenshot` captures the desktop (with a brief orange-red border flash drawn over it, `README.md:876`; `WINDOWS_MCP_DISABLE_FLASH=1` suppresses it), or a `region`/`display` of it — there is no per-window capture. `Snapshot` does keep the image opt-in (`use_vision` defaults to `False`, `src/windows_mcp/tools/snapshot.py:40`), which is the right instinct, but the desktop-wide frame is the primitive.

It also installs itself as a **login scheduled task** by default when told to (`windows-mcp install`, creating `~/.windows-mcp/start-server.cmd`, `README.md:91-102`), requires **Python 3.14+**, and offers SSE/HTTP transports with an auth key (`README.md:841-848`) — a network listener is precisely what the amendment's never-remote rule would have to defend against.

Two ideas in it are worth copying, though, and I would put both in our design: its **ownership model** (`ControlStatus`, a visible edge and cursor glow while the AI has the desktop, physical mouse movement or `Ctrl+Alt+Shift+Backspace` returning control, `README.md:873`) is very close to the amendment's indicator-and-hotkey requirement, and it is honest that the takeover is "best-effort, not a security boundary". Its `WaitFor` (poll until text or an element appears, inside one tool call, `:872`) is a better primitive than a bare `Wait` and saves round trips.

### 1.5 `FlaUI-MCP`: the closest competitor, and the two defaults that decide it

`shanselman/FlaUI-MCP` (MIT, C#, 113 stars, **v0.2.1 released 2026-11-07**) is the closest thing to "Playwright for Win32", and it is a serious alternative. `windows_snapshot` returns the accessibility tree with `ref`-style ids (`w1e5`), `windows_click` takes a `ref`, `windows_batch` runs several actions in one call, and the pitch is "No screenshot parsing. No coordinate guessing." (`README.md:15-20`).

It is stronger than I expected on paper:

- **Twelve tools**: `windows_launch`, `windows_snapshot`, `windows_click`, `windows_type`, `windows_send_keys`, `windows_fill`, `windows_get_text`, `windows_screenshot`, `windows_list_windows`, `windows_focus`, `windows_close`, `windows_batch` (`README.md:101-114`). Window- and element-scoped, no shell and no file access — the same restraint that makes OCU usable.
- **It does take Windows screenshots**, and in the shape we would want: an element, a window, the foreground window, **a background window**, or the full screen (`src/FlaUI.Mcp/Tools/ScreenshotTool.cs:33-50`). The `background: true` path "uses native background capture when available and falls back to the normal screenshot path if Windows returns a blank frame" (`README.md:120-125`) — the same occluder problem OCU solves with `PrintWindow`, solved with a fallback.
- **A 30-second timeout per call**, explicitly to stop "a blocked UI Automation provider or modal dialog" hanging the server (`README.md:126-127`).
- **A self-contained .NET 8 ZIP**, "64-bit Windows, no .NET runtime required" (`README.md:60-67`), run as `FlaUI-MCP.exe` over stdio (`:77-79`).

Two things in its source decide against it for this path:

1. **It takes the owner's focus.** `windows_send_keys` calls `element.Focus()` and then `Thread.Sleep(50)` before sending (`src/FlaUI.Mcp/Tools/SendKeysTool.cs:203-204`), and `windows_click` "Prefers Invoke pattern for reliability, falls back to mouse click if needed" (`src/FlaUI.Mcp/Tools/ClickTool.cs:22-24`, fallback after `:73`). A mouse click is real input at wherever the pointer is; a `Focus()` call brings a window to the front and takes the caret away from whatever the owner was typing into. The amendment's whole design — a visible indicator, a stop button, "the bot must not surprise you" — assumes the bot works *without* taking the desktop, which is what OCU's window-message default gives and this does not.
2. **Nothing in its snapshot code mentions `IsPassword`.** `src/FlaUI.Mcp/Core/SnapshotBuilder.cs` (31 C# files in the repository) has no password handling I could find, and the README is silent. FlaUI exposes the property, so it could be added — but then we are writing the masking ourselves in C#, which is the cost this option was supposed to avoid.

It is the right **fallback**: if OCU's PowerShell bridge proves fragile on the owner's machine, this is a smaller, more static artifact with a better type system behind it, and the same `ref` addressing model.

### 1.6 `nuphus-mcp`: a Rust implementation that already exists

This one was not in the earlier report and matters because it is the "write our own" option, already written. `mrpulor-gh/nuphus-mcp` is MIT, Rust, 334 stars, **v0.3.4 (2026-11-08)**, distributed as prebuilt binaries (npm meta package puts `nuphus-mcp` on PATH, "No Rust toolchain needed", `README.md:246-248`; release assets `nuphus-mcp-win32-arm64.bin` and siblings).

It speaks JSON-RPC over stdio, "no daemon, no network service, one binary", and splits into `desktop-api` (xcap + Win32) and a semantic layer:

- Seven UIA tools: `desktop_targets_list`, `desktop_target_bind`, `desktop_semantic_observe`, `desktop_semantic_candidate`, `desktop_semantic_execute`, `desktop_semantic_action`, `desktop_verify_state` (`README.md:43-48`), described as "handles, coordinates and platform locators never cross the MCP boundary; the caller only sees opaque ids and stable semantic locators… Backed by Windows UIA and the macOS Accessibility (AX) API". That opaque-handle design is better than raw element indexes and is worth copying.
- Source layout confirms it is real, not a wrapper: `crates/desktop-api/src/semantic/windows_uia.rs`, `input/sendinput.rs`, `vision/capture.rs`, `vision/ocr.rs`.

Reasons it is not my recommendation for Phase C:

- **It ships a vision path that sends screenshots to a model by design.** `desktop_vision` is "BYOK — send a screenshot to your … vision model" (`README.md:50-52`), and `desktop_perceive` downloads ONNX OCR/YOLO models on first use (`README.md:228-233`). Under the amendment those calls would have to be blocked, not configured.
- **Its documentation teaches copying the owner's Chrome profile** (`README.md:204-213`: `copy "%LOCALAPPDATA%\Google\Chrome\User Data\Default" <dest>\Default`, with a note that DPAPI-encrypted cookies "decrypt fine from the copy under the same user"). That is exactly the pattern the amendment's "Retinue profile, no imported cookies" rule forbids, and a driver whose help text advises it is a driver whose defaults we would be fighting.
- Its platform table reads "Windows | Full | Full (Win32 API)" and "macOS | Full | Semantic (AX) + mouse/keyboard" (`README.md:89-93`), and the semantic tools are described as "Backed by Windows UIA and the macOS Accessibility (AX) API" (`README.md:48-49`). So UIA on Windows is claimed, but the project is young (v0.3.x) and its documentation leads with the pixel path.

It is, however, the best available **reference implementation** for section 2: the same crates, the same split between a semantic layer and a pixel layer, worth reading before writing ours.

### 1.7 What is not a candidate at all

There is no way to point a bot at Claude Code's, Codex's or OpenCode's own computer use. None of the three exposes desktop control to a third party: Claude Code's tools are file, shell and web; Codex's app-server schema in this repository has no desktop tool (`crates/retinue-adapter-codex/schema/` — the item types are commands, file changes, MCP calls, web search, images and reasoning); OpenCode's ACP surface is tools the harness defines. The earlier report reached the same conclusion (`docs/research/owner-requests-2026-11-14.md:395`), and nothing I read changes it.

---

## 2. Writing our own, in Rust

Retinue could skip the third-party binary and put the UI Automation client in the daemon. This section prices that option. **Every day count below is my estimate, not a measurement**; every crate fact is cited.

### 2.1 What each crate covers

| Crate | Version (crates.io, 14 Nov 2026) | Licence | Downloads | What it gives us |
|---|---|---|---|---|
| **`uiautomation`** (`leexgone/uiautomation-rs`) | **0.25.1**, updated 2026-09-04 | **Apache-2.0** | 1,104,484 | The whole UI Automation client: `UIAutomation`, `UIElement`, tree walkers, matchers, cache requests, events, and **31 pattern wrappers** — `UIInvokePattern`, `UIValuePattern` (with `set_value`), `UITogglePattern`, `UITextPattern`, `UITextEditPattern`, `UIScrollPattern`, `UISelectionPattern`, `UIExpandCollapsePattern`, `UILegacyIAccessiblePattern` and the rest (`crates/uiautomation/src/patterns.rs`). Also `get_clickable_point`, `get_bounding_rectangle`, `get_property_value(UIProperty)` and `get_cached_property_value`. Features: `control`, `pattern`, `event`, `input`, `screenshot` (BMP/PNG), `process`, `dialog`, `clipboard`, `log`, `all`. |
| **`windows`** (microsoft/windows-rs) | **0.62.2** | **MIT OR Apache-2.0** | 408,439,053 | Raw Win32 and WinRT when the wrapper is not enough: `Win32_UI_Accessibility` (the `IUIAutomation` COM interfaces), `Win32_System_Com` (`CoInitializeEx`, `CoCreateInstance`), `Win32_UI_Input_KeyboardAndMouse` (`SendInput`), `Win32_UI_WindowsAndMessaging` (`EnumWindows`, `PrintWindow`, `GetGUIThreadInfo`, `PostMessage`), `Win32_Graphics_Gdi` (`BitBlt`), `Win32_Graphics_Dwm` (`DwmGetWindowAttribute`), `Graphics_Capture` (Windows Graphics Capture). |
| **`enigo`** | **0.6.1**, 2025-08-28 | **MIT** | 4,095,250 | Keyboard and mouse simulation. On Windows it is `SendInput` with `INPUT`/`MOUSEINPUT`/`KEYBDINPUT` structs (`src/win/win_impl.rs:5-14`, `send_input` at `:74`). **This is real, global input**: it moves the system pointer and types into whatever has focus. It cannot deliver a window message, which is what `open-computer-use` uses to act without stealing focus. |
| **`xcap`** (`nashaofu/xcap`) | **0.9.10**, updated 2026-11-09 | **Apache-2.0** | 2,919,090 | Screen and **window** capture. On Windows it has two backends: Windows Graphics Capture (`src/windows/wgc.rs`, used when the `wgc` feature is on, `src/windows/capture.rs:29-45`, `:81-105`) and a GDI path when it is off (`:48-79`, `:107-…`). Handles DPI-awareness of the *capturing* process (`utils::get_process_is_dpi_awareness`) and window scale factor. |
| **`windows-capture`** | **2.0.1**, 2026-08-08 | **MIT** | 2,784,033 | Windows Graphics Capture only, frame-callback oriented (made for recording). Usable for one frame; `xcap` is the smaller fit for "grab this window now". |

Two things the crates do **not** remove, and they are the reason this option is bigger than it looks:

1. **COM apartment and blocking.** `UIAutomation::new()` calls `CoInitializeEx(None, COINIT_MULTITHREADED)` (`crates/uiautomation/src/lib.rs:117-122`), so a UI Automation client wants a dedicated **MTA** thread, not a tokio task. Cross-process UIA calls block and **can hang indefinitely** when the target app is busy: the entire `hung_window_*` test and history in `open-computer-use` (`apps/OpenComputerUseWindows/hung_window_integration_windows_test.go`, `docs/histories/2026-11/20261108-2240-windows-hung-window.md`) exists for that one failure, and its answer is a 30-second process timeout on the bridge. A Rust version needs the same: every call on a worker thread with a deadline, and a way to abandon a thread that will not return.
2. **DPI awareness of our own process.** The daemon crate embeds no manifest of its own and makes no DPI call: `crates/retinue-daemon/build.rs` only embeds the skill library (51 lines, no resource step), and nothing in `crates/` mentions `DpiAwareness`, `SetProcessDpi` or `PerMonitor`. So the process runs with whatever rustc's default manifest asks for — system awareness at best, not per-monitor v2 (**unverified** which). A DPI-unaware or merely system-aware process receives **virtualised** coordinates from `GetWindowRect` and `BoundingRectangle`, so a click computed from the tree lands in the wrong place on a 125%, 150% or 200% display, and a screenshot is the wrong size. `open-computer-use` is precise about this because it hit it (`docs/windows-targeting.md`: "screenshot coordinates remain physical pixels relative to `GetWindowRect`, including at 200% display scaling"; and `docs/windows-input.md`: "DPI bridge uses physical pixels"). We would have to call `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` before any window is touched, and test at 200%.

### 2.2 What it would take

Estimates for one developer who knows Win32 but not this codebase. I have deliberately not tried to be precise; the point is the order of magnitude and which items are the expensive ones.

| Work item | Estimate | Why it costs that |
|---|---|---|
| Window enumeration, app-name resolution, real frame bounds (`DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`) | 2–3 days | Name resolution across processes (`QueryFullProcessImageName`, Start-menu names), UWP/`ApplicationFrameHost` hosting, per-monitor DPI. |
| UIA tree: walker, cache request, node/depth/text budgets, stable ids per generation | 4–6 days | Chromium/Electron trees need accessibility enabled and are enormous; the budgets (1,200 nodes / 64 levels / 500 chars, `skills/open-computer-use/references/usage.md:149`, `:166`) exist because unbounded trees blow both the context window and the latency budget. Our own id scheme has to survive re-reads. |
| Masking: `IsPassword` fail-closed, value, selection, text patterns, region mask for frames | 1–2 days | Small code, but it has to be exhaustive, and "unknown is a password" is a deliberate asymmetry to write down and test. |
| Actions: Invoke / Toggle / Value / Selection / Scroll patterns, click by element, window-message fallback | 4–6 days | Each pattern fails differently per toolkit; owner-drawn controls ignore posted input (`docs/windows-targeting.md`, "Limits"); key combinations cannot travel as window messages (`docs/windows-input.md`). |
| `SendInput` path for apps that need real input, plus UIPI integrity check before every send | 2–3 days | Higher-integrity targets silently drop input; `SendInput` does not report that it was blocked, so the check must come first (`docs/windows-input.md`). |
| Screenshot: per-window capture, degenerate-frame guard, DPI, occluder behaviour | 2–4 days | `PrintWindow` returns blank for many GPU-accelerated windows; WGC cannot capture protected content; the two disagree on which windows work. |
| COM worker thread, deadlines, cancellation | 2–3 days | See above. |
| Tests with a real desktop: fixture windows, an integration suite, a 200% DPI pass | 3–5 days | `open-computer-use` needs a dozen PowerShell fixture scripts plus Go integration tests for the same coverage (`scripts/tests/fixtures/*.ps1`, `apps/OpenComputerUseWindows/*_integration_windows_test.go`). |
| **Total, first usable cut** | **20–32 days** | Plus per-app quirks, which is where the unbounded time is. |

For comparison, the whole of Phase C's *gate* — consent cards, grants with the 5/15/60-second limits, the deny-list, the indicator, the content-free activity rows, the tier rows — is needed either way, and is not in that table. Writing our own driver adds the table on top.

### 2.3 Where it would live, and the shape I would give it

- **A new crate, `crates/retinue-owner-computer`**, not code inside `retinue-daemon`. The daemon is already the largest crate and it is the one that must never hang; a COM layer that can block belongs in its own crate with its own tests, so a compile error or a test failure in the driver does not touch the gate.
- **Run it as its own process anyway.** Even for our own code, a child process is the right boundary, because a hung UIA call can only be abandoned by killing something. The daemon can then keep a deadline and restart the child, exactly as the amendment's "supervised child" bullet describes, without pretending that a leaked thread inside the daemon is fine.
- **Same tool contract as the third-party option.** `owner_screen_state`, `owner_click`, `owner_type`, `owner_key`, `owner_scroll`, `owner_screenshot` (the earlier report's list, `docs/research/owner-requests-2026-11-14.md:399`), with the driver behind them swappable. If the contract is right, replacing the driver changes no prompt, no card and no test in the UI.
- **Dependencies today.** The daemon already links `windows-sys 0.61.2` (`crates/retinue-daemon/Cargo.toml:57`) and the shell links `windows 0.62.2` (`apps/desktop/src-tauri/Cargo.toml:50`), so a `windows` dependency in a new crate is not new to the build graph — but the feature list would be much larger (`Win32_UI_Accessibility`, `Win32_System_Com`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_Graphics_Gdi`, `Win32_Graphics_Dwm`), and the `uiautomation` crate is a second, separate COM wrapper. Expect the first clean build of that combination to be slow; expect no runtime cost.

### 2.4 The honest summary of this option

Writing our own is the right **long-term** home: no third-party binary in the trust path, no PowerShell in the middle, one process model, and total control of masking and budgets. It is not the right **first** cut, because roughly 80% of the effort goes into problems that `open-computer-use` has already met and documented — hung windows, Chromium trees, DPI, UIPI, occluders, blank captures — and every one of those is invisible in a demo and obvious on the owner's real desktop.

---

## 3. Recommendation for Phase C's first cut

### 3.1 The recommendation

**Use `open-computer-use` v1.1.4 (MIT) as Phase C's driver, pinned by SHA-256, run as a supervised stdio child, behind Retinue's own `owner_*` tools, with screenshots refused outright in the first cut.** Read the owner's screen as a text tree only; ship the pointer and keyboard on the accessibility path; leave `owner_screenshot` unimplemented until the frame-approval card exists.

The reasoning, in the order that decided it:

1. **Its defaults are already the amendment's shape.** App-scoped, no shell, no file access, no registry, no process control, and **no real pointer movement unless an environment variable is set** — so the single most dangerous property of a computer-use driver (it moves the owner's mouse) is off unless Retinue turns it on. The alternatives each fail one of these by default: `Windows-MCP` puts `PowerShell` and `FileSystem` beside the mouse; `FlaUI-MCP` focuses windows and falls back to real mouse clicks (`src/FlaUI.Mcp/Tools/SendKeysTool.cs:203`, `Tools/ClickTool.cs:22-24`); `nuphus-mcp` ships a vision tool that sends frames to a model by design.
2. **It masks passwords fail-closed**, which is the property I would otherwise have to write and prove myself. "Unknown counts as a password" (`runtime.ps1:1044-1046`) is the only version of that rule that survives a hostile app.
3. **Its observations are bounded and cheap.** 1,200 nodes / 64 levels / 500 characters is a default we would have chosen anyway, and text is reviewable in a way a picture is not.
4. **It is one static 2.4 MB `.exe` with no runtime dependency** beyond `powershell.exe`, so the installed app gains a binary, not a Python or .NET runtime.
5. **It is the one option whose work I can reuse later.** If we write our own in Rust (section 2), the tool contract, the gate, the cards and the tests carry over unchanged; only the process behind them is replaced.

### 3.2 What gets pinned, and where it is recorded

The amendment requires the licence and pinned version to be in `docs/operations/`. The artifact to record:

- **Upstream**: `iFurySt/open-codex-computer-use`, release `v1.1.4` (published 2026-11-07T06:57:33Z), asset **`open-computer-use-cli-1.1.4-windows-amd64.zip`**, with `SHA256SUMS` from the same release as the cross-check. The extracted `open-computer-use.exe` is 2,467,328 bytes **in the npm copy** of the same version; the zip's own figure must be taken at fetch time and recorded then.
- **Record**: version, the SHA-256 of the zip *and* of the extracted exe, the licence (`MIT License, Copyright (c) 2026 Leo`, `LICENSE` in the repository), the review date, and the reviewer's name.
- **Do not** pin from `anomalyco/computer-use`: it has **no releases** (`https://api.github.com/repos/anomalyco/computer-use/releases` → `[]`), so it cannot be a source of a hash, and it lags upstream (last push 2026-11-11 vs 2026-11-13).
- **Do not** take the npm package as the artifact: it is a Node shim plus four platform binaries and 39 MB of macOS app resources, and it would put Node on the owner's machine for no reason.

Proposed file: `docs/operations/owner-computer-driver.md`, with the pin table and a one-line "how to upgrade" procedure (download, verify hash, run the smoke list in §3.7, update the table, bump the constant in the daemon).

### 3.3 How it sits in Retinue

The daemon already has the exact pattern this needs, and it is worth naming because it decides the architecture: Retinue does not hand an MCP server to the *agent*; it hands the agent a **second copy of its own executable** which proxies back to the daemon over loopback with the conversation's capability.

- `computer::mcp_server_for` builds an `McpServerSpec { name, command: current_exe(), args: ["computer-server", "--endpoint", <file>], env }`, writing an endpoint file first, restricted to the user (`crates/retinue-daemon/src/computer.rs:971-987`, endpoint writing at `:928-951`, `apply_key_permissions` at `:949`).
- `computer::mcp_rpc` serves that child, and every Harbor call is made in the daemon (`crates/retinue-daemon/src/computer.rs:904-923`) — the child carries no token and reaches nothing itself.
- `McpServerSpec` is `crates/retinue-agent-core/src/lib.rs:882`.

So Phase C adds a **`retinued owner-server --endpoint <file>`** child with the same three properties: it names `owner_*` tools, it proxies `mcp.owner.call` back to the daemon, and the daemon owns the grant, the card, the indicator and the activity row. The OCU `.exe` is then a **child of that child**, or of the daemon directly — either is acceptable as long as only the daemon decides whether a call proceeds. I would spawn it from the daemon and give the `owner-server` child nothing but the RPC, so there is exactly one long-lived OCU process per live grant and it dies with the grant.

The tools, and what each becomes:

| Retinue tool | OCU call | Forced arguments | Gate |
|---|---|---|---|
| `owner_apps` | `list_apps` | — | Grant live. Returns app names only; drops usage frequency. |
| `owner_screen_state {app}` | `get_app_state` | **`include_screenshot: false`**, `text_limit` ≤ 500, `max_tree_nodes` ≤ 1200, `max_tree_depth` ≤ 64 | Grant live, app not on the deny-list, attended. Returns text only in the first cut. |
| `owner_click {app, element}` | `click` | `element_index` **required**; `click_method` ∈ `auto`/`accessibility`/`app_post` (never `global`) | Grant live; element must belong to the latest observation generation. |
| `owner_type {app, element, text}` | `set_value` | Verifies read-back by itself (`main.go:1039`) | Grant live; text length recorded, never the text. |
| `owner_key {app, key}` | `press_key` | `key_method: auto`; **combinations refused** (we refuse them, not only OCU) | Grant live. |
| `owner_scroll {app, element, direction}` | `scroll` | `element_index` required | Grant live. |
| `owner_stop` | — | Kills the driver process, releases the grant, cancels the turn | Always available. |
| `owner_screenshot` | **not in the first cut** | — | Second cut, behind the frame-approval card (§3.5). |

`drag` and `perform_secondary_action` stay unexposed until something needs them: every tool we do not expose is a tool the model cannot invent a use for.

### 3.4 What Retinue must force rather than trust

These are the places where the driver's behaviour is close to right but not right enough, and where a test, not a code review, has to hold the line.

1. **`include_screenshot: false` on every default call.** The default is `true` (`main.go:993`). Retinue's serialiser must write the field explicitly, and a unit test must assert that no outgoing `get_app_state` payload can omit it. Without this, the very first observation sends a picture of the owner's window and the amendment is broken before any card can appear.
2. **`OPEN_COMPUTER_USE_ALLOW_GLOBAL_POINTER_FALLBACKS` is never set.** The child's environment is built by us (`McpServerSpec.env` is a `BTreeMap` we fill, `crates/retinue-daemon/src/computer.rs:981-986`), so the safe value is the default one: an empty map. A test should assert the variable is absent even when the owner's own environment has it set, because a child inherits the environment by default.
3. **Element handles carry a generation.** Any new observation invalidates prior indexes (`main.go:992`, and `skills/open-computer-use/references/usage.md`). A stale handle must be refused with a clear message ("the screen changed; look again"), not retried against the new tree, or a click can land on a different control than the one the owner approved.
4. **The deny-list runs before the call, not after.** Matching on app name and window title, with the starter list from the earlier report (bank, password manager, mail, SSH terminal, `docs/research/owner-requests-2026-11-14.md:419`). The refusal is shown in the conversation so the owner learns why.
5. **`type_text` is not exposed in the first cut, and `set_value` is.** `set_value` verifies its own write; `type_text` writes into "the control that has keyboard focus" inside the window, which is the wrong place if focus moved between the observation and the action. Where a field does not support `ValuePattern`, the honest answer is a refusal, not a blind keystroke.
6. **A hard deadline per call, and a kill that works.** OCU bounds a hung bridge at 30 seconds (`docs/windows-targeting.md`); Retinue's own deadline should be shorter (10 s for an observation, 15 s for an action) and stopping must kill the process tree, not ask it.
7. **The grant ends when the tree goes blind.** A locked workstation, a UAC secure desktop or a switch to another session makes UIA fail. That failure is the signal to end the grant and say so, and it happens to implement the amendment's lock rule (`AGENTS.md`, bullet "A live grant is visible and stoppable") without extra code.
8. **One content-free activity row per call**, in the shape `remote_activity` already has (`crates/retinue-storage/src/lib.rs:850`): the app name, the action class, and a count for typed text. Never the tree, never the text, never a frame.
9. **Tiers.** Every `owner_*` method gets a `tiers.json` row class `never` and every event `drop`, the same as the computer methods (`apps/desktop/src/remote/tiers.json:348-365`, `docs/ipc-contract.md:767`). A parity test already fails the build when a method has no row, which is what will catch a forgotten one.

### 3.5 The screenshot path, when it comes

The second cut is small if the first cut is right, because OCU already produces exactly one image, for exactly one window, on request. What changes:

- `owner_screenshot {app}` runs one `get_app_state` with **`include_ax: false` and `include_screenshot: true`** and takes the single `image/png` content item. Turning the tree off in that call is deliberate: the call is a capture, not an observation, so no tree text is read out of it, and the frame cannot be confused with an observation. The cost is that it invalidates the element handles from the previous observation (`main.go:992`), which is correct — after a capture the bot must look again before it clicks. (The two flags may not both be false: `main.go:299-301` refuses that, so this is the only clean combination.)
- The image is **held locally** and shown to the owner as a card: "Send this frame? [Send] [Retake] [Never for this app]". Nothing goes to a provider until "Send".
- On "Send", the bytes become one `generated_media` attachment on the tool row, following the generated-image path (`crates/retinue-daemon/src/media.rs`, `meta` carries no path — the rule the amendment repeats), and the model receives that one image. The approval is **consumed**: the next frame needs a new approval.
- "Never for this app" adds the app to the deny-list, permanently, from inside the card.
- Password regions in the frame are masked before the owner even sees the card. `PrintWindow` captures what the window renders, so the mask has to come from the tree: for every element with `IsPassword` true, black out its rectangle in a copy of the image. That is the one place where "trust the driver" is not enough, and it is the reason mask rectangles should already be carried in the first cut's observation (cheap, unused, and testable there).

### 3.6 Risks, and what each one costs

| Risk | Likelihood | What it looks like | Mitigation |
|---|---|---|---|
| **A third-party binary in the trust path** | Certain (it is the choice) | A 2.4 MB Go binary and a 1,865-line PowerShell script that can read the screen and press keys | Pin by hash, review `runtime.ps1` once and record the review, download only from the pinned release, never resolve through PATH, record the pin in `docs/operations/` |
| **PowerShell as the bridge** | Certain | A `powershell.exe -ExecutionPolicy Bypass -File <temp>` per call (`main.go:702`) — a script host inside our process tree | Never expose a tool that takes a script path; never pass the owner's arguments into the script; treat the temp folder as ours |
| **A default turns a screenshot on** | High if written quickly | The first observation sends a picture of the owner's screen to a provider | Force the field, and assert on the outgoing payload (§3.4.1) |
| **Wrong-window input** | Medium | Text lands in another app because focus moved | No `type_text`; `set_value` only; element handles tied to a generation |
| **The tree carries secrets** | Medium | A mail body, a token visible in a terminal, a customer list | 500-character default, deny-list, never persist a tree, never log it |
| **Per-call latency ~1 s** | Certain | Ten actions ≈ ten seconds of shell start before the app reacts | Count idle time in calls, not wall-clock; batch where OCU allows it; say in the card that a step can take a second |
| **Upstream churn** | High | Ten Windows changes in November 2026 alone (`docs/histories/2026-11/`), including a click-targeting rewrite between v1.1.3 and v1.1.4 | Pin, read the diff before every bump, keep the smoke list in §3.7 as the acceptance test |
| **Elevation** | Certain | Windows refuses input to higher-integrity windows (UIPI), by design | Say so in the Settings page and in the refusal: "This window runs as administrator, so Retinue cannot drive it." |
| **Locked or switched session** | Medium | UIA fails or the grant hangs | Treat as end-of-grant; the amendment already requires it |
| **Single-maintainer project** | Medium | The upstream stops, or the licence changes | MIT code we have read, and a tool contract that a Rust rewrite can satisfy (section 2) |

One risk that is *not* on that list, and worth saying plainly: **the model seeing the tree is already the egress.** The 500-character text limit bounds the size of what leaves, not whether secrets are in it. A deny-list and a human watching the conversation are the only two controls that actually work, which is why the amendment makes the conversation attended.

### 3.7 What I would check before committing to it

Five checks on the owner's own machine, with the pinned `.exe` and `get_app_state` called by hand, before any Retinue code is written. If two or more fail, the recommendation changes to "write our own" (section 2) and Phase C slips.

1. **Notepad**: does `get_app_state` return a usable tree with element indexes? (Baseline: Win32, should be trivial.)
2. **Chrome**: is the page tree present, or does Chromium report only its frame? Electron and Chromium trees depend on the app enabling accessibility. If Chrome's tree is empty, the most useful half of the feature is gone and the browser path of Phase B becomes mandatory first.
3. **File Explorer and Settings**: two places where users actually work, and both mix UWP and Win32 controls.
4. **A password field** (any sign-in form): does the returned text show the field and never its value?
5. **A second monitor at 150%**: does a click at a tree element land on that element? This is the DPI check, and it is the one most likely to fail.

Plus one measurement: how long a `get_app_state` on Chrome takes end to end, because that number decides whether a 5-minute grant is enough for a real task.

### 3.8 What would make me change this recommendation

- **Chrome's tree comes back empty** (check 2): switch Phase C's order and do the browser first, because then the useful work is browser work, not desktop work.
- **A security review of `runtime.ps1` finds a way to reach beyond the app-scoped contract**: stop, fix upstream or write our own; do not ship it.
- **Upstream is archived or quiet for a month**: it is a young, fast-moving project, and a driver nobody maintains is a liability with screen access. Trigger the Rust option.
- **The owner's real desktop has an app that hangs the bridge constantly**: the 30-second timeout becomes a 30-second stall in a 5-minute grant. Our own process, with our own deadline, would handle it better.
- **A licence change**: the pin is MIT today; a move to a source-available licence ends the option immediately, and the pin becomes a reason not to upgrade rather than a reason to stay.

---

## 4. What I could not verify

Grouped so the Director can decide which ones are worth a test before Phase C starts.

**About `open-computer-use` (the recommendation).**

1. **Code signing.** Whether the published `open-computer-use.exe` is Authenticode-signed. I did not download the binary, and no signing is mentioned in the files I read (`README.md`, `LICENSE`, the release asset names, `main.go`). This decides whether the pin is "hash + version" or "hash + version + publisher", so it should be checked the moment the artifact is fetched.
2. **Which release contains the click-targeting fix.** `docs/windows-targeting.md` describes the current behaviour and compares against "a v1.1.3 binary", which implies v1.1.4 carries it, but I found no changelog stating it. If the fix is only on `main`, the pinned release behaves worse in covered windows and behind other apps than this report assumes.
3. **Real latency.** I measured only a bare `powershell.exe` start (798 / 978 / 853 ms over three runs on this machine). A full `get_app_state` on Chrome will be more: shell start, UIA walk, PNG encode. Not measured.
4. **Whether Chrome's page tree appears at all.** This is the single most consequential unknown, and it is check 2 of §3.7. Chromium exposes its tree only when accessibility is active; OCU's own tests include a "Chromium selection" case (`docs/histories/2026-11/20261109-2310-windows-chromium-selection.md`), which suggests it works at least sometimes, but I did not run it.
5. **The zip and the npm copy are not proven identical.** The 2,467,328-byte figure comes from the npm tarball listing (`https://unpkg.com/open-computer-use@1.1.4/?meta`); the GitHub zip's contents and `SHA256SUMS` were not downloaded, so the pin must be taken from the zip at fetch time.
6. **`list_apps`'s 14-day history.** Its description says it returns apps "used in the last 14 days, including details on usage frequency" (`main.go:1002`). Where that data comes from (Start-menu tracking, recent items, a Windows API) is not stated in the code I read. It stays local, but it is a read of the owner's habits and worth knowing before we offer the tool.

**About the alternatives.**

7. **`Windows-MCP`'s password value.** I read the code path (`src/windows_mcp/tree/service.py:711-729`), not its output. Whether `LegacyIAccessibleValueProperty` returns real text for a password `Edit` is app-dependent; the finding is that the code does not guard it, not that it leaks.
8. **`FlaUI-MCP` and password fields.** `src/FlaUI.Mcp/Core/SnapshotBuilder.cs` has no password handling I could find, and the README says nothing; but I read one file, not the renderer it calls. Whether a password `Edit` appears with its value in its tree is therefore **unverified** — the finding is the absence of any masking code, not a demonstration of a leak.
9. **`nuphus-mcp` and password fields, and its Windows semantic parity.** I read its README, its workspace `Cargo.toml` and its file layout, not its UIA implementation. Whether `desktop_semantic_*` masks `IsPassword`, and whether the Windows semantic path has the same features as the macOS one, are both open.
10. **The three options I set aside on platform grounds** (Anthropic's `computer-use-demo`, Bytebot, UI-TARS-desktop): their platform support is stated from their own documentation as I know it, not re-fetched today.
11. **`enigo`** is at 0.6.1 on crates.io (2025-08-28) while its repository was pushed 2026-11-09, so unreleased work exists. I read the Windows backend on `main`, not necessarily the 0.6.1 release.

**About our own option.**

12. **Which DPI awareness the daemon process actually has.** No manifest, no call in `crates/` (see §2.1.2). What rustc's default manifest requests is not something I checked on a built binary.
13. **The effort estimate.** 20–32 days is a judgement, built from the work items in `open-computer-use` that already exist (1,865 lines of PowerShell, a dozen fixture scripts, a dozen integration tests). It is not measured.

**About the amendment's own wording.**

14. The amendment says "a release that changes these defaults needs the owner's live check before it ships" (`AGENTS.md`, "Release gate"). Nothing in that sentence says *which* defaults, so Phase C's Settings switch and its deny-list both count, and the owner should confirm the reading before a beta ships with the feature on by default (it must not be).

---

## 5. Proposed skill updates (not applied)

This run changed one file and no code, so I did not edit `.claude/skills/retinue-development/`. For the Director:

1. **`references/lessons.md`** — a dated entry: *the Windows computer-use driver default is the dangerous thing.* `open-computer-use`'s `get_app_state` returns a screenshot unless `include_screenshot: false` is sent (`apps/OpenComputerUseWindows/main.go:993`), and its real-pointer mode is gated by an environment variable the child inherits. When Phase C is built, the test that matters is an assertion on the outgoing payload and on the child's environment, not a code review.
2. **`references/codebase-map.md`** — a daemon MCP server is always a second copy of the daemon's own executable plus an endpoint file: `computer::mcp_server_for` (`crates/retinue-daemon/src/computer.rs:971-987`), `mcp_rpc` (`:904-923`), `McpServerSpec` (`crates/retinue-agent-core/src/lib.rs:882`). Any new `owner_*` surface copies this shape, and the child carries no token.
3. **`references/verification.md`** — the daemon's `build.rs` embeds only the skill library (`crates/retinue-daemon/build.rs`), so native Windows behaviour that depends on a manifest or DPI awareness (anything computing screen coordinates) must set it explicitly and be checked at 150% and 200% scaling.
4. **`docs/operations/owner-computer-driver.md`** — new file, not a skill update, but the amendment requires it: the pin table from §3.2 and the smoke list from §3.7.
