# Retinue in Rust (GPUI) — native preview

The native Rust version of Retinue, ported screen by screen until it matches the
app. This crate is **slice 2: the look, the team and new projects** — the window
opens, starts its own daemon, and shows the projects tree, the conversations
column and one conversation; each bot carries its Haze portrait or its agent's
logo; the Team page hires and releases employees; new projects can be staffed by
the CEO; proposals are answered in the conversation; there is a first Settings
page; and a killed daemon restarts by itself.

It is its own Cargo workspace on purpose: GPUI's dependency tree and toolchain
stay out of the root workspace and its gates.

```
apps/native/
  Cargo.toml            own [workspace]; binary `retinue-native`
  build.rs              rasterises the Haze portraits and agent logos to PNG, embeds the app icon
  src/main.rs           the window: 1360×860 (min 1000×680), Mica Alt backdrop
  src/art.rs            a bot's mark: portrait, coloured/tinted logo or initial; Lucide icons
  src/daemon/           start retinued, RPC, events, restart with backoff (a worker thread)
  src/state/            the snapshot, `apply_event`, the projects tree, the preset catalogue
  src/ui/               root view, sidebar, conversations, conversation, team page, settings,
                        proposals, dialogs, motion, the text input
  src/theme.rs          Haze dark and light tokens, the theme choice and the density tokens
  assets/               the few icon SVGs (Lucide) and the tinted agent logos
  scripts/install.ps1   per-user install: the folder and a Start menu shortcut
  scripts/package.ps1   build both executables and zip them for install
```

## Pinned revisions

| Dependency | Pin | Why |
|---|---|---|
| `gpui` | Zed `e2f3d5ed8e5a2d8532ef0a82d3594593f0523804` (15 February 2026) | Zed's own UI framework, used as a git dependency. This commit is the newest one whose `rust-toolchain.toml` still builds with the machine's `rustc 1.94.0`; `main` asks for 1.98.1. It has the Windows DWM backdrops (`WindowBackgroundAppearance::MicaAltBackdrop`) and a working IME path (`EntityInputHandler` + `ElementInputHandler`). |
| `gpui-component` | **not used** | Its published line depends on `gpui-base`/`gpui-pre` and pulls in tree-sitter, resvg and LMDB. The text field, the markdown subset and the previews are written here against the pinned gpui; adding the library is a decision for the Director. |
| `resvg` | 0.45, build-dependency | Rasterises the Haze portraits (full colour vector art) to PNG once at build time: GPUI's `svg()` element draws an SVG as a single tint, so it cannot show them. Already in the tree through gpui, so it needs no extra download. |
| `embed-resource` | 3 | Puts `assets/icon/retinue.ico` in `retinue-native.exe`. A failure is a build warning, not an error, because the checkout path contains an apostrophe. |

The markdown subset and the multi-line input are ported from the React app and
from Zed's `crates/gpui/examples/input.rs` at the same revision.

## Build and run

Build output goes on `C:` because `D:` is short on space, and the checkout path
contains an apostrophe.

