# Brief: Retinue in Rust (GPUI), slice 4 — tasks, schedules, changes, terminal and pull requests

**Lane:** Gogo (OpenCode Go DeepSeek V4.1 Flash, high), in its own git worktree. **Two other lanes run at the same time:**
- slice 6 (Settings), in another Gogo conversation;
- the Director's Haze look pass, in the main checkout.

Keep to the file ownership below.

**Base:** `main` at `21b143a` (slices 1, 2, 3 and 5 merged).

## Rules

- Never run `git commit`, `push`, `merge`, `stash` or `reset`, and never switch branches. Change nothing outside your worktree.
- Never print or log tokens, the capability or credentials.
- Don't modify `crates/` or `apps/desktop/`. Read them as the reference.
- **No live or GUI testing.** The owner forbids it.
  - Don't launch the app.
  - Don't drive the mouse or keyboard.
  - Don't take screenshots.

  Verify with unit tests and code reading. The owner and the Director test windows later.
- `CARGO_TARGET_DIR=C:\bbn\target4`, your own.
- If something is impossible at the pinned GPUI rev, take the smallest working alternative and say so.

**Read first:** `.claude/skills/retinue-development/references/native-gpui.md`, especially "Traps that kill or blank the window":
- double update;
- never consuming state in render;
- `min_h(0)`;
- `.truncate()`;
- text hooks get the field itself.

Read the native entries at the end of `references/lessons.md` too.

## File ownership

**You own new files:**
- `src/ui/tasks/` (board, form, tree, timeline, images);
- `src/ui/schedules.rs`;
- `src/ui/changes/` (changes view, diff body, change banner, the session rail);
- `src/ui/terminal/` (dock, PTY);
- `src/ui/pull_request.rs`;
- `src/state/tasks.rs`, `src/state/changes.rs`.

**Shared files, minimal hooks only.** List every line in your report.
- `src/ui/root.rs`: new `Page` variants, the render dispatch, the event arms that feed your state, and the layout slots for the rail and the terminal dock.
- `src/ui/mod.rs` and `src/state/mod.rs`: `mod` lines and event routing.
- `src/ui/search.rs`: switcher actions and shortcuts.
- `Cargo.toml`.

**Don't edit:**
- `theme.rs`, `art.rs`, `sidebar.rs`, `conversations.rs`, `conversation.rs`, `message.rs`, `markdown.rs`, `composer.rs` or `team.rs` (the Director is restyling them);
- `settings_page.rs` or anything under `src/ui/settings/` (slice 6).

If you need an entry point in the sidebar or the conversation header, add a public method on `Root` (for example `open_tasks`). Say in your report where the button should go; the Director adds it.

## 1. Tasks board

Port `ui/TasksView.tsx`, `TaskForm.tsx`, `TaskTree.tsx`, `TaskTimeline.tsx` and `TaskImageStrip.tsx`, plus the tasks parts of `App.tsx`.

- **The board:** columns by state, as React draws them, with cards.
- **New task and edit:** a form with title, brief, done goal, report back, bot, project and images.
- **Running a task:** assign and run it, and drag a card onto a bot if GPUI drag-and-drop works at this rev. Otherwise use an "Assign to…" select.
- **Reading a task:** the sub-task tree and the timeline.

Use the same RPCs and events as React (`task.*`).

## 2. Schedules

Port `ui/SchedulesView.tsx`, `ScheduleWhen.tsx` and `ScheduleOverview.tsx`:
- list, create, edit, pause and delete, with the "when" editor;
- the same RPCs;
- automations bring their own caps (runtime limit, concurrency, pause), and the permission mode is never bypass. Keep the React rules.

## 3. Changes and the session rail

Port `ui/SessionRail.tsx`, `SessionRailExtras.tsx`, `ChangesView.tsx`, `DiffBody.tsx` and `ChangeBanner.tsx`.

- **The rail:** a right-hand column in the conversation view, toggled from the header, showing environment, workspace, context, outputs and changes, plus "Changes by turn".
- **Diffs:** shown with added and removed lines coloured from the theme tokens. Large diffs use a virtualised list.
- **File revert** stays as React has it. It's a desktop-only RPC, which is fine here.

## 4. Terminal dock

Port `ui/TerminalDock.tsx` and the native `apps/desktop/src-tauri/src/pty.rs`: a bottom dock in the conversation view with tabs, running the user's shell in the project folder through ConPTY (`portable-pty` or Windows ConPTY directly).

- Render with a simple monospace grid. Basic ANSI colours and the cursor are enough; a full VT emulator (`vte` or `alacritty_terminal` parsing) only if it's cheap.
- Input, paste, resize and scrollback.
- Never log terminal content.

## 5. Pull requests and delegation views

- **Pull requests:** port `ui/PullRequestDialog.tsx`, the same daemon RPCs.
- **Delegation:** port `DelegateBoard.tsx`, `SubbotViews.tsx`, `DelegationGroup.tsx`, `FanoutCompare.tsx` and `AbandonedWorktrees.tsx` where they show in the conversation view. If time runs short, list what's left.

## 6. Tests and gates

**Unit tests:**
- board grouping and ordering;
- task form validation;
- the schedule "when" parsing and next-run text;
- diff parsing and line numbers;
- ANSI parsing for the terminal subset;
- PTY resize maths.

**Gates** in `apps/native`, run once each at the end: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`. Package with `scripts/package.ps1 -NativeTarget C:\bbn\target4 -DaemonTarget C:\bbn\daemon-target4`.

## Report

- What you built, per section, with `path:line`.
- Every shared-file line you touched.
- Where entry buttons should go.
- The gate tails and the zip path.
- What's left and what's unverified.

Apply proposed `native-gpui.md` additions to the file (terminal, diffs, drag and drop).
