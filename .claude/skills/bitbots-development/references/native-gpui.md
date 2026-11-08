# Native GPUI app (`apps/native`)

The Rust port of Bitbots. Slice 1 (8 November 2026) was an installable core: the
window opens, starts its own `bitbotsd`, shows the projects tree, the
conversations column and one conversation, and streams replies. Slice 2 (8
November 2026) adds the look, the team and new projects: Haze portraits and
logos, the working shimmer, the marquee and the folding motion, the light theme
and density, the Team page with hiring, new projects with the CEO staffing
choice, hire and bot proposal cards, a first Settings page, daemon restart with
backoff, and `scripts/install.ps1`. Slice 3 (8 November 2026) makes it a working
day: the shared menu/select/toast primitives, the full composer (model, effort
and permission controls, `/commands`, the queue, pasted and dropped images,
history), approvals, questions and every proposal kind, message actions and rich
markdown, conversation rename/pin/archive with branch lines, the sidebar search,
the quick switcher and its shortcuts, the bot wizard and settings with archived
bots, the plan-usage line, "Needs you" and Windows notifications. Later slices
add Tasks and schedules, diffs, the terminal, the companion and dictation, and
the rest of Settings.
Lane: Gogo; the Director reviews, runs the gates and commits.

## Pinning GPUI

- `gpui` is a **git dependency on Zed**, pinned by `rev`. The version number in
  Zed's `crates/gpui/Cargo.toml` (`0.2.2`) is stale and means nothing: two
  commits a year apart both say 0.2.2. Pin the commit and write down why.
- **Choose a commit whose `rust-toolchain.toml` matches the machine.** `main`
  asks for 1.98.1 while the machine's `rustc` is 1.94.0; a git dependency ignores
  the repository's toolchain file, so the crate either builds on the local
  compiler or fails with syntax errors that look nothing like a version problem.
  Slice 1 pins `e2f3d5e…` (15 February 2026, toolchain 1.93). Check the pin with
  `https://raw.githubusercontent.com/zed-industries/zed/<rev>/rust-toolchain.toml`
  before a big build.
- Cargo clones the whole Zed repository into `~/.cargo/git/db`. It is large; do it
  once, early, in the background.
- **Do not adopt `gpui-component` casually.** From 0.9.0 it depends on
  `gpui-base`/`gpui-pre`, tree-sitter, resvg and LMDB, and it must match your
  GPUI pin exactly. Slice 1 writes its own text field and markdown subset instead.
- Read the API from the checkout, not from memory or docs.rs:
  `~/.cargo/git/checkouts/zed-*/<short-rev>/crates/gpui/src/`. `raw.githubusercontent.com`
  serves any file at the pinned rev, which is faster than a full checkout when you
  only need `examples/input.rs` or `src/elements/list.rs`.

## Patterns that work

**Entity and context.** `cx.new(|cx| State { .. })` makes an entity; `Context<T>`
derefs to `App`. Render with `impl Render for T { fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) }`.
A view that others own is `Entity<T>` and clones into a child slot:
`.child(self.composer.clone())`.

**Async tasks must not hold the UI.** `cx.spawn(async move |this, cx: &mut AsyncApp| { .. })`
takes the weak entity **and** an `AsyncApp`; `App::spawn` takes only `&mut AsyncApp`,
so the two are easy to confuse. Inside, `this.upgrade()` then
`root.update(app, |root, cx| ..)`. Never block the foreground with file or process
work: the daemon lives on its own `std::thread` with a `current_thread` tokio
runtime, and the window only reads an `async_channel` receiver. `async_channel`
is executor-agnostic, so it works with GPUI's executor without a bridge.

**Lists.** `gpui::list(ListState::new(count, ListAlignment::Bottom, overdraw), |index, window, cx| ..)`
gives variable-height rows and stick-to-bottom, which is what a streaming
conversation wants. `ListSizingBehavior::Auto` (the default) means the list does
**not** size itself: give it `.flex_1().h_full()` inside a flex parent, or it
collapses to nothing. Keep the row data in an `Rc<Vec<Row>>` moved into the
closure (`FnMut(usize, ..) -> AnyElement` cannot borrow `self`).

**Text input and IME.** Copy `crates/gpui/examples/input.rs` at the pinned rev:
an `EntityInputHandler` entity plus an `Element` that calls
`window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx)`
in `paint`. Windows sends composition to that handler, so Japanese and Chinese
input arrive as marked text. Keys go through `actions!` + `cx.bind_keys` with a
`key_context("Composer")`; Enter is *not* text on Windows, so bind
`shift-enter` to "insert newline" and handle plain Enter yourself.

