# Native GPUI app (`apps/native`)

The Rust port of Bitbots. Slice 1 (8 November 2026) was an installable core: the
window opens, starts its own `bitbotsd`, shows the projects tree, the
conversations column and one conversation, and streams replies. Slice 2 (8
November 2026) adds the look, the team and new projects: Haze portraits and
logos, the working shimmer, the marquee and the folding motion, the light theme
and density, the Team page with hiring, new projects with the CEO staffing
choice, hire and bot proposal cards, a first Settings page, daemon restart with
backoff, and `scripts/install.ps1`. Later slices add the remaining screens.
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

## What the preview's daemon client does

Port of `apps/desktop/src-tauri/src/lib.rs`: find `bitbotsd.exe` (beside us,
then `CARGO_TARGET_DIR`, then `<repo>/target`), spawn with `BLOBLEX_PARENT_PID`
and its own `BLOBLEX_DB_PATH`, read one JSON bootstrap line within 10 s, check
protocol 1 + `127.0.0.1:` + a capability, then `POST /v1/rpc` with
`Authorization: Bearer` and follow `ws://…/v1/events` (hello
`{"v":1,"afterSequence":n}`, `events.replay.complete`, `replay.gap` → fresh
snapshot). The capability is memory-only: its `Debug` prints `<redacted>` and it
never reaches a log.