```powershell
cd apps/native
$env:CARGO_TARGET_DIR = 'C:\bbn\target'
cargo build            # or: cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

The window needs `retinued.exe`. It looks, in order, beside `retinue-native.exe`,
then `<CARGO_TARGET_DIR>\{release,debug}\retinued.exe`, then
`<repo>\target\{release,debug}\retinued.exe`. Build it first:

```powershell
cd ..\..
$env:CARGO_TARGET_DIR = 'C:\bbn\daemon-target'
cargo build -p retinue-daemon --release
```

The preview keeps its own database: `%LOCALAPPDATA%\Retinue Native\retinue.db`,
set as `RETINUE_DB_PATH` on the daemon child. On first run, when that file does
not exist and `%LOCALAPPDATA%\Retinue\retinue.db` does, the window asks once
whether to start from a copy of the installed app's data (SQLite's online backup
API, database file only) or empty. `RETINUE_DB_PATH` in the environment overrides
the location, which is how the live check runs against a throwaway file.

The window writes `%LOCALAPPDATA%\Retinue Native\native.log`: what started, what
the snapshot held, who is hired where (counts only) and every error. It never
records the daemon capability, a token or message text.

`RETINUE_NATIVE_SMOKE_SECS=<n>` closes the window by itself after n seconds, so
the close path (and the daemon shutdown it triggers) can be checked without
someone at the keyboard. `RETINUE_NATIVE_FIRST_RUN=empty|copy` answers the
first-run question up front, and `RETINUE_NATIVE_OPEN_SESSION=<id>` opens one
conversation on the first snapshot, which is how the conversation pane is checked
without a click.

## What this slice does

- **Portraits and logos.** `agent.look.icon` is `logo`, `logo-tint`, `logo-mono`
  or a `char-*` portrait, as in `blob/haze/HazeMark.tsx`; the portraits are PNGs
  `build.rs` rasterises from `apps/desktop/src/blob/haze/characters`, the logos
  from `apps/desktop/src/assets/providers`. A bot whose agent has no logo shows
  its initial on its own colour. Images are decoded once (`Arc<Image>`), never per
  frame.
- **Motion.** The working word carries a light band sweeping over it (1.9 s, per
  `projectsSidebar.css`), a selected or hovered row's title marquees when it
  overflows, and folding a project or an employee animates its height over
  180 ms. With Windows' "Show animations" off (`SPI_GETCLIENTAREAANIMATION`)
  every one is instant. A title that overflows and is not moving ends in an
  ellipsis.
- **Conversation.** Runs of tool steps fold into one muted line ("Ran 30
  commands") that expands on click, and opening or switching a conversation
  lands on its last line.
- **Light theme and density.** Haze's light block, following Windows at start and
  when it changes, with Theme and Density in Settings.
- **The Team page** (`Your team` and `Hire`), hiring from the preset catalogue
  with `project.hire`, releasing with `project.release`, and following
  `project.members.changed`.
- **New projects** with the folder picker, then the staffing choice: interview
  the CEO, hire by hand, or not now. A project's "+" offers a new conversation,
  a hire and asking the CEO.
- **Proposals** for a `hire` (with "Hire all N") and a `bot` (create or skip);
  everything else still says to open Retinue.
- **Settings**: theme, density, where the database is, copying the installed
  data again, and the daemon's version.
- **Daemon resilience**: an unexpected exit shows a notice, restarts after 1, 2,
  5 and then 10 seconds, reloads the snapshot, and offers Retry when it gives up.

Not yet: the companion and dictation windows, tasks, review and diffs, the
terminal, PDF and media, schedules, machines, usage, onboarding, the animated
portraits, editing a bot, creating your own employee, search, and the remaining
settings pages.

## Packaging and installing

```powershell
pwsh apps/native/scripts/package.ps1
```

Builds `retinued` (and `retinue-hook`) in release, builds `retinue-native` in
release, stages `apps/native/dist/Retinue-Native-<version>/` with `install.ps1`,
the icon and a short `README.txt`, and zips it as
`Retinue-Native-<version>.zip`. `cargo-packager` is not used: it is not
available offline.

```powershell
# from the unzipped folder
pwsh -ExecutionPolicy Bypass -File .\install.ps1
pwsh -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA\Programs\Retinue Native\install.ps1" -Uninstall
```

Install copies the folder to `%LOCALAPPDATA%\Programs\Retinue Native` and adds a
Start menu shortcut named **Retinue (Rust preview)** with the app icon.
`-Uninstall` removes the shortcut and that folder, and never the data folder
`%LOCALAPPDATA%\Retinue Native`.

## Third-party art

The icons in `assets/icons` are Lucide 0.468.0, the version `lucide-react` is
pinned to, under the ISC licence (see `assets/icons/LICENSE.txt`). The tinted
agent logos in `assets/logos` are the same paths `HazeMark.tsx` draws, kept in
one colour so GPUI can tint them. The portraits and provider logos are read from
`apps/desktop/src` at build time, never copied into this crate.