**Text styling.** `div().text_size(..).text_color(..).font_family(..)` sets the
inherited `TextStyle`; `StyledText::new(text).with_highlights([(range, HighlightStyle)])`
renders one shaped, wrapping paragraph with bold/italic/code ranges. That is the
cheap way to do inline markdown, and selection and wrapping keep working.
`.truncate()` ellipsises; `.whitespace_nowrap()` keeps a line together.

**Colours.** `rgb(0xRRGGBB)` and `rgba(0xRRGGBBAA)` are plain fns, so a token
struct cannot be `const`; build it with a function (`theme::haze_dark()`) and copy
the CSS alpha into the last byte. `Rgba` and `Hsla` are different types: `.bg()`
wants `Rgba` (`gpui::rgba(0x00000000)` for "no fill"), text runs want `Hsla`
(`.into()`).

## Windows specifics

- Build output goes on `C:` (`CARGO_TARGET_DIR=C:\bbn\target`). The checkout path
  contains an apostrophe; the target dir usually saves you from it.
- `WindowOptions { window_background: WindowBackgroundAppearance::MicaAltBackdrop, window_min_size: Some(size(..)), titlebar: Some(TitlebarOptions { title, appears_transparent: false, traffic_light_position: None }), .. }`.
  Mica needs Windows 11; paint the documented 80% Haze tint yourself so the window
  is readable when Windows draws nothing.
- `std::os::windows::process::CommandExt::creation_flags(0x0800_0000)` is
  `CREATE_NO_WINDOW`, so the daemon child never flashes a console.
- Windows' "Show animations" is `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, ..)`
  (`windows-sys`, feature `Win32_UI_WindowsAndMessaging`). GPUI has no
  reduced-motion query at the pinned rev, so read it yourself, cache it in an
  `AtomicBool`, and refresh it when the window appearance changes.

## Images, SVG and build-time art (slice 2)

- `svg()` draws one **tint** only (`render_alpha_mask`) and loads its file through
  the app's `AssetSource`, which this window does not install. For a one-colour
  drawing from bytes in the binary, paint it yourself:
  `window.paint_svg(bounds, name.into(), Some(bytes), TransformationMatrix::default(), color, cx)`
  inside a small custom `Element` (`src/art.rs` `TintSvg`). That is how the Lucide
  icons and the tinted agent logos are drawn.
- `img(Arc<Image>)` renders a PNG in colour. `Image::from_bytes(ImageFormat::Svg, ..)`
  exists but its `to_image_data` path skips the BGRA swap the `img` asset loader
  does, so full-colour SVGs are rasterised to PNG at **build time** instead:
  `build.rs` renders with `resvg` (already in the lock through gpui) and encodes
  with `image`, writes `<OUT_DIR>/art_assets.rs`, and `include!`s it. Mind the
  alpha: `tiny_skia` pixmaps are premultiplied, so demultiply before encoding.
- Decode each image once, into a `LazyLock<BTreeMap<&str, Arc<Image>>>`; `img()`
  per frame would decode 30 portraits again every 16 ms.

## Animation, appearance and dialogs (slice 2)

- `element.with_animation(id, Animation::new(dur).repeat().with_easing(..), |el, delta| ..)`
  drives the frames. `AnimationElement` has **no `.child`**: build the div with its
  children first, then animate it, or use `.map_element(..)`.
- A one-shot with a stable id runs once and stays at the end; to replay a close
  animation, render for `dur` with a *different* id (`{key}:close`) and reverse the
  delta. Keep the row mounted for that long with a timer, then drop it.
- There is no `rotation` on `Div` and no gradient-to-text at this rev: fold
  chevrons are two icons, and the working shimmer is a soft band of the text
  colour slid across the word inside an `overflow_hidden` box.
- `window.observe_window_appearance(..)` returns a `Subscription` that must be
  kept alive, and it fires **immediately** on registration, so register it outside
  `render` (the open-window closure is the safe place) and hand the result to the
  root view.
- `cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, .. })`
  works on Windows (IFileOpenDialog) and returns a oneshot receiver, so await it in
  a `cx.spawn`.
- Dialogs are an overlay: `div().absolute().top_0().left_0().size_full()` inside a
  `relative()` root view. `overflow_y_scroll` is on `StatefulInteractiveElement`,
  so the scrolling div needs an `.id(..)` first.

## Restarting the daemon (slice 2)

