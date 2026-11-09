# The native (Rust, GPUI) port: where it stands

**Paused by the owner on 9 November 2026**, to finish the profile-photo look and the rebrand first. Nothing here is released; the port lives in `apps/native` (its own Cargo workspace) beside the shipping Tauri + React app.

## What is on `main`

| Slice | Commit | What it gives |
|---|---|---|
| 1, installable core | `f5424db` | Starts its own `retinued`, with its own database and an optional copy of the installed data. The sidebar has the CEO, Projects / Team and the projects tree. The conversation pane shows markdown, streaming, approvals and an IME composer. |
| 2, look, team, new projects | `4b677f3` | Haze portraits and logos (rasterised at build time). The working shimmer, marquee and fold motion. Light theme and density. The Team page and hiring. New projects with the CEO staffing choice. Hire and bot proposal cards. A first Settings page. Daemon restart with backoff. `install.ps1`. |
| 3, everyday conversations and bots | `e2863d1` | Popovers, dialogs, toasts and pressed states. The full composer: model, effort and access pickers, the queue, slash commands, images and history. Every approval, question and proposal can be answered. Message actions, tables, code blocks, safe links, and file chips that only preview or show the file in its folder. Rename, pin and archive. The Ctrl+K switcher. The bot wizard. The usage line, Needs you, and Windows toasts. |
| 5, companion and dictation | `21b143a` | The floating companion: modes, drag, resize, per-monitor positions, the team row, approvals, the picker, peek, file drop and its composer. Synthesised sound cues. Ctrl+E on-device dictation (`retinue-speech`) with its bubble. |
| Look pass 1 | `5a7544f` | The misty backdrop computed from `haze.css`. Inter bundled. The centred 760 px chat column with 18 px bubbles. The 22 px composer. |
| Look pass 2 | `65a508a` | The sidebar brand row, switch icons with the team count, and chevron folds. User bubbles with explicit widths. |
| 4, tasks, schedules, changes, terminal, PRs | `d704e05` | The tasks board, form, detail, tree and timeline. Schedules, never in bypass. The details rail with changes, diffs, revert and changes by turn. The ConPTY terminal dock. Pull requests and delegations. Entry points in the brand row, the footer and the header. |

Gates at `d704e05`, in `apps/native`: `cargo test` 314 passed, `cargo clippy --all-targets -D warnings` clean. Build output goes to `C:\bbn\target`.

## Not on `main`

- **Slice 6** (the full Settings sheet, the CEO page, project settings, onboarding, analytics, read-only updates). It is **unfinished and uncommitted** in the conversion lane's fourth worktree (run `git worktree list` on the owner's machine; it is based on the archived history, `archive/pre-retinue`, so bring its changes over as a patch): 35 changed files and 25 new ones under `src/ui/settings/`, `analytics.rs`, `ceo.rs`, `onboarding.rs`, `project_settings.rs`, `updates.rs` and `state/settings.rs`. It was stopped mid-turn, so it may not build. Its brief is `slice-06-brief.md`. Don't delete that worktree while the port is paused.
- **Still to port after slice 6:** a visual parity pass on every screen against the React preview (the Team page, dialogs, Settings, the companion, light theme); the Stone look; the remaining delegation views (fan-out compare, sub-bot panels, abandoned worktrees); per-line review comments; a drag splitter for the terminal; cloud dictation (`TODO(owner)`); and an installer beyond the zip and `install.ps1`.

## Known gaps in what is merged

- A closed terminal tab kills its shell but not the shell's children (a job object would fix it).
- The rail lacks React's "edited earlier in this conversation" list.
- Reviews take comments per file, not per line.
- Some RPC reply shapes in slice 4 are inferred and still need checking against a live daemon: `task.image`, `session.git.stage`, `automation.preview`, `delegation.list`, and `session.pr.draft.ready`.
- The look picker has no profile photos yet. The React app gained them on 8 November (`look.photo`), and the port should follow the React look once the photo-only design settles.
- The first click on an unfocused window only activates it.

## How to resume

1. Read `.claude/skills/retinue-development/references/native-gpui.md` (patterns and traps) and the native entries at the end of `references/lessons.md`.
2. Bring slice 6 over from its worktree as a patch (it is based on `archive/pre-retinue`), renaming any old product names in it first.
3. Lane rules the owner set (8 November 2026):
   - Gogo lanes never launch or drive the app; they rely on unit tests and code reading.
   - The Director does the frontend look and checks the window with `PrintWindow` captures against a copy of the data, never the live database.
   - Every lane uses its own `CARGO_TARGET_DIR`.
