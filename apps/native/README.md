# Bitbots in Rust (GPUI) — native preview

The native Rust version of Bitbots, ported screen by screen until it matches the
app. This crate is **slice 1: an installable core** — the app opens, starts its
own daemon, shows the projects tree, the conversations column and one
conversation, and sends messages that stream back.

It is its own Cargo workspace on purpose: GPUI's dependency tree and toolchain
stay out of the root workspace and its gates.

```
apps/native/
  Cargo.toml            own [workspace]; binary `bitbots-native`
  src/main.rs           the window: 1360×860 (min 1000×680), Mica Alt backdrop
  src/daemon/           start bitbotsd, RPC, events (a worker thread, never the UI thread)
  src/state/            the snapshot, `apply_event`, `projectBlocks`
  src/ui/               root view, sidebar, conversations, conversation, the text input
  src/theme.rs          Haze dark tokens, copied from the app's stylesheets
  scripts/package.ps1   build both executables and zip them for install
```

## Pinned revisions

| Dependency | Pin | Why |
|---|---|---|
| `gpui` | Zed `e2f3d5ed8e5a2d8532ef0a82d3594593f0523804` (15 February 2026) | Zed's own UI framework, used as a git dependency. This commit is the newest one whose `rust-toolchain.toml` still builds with the machine's `rustc 1.94.0`; `main` asks for 1.98.1. It has the Windows DWM backdrops (`WindowBackgroundAppearance::MicaAltBackdrop`) and a working IME path (`EntityInputHandler` + `ElementInputHandler`). |
| `gpui-component` | **not used** | Its published line depends on `gpui-base`/`gpui-pre` and pulls in tree-sitter, resvg and LMDB. Slice 1 needs a text field and basic markdown, both written here against the pinned gpui; adding the library is a decision for the Director. |

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

The window needs `bitbotsd.exe`. It looks, in order, beside `bitbots-native.exe`,
then `<CARGO_TARGET_DIR>\{release,debug}\bitbotsd.exe`, then
`<repo>\target\{release,debug}\bitbotsd.exe`. Build it first:

```powershell
cd ..\..
$env:CARGO_TARGET_DIR = 'C:\bbn\daemon-target'
cargo build -p bloblex-daemon --release
```

The preview keeps its own database: `%LOCALAPPDATA%\Bitbots Native\bloblex.db`,
set as `BLOBLEX_DB_PATH` on the daemon child. On first run, when that file does
not exist and `%LOCALAPPDATA%\Bitbots\bloblex.db` does, the window asks once
whether to start from a copy of the installed app's data (SQLite's online backup
API, database file only) or empty. `BLOBLEX_DB_PATH` in the environment overrides
the location, which is how the live check runs against a throwaway file.

The window writes `%LOCALAPPDATA%\Bitbots Native\native.log`: what started, what
the snapshot held, and every error. It never records the daemon capability, a
token or message text.

`BITBOTS_NATIVE_SMOKE_SECS=<n>` closes the window by itself after n seconds, so
the close path (and the daemon shutdown it triggers) can be checked without
someone at the keyboard.

## What this slice does not do yet

Settings, the companion and dictation windows, tasks, review and diffs, the
terminal, PDF and media, schedules, machines, usage, onboarding, light mode, the
animated portraits, and every other screen. Hiring is read (`project.members`) but
not written. A permission request that is a question or a proposal says "Open this
in Bitbots to answer" instead of answering it here.

## Packaging

```powershell
pwsh apps/native/scripts/package.ps1
```

Builds `bitbotsd` (and `bitbots-hook`) in release, builds `bitbots-native` in
release, stages `apps/native/dist/Bitbots-Native-<version>/` and zips it. The
product name is **Bitbots (Rust preview)**, so it never replaces the installed
Bitbots.