`worker.rs` runs one session at a time: start `bitbotsd`, snapshot, follow events,
serve commands. `serve_commands` is a `tokio::select!` over the command channel and
a 400 ms `child.try_wait()` poll; an exit it did not ask for returns `Died`, and
the outer loop restarts after 1, 2, 5 and 10 s, then stops and offers Retry. A
clean `Command::Shutdown` must return `Stopped`, or closing the window restarts
the daemon.

**The bootstrap line can take 20 seconds.** Opening a copy of an 804 MB installed
database re-creates its read indexes, and the daemon answered after 20 s where a
fresh database answers in 2. `supervisor::BOOTSTRAP_TIMEOUT` is 45 s for that
reason; a 10 s limit made every start on the owner's data look like "the daemon
did not start", five times in a row, and the Retry button then never helped.

## Shared pieces (slice 3)

**Menus and popovers.** `anchored()` and `deferred()` do exist at the pinned rev
(`crates/gpui/src/elements/`), which the slice 1 notes' "no popover API" would
hide. One menu at a time lives on `Root` (`src/ui/menu.rs`): a panel in window
coordinates (`anchored().position(at).snap_to_window_with_margin(px(8.0))`), a
transparent full-window backdrop under it that closes on any click, and
`deferred(panel).with_priority(20)` so it paints over every column and over a
`list`. The click position comes from `ClickEvent::position()` — the same call
serves a "…" button and a right-click, and there is no way to ask GPUI where a
trigger element is. Escape closes the menu through the window's own
`on_key_down`, because a menu is not focused.

**Pressed state.** `Div` has `.active(..)` (a `StyleRefinement`), so a button or
row can darken while the mouse is down without tracking mouse-down state.

**Focus.** `TextInput` takes three hooks: `set_on_submit`, `set_on_key` (returns
`true` when it used the key, so Enter does not also send) and `set_on_change`
(called after any edit, so a field that filters a tree can ask for a redraw).
A `/command` menu's arrows, Enter and Escape, the Up-arrow history and Ctrl+V for
a picture all live in that key hook. `window.is_window_active()` is how the
window knows it does not have the person's attention (notifications).

**Clipboard images.** `cx.read_from_clipboard()` returns `ClipboardEntry::Image`
with the bytes Windows holds: a PNG when the source app put one there, and a
**BMP** when it only offered a DIB (GPUI's Windows reader wraps the DIB in a BMP
header). `src-tauri` avoids this by reading through `arboard`; here the bytes are
re-encoded with the `image` crate (`files::clipboard_png`), which is a runtime
dependency now. Do not add `arboard`: its `clipboard-win 5.4.1` pin is yanked and
an offline build refuses it.

**Drag and drop.** A file dropped on the window arrives as an internal drag whose
value is `Arc<Vec<PathBuf>>`: `.can_drop(|v, ..| v.is::<Arc<Vec<PathBuf>>>())` and
`.on_drop(..)` on the root div. There is no separate file-drop event at this rev.

**Toasts.** A `Toast` on `Root` with an `Instant` deadline and one spawned timer
that clears it after 4 s; the renderer draws it in the root's bottom-left corner.
A toast is the right answer for "Copied" and "Renamed", and it is what keeps a
failed clipboard write from taking over the error bar.

**Dialogs.** They are overlays over a `relative()` root; Escape and a backdrop
click both close them, and the window's key handler closes the top-most one
first. A dialog that must not close on a backdrop click puts an empty
`on_mouse_down` on its card.

## Rows, widths and what a file chip may do (fix round, 8 November 2026)

- **The list caches each row's height.** After any event that can change a row
  (`message.delta`, `message.completed`, `tool.changed`, `turn.completed`,
  `turn.error`, `session.changed`), after a `reset`/`splice`, and whenever
  `ListState::viewport_bounds().size.width` changes, call
  `list.remeasure()`. Without it a grown row keeps its old height and the next
  row is drawn on top of it.
- **Every text block needs a width-constrained chain**: `w_full().min_w(px(0.0))`
  on the column, the paragraph and the list row's text (`flex_1().min_w(px(0.0))`),
  or one unbreakable token in inline code widens every ancestor and the pane
  clips it. A fenced code block keeps its own lines and scrolls sideways
  (`id(..).overflow_x_scroll().whitespace_nowrap()`), as `<pre>` does.
- **Inline code needs `line_height(size * 1.65)`** on its text div, or the
  highlight background paints over the line above (clearest in the light theme).
- **A chip never runs a file.** A message names paths a bot chose, so opening one
  with its program executes bot-written code. `files::show_in_folder` is the only
  action (`explorer.exe /select,<canonical path>`, which highlights and runs
  nothing), plus an in-window preview for images and text inside the project
  (`files::read_preview`). `files::spawn_guard` tests enforce it by scanning the
  source.
- **Keys need a focused element.** `on_key_down` on the root view is not a window
  listener; on a page with no field the event goes nowhere. Bind shortcuts as
  `actions!` in the `"Workspace"` context, `.track_focus(..).key_context("Workspace")`
  on the root, and focus the root when a page has no field.
- **Stop the press inside a dialog card, a menu panel or a "…" button**:
  `cx.stop_propagation()` on its own `on_mouse_down`, or the backdrop's close and
  the card's own click both fire.

## Traps that kill or blank the window

- **Never touch a text field through its entity from inside its own hook.** A hook
  runs while the field is leased, and `Entity::lease` removes the entity from its
  map: a second `update`/`read` panics ("cannot update X while it is already being
  updated") and the panic aborts the process silently. The hooks take the field
  (`&mut TextInput`), and only the owner is notified. Programmatic `set_text`
  does not fire the change hook.
- **Install `trace::install_panic_hook()` first in `main`.** Without it a panic is
  a silent quit: the daemon child is dropped (the log stops at "daemon child
  stopped") and the message goes to stderr.
- **Draw an overlay from a clone.** `self.menu.take()` in `render` gives the menu
  one frame; this window repaints continuously, so it vanishes. Same for anything
  a render consumes.
- **A percentage width inside a content-sized parent is nothing.** A user bubble
  is as wide as its text, so `w_full()`/a wrapper block inside it collapses to one
  character. Bubbles draw shaped text directly (`styled_inline`).
- **`min_w(px(0.0))` + `w_full()` on every level of a text chain** is what stops a
  long inline-code token from widening the pane; it belongs in the chain whose
  width is definite (columns, rows with `flex_1`), not in a bubble.
- **Moving a transparent window leaves a frame edge behind** when captured; take
  screenshots without moving it when judging layout.

## The snapshot's lists can be null

The daemon writes `null` where a list is empty in several places (a message with
no attachments, `quotas` before the first reading), and one `null` where a `Vec`
is expected throws the **whole** snapshot away — the window then shows nothing and
the log says only "snapshot did not parse: invalid type: null, expected a
sequence". Every list on `Snapshot`, `Session`, `Message`, `Permission`,
`Agent`, `UserQuestion` and `QueuedPrompt` carries
`#[serde(default, deserialize_with = "serde_defaults::null_as_default")]`. A new
list field needs it too; `#[serde(default)]` alone does not accept an explicit
`null`.

## What the preview's daemon client does

Port of `apps/desktop/src-tauri/src/lib.rs`: find `bitbotsd.exe` (beside us,
then `CARGO_TARGET_DIR`, then `<repo>/target`), spawn with `BLOBLEX_PARENT_PID`
and its own `BLOBLEX_DB_PATH`, read one JSON bootstrap line within 10 s, check
protocol 1 + `127.0.0.1:` + a capability, then `POST /v1/rpc` with
`Authorization: Bearer` and follow `ws://…/v1/events` (hello
`{"v":1,"afterSequence":n}`, `events.replay.complete`, `replay.gap` → fresh
snapshot). The capability is memory-only: its `Debug` prints `<redacted>` and it
never reaches a log.

Every RPC body is `{"v":1,"id":"…","method":"…","params":{…}}`, and the answer
wraps the result: `{"v":1,"id":"…","ok":true,"result":{…}}`. `Command::Rpc` in
`worker.rs` is the door for the calls a screen drives by hand: the window sends a
`tag` with it and gets `Update::Rpc { tag, value }` back, so the worker holds no
per-method knowledge and the window reads only the calls it made. Three tags are
deliberately quiet on failure (`catalog`, `queue-list`, `image`): their absence
is a placeholder or a retry row, not a red bar over the conversation.

`clientMessageId` is 8 to 80 characters of letters, digits, hyphens or
underscores, and a send without one is not idempotent: a resend after a timeout
can send the prompt twice. `uuid::Uuid::new_v4()` fits.

**A conversation's model cannot be unset.** `session.model.update` refuses
`model: null` ("Choose a concrete model. Conversation model selections cannot be
unset."), and `session.model.plan` refuses the same. So the model menu offers
concrete models only; the pill shows the bot's own model until a conversation
pins one, exactly as the React pill does.

