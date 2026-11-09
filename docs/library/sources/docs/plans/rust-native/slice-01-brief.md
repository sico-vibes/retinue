# Brief: Retinue in Rust (GPUI), slice 1 — an installable core

**Lane:** Gogo (OpenCode Go DeepSeek V4.1 Flash, high), in its own git worktree, assigned through the Retinue MCP. The Director (Claude) mentors you, reviews every diff, runs the gates and commits.

**Owner's goal (8 November 2026):** "create the alternative rust version so I can install it and test it … a 1:1 rust version from what we have thus far with all the functions and UI and everything in place."

The full 1:1 port goes in slices. This first slice must give the owner something to **install and use for real**:
- the app opens;
- it starts its own daemon;
- the owner sees their projects, employees and conversations;
- they can read a conversation and send messages that stream back.

Later slices add the remaining screens, one at a time.

**Rules:**
- Never run `git commit`, `push`, `merge`, `stash` or `reset`, and never switch branches. Change nothing outside your worktree.
- Never print or log tokens, the daemon capability or credentials.
- Don't modify `crates/` or `apps/desktop/`. You may *read* them as the reference, and depend on `crates/retinue-protocol` by path if useful.
- If something in this brief is impossible, pick the smallest working alternative and say so in your report. If you are stuck on a decision only the owner or the Director can make, stop and ask.
- Run long builds in the foreground and report in the same turn.

**Read first:**
- `AGENTS.md`;
- `.claude/skills/retinue-development/SKILL.md`, with `references/design-language.md` and `references/codebase-map.md`;
- `docs/research/rust-native-ui-port.md`, especially sections 1, 2 and 4;
- `docs/ipc-contract.md` (the daemon API you will call).

## Where it lives

- **Crate location:** a new crate at `apps/native` (package and binary `retinue-native`). Make it **its own Cargo workspace**, not a member of the root workspace. Add `exclude = ["apps/native"]` to the root `Cargo.toml` only if cargo needs it. This keeps GPUI's dependencies and toolchain out of the existing gates.
- **Toolchain:** `rustc` here is 1.94. If GPUI needs a newer one, add `apps/native/rust-toolchain.toml` and say so.
- **GPUI:** use GPUI from Zed's repository as a git dependency **pinned to one commit** (`rev = "…"`), plus `gpui-component` (longbridge) pinned the same way if it helps (markdown, inputs, scroll views). Record both revs and why in `apps/native/README.md`.
- **Build output:** put build output on C: to save D: (`CARGO_TARGET_DIR=C:\bbn\target`). The worktree path contains an apostrophe. If any build step (resource compiler, manifest embedding) fails because of it, copy the crate to a short path such as `C:\bbn\src` and build there, as `references/verification.md` describes.

## 1. The daemon: start it, talk to it

Port the shell's supervisor logic from `apps/desktop/src-tauri/src/lib.rs`: `start_daemon` (around line 329), `ReadyLine`, `connection`, and the RPC and event calls (`daemon_rpc`, the events WebSocket).

**Finding the daemon.** Look for `retinued.exe`, in this order:
1. beside `retinue-native.exe`;
2. in dev, `<repo>/target/release/retinued.exe`, then `<repo>/target/debug/retinued.exe`.

If it is missing, show a clear message in the window, not a panic.

**Starting it:**
- Spawn it with `RETINUE_PARENT_PID=<our pid>` and `CREATE_NO_WINDOW`.
- Read the one JSON bootstrap line from stdout (10 s timeout) and check `protocolVersion == 1`, an address starting `127.0.0.1:` and a non-empty capability, exactly as the shell does.
- Keep draining stdout and stderr.

**Its own data.** Set `RETINUE_DB_PATH=%LOCALAPPDATA%\Retinue Native\retinue.db`, so this app never shares a live database with the installed Retinue. On first run, when that file doesn't exist yet and `%LOCALAPPDATA%\Retinue\retinue.db` does, ask once in the window:
- **Start with a copy of my Retinue data:** copy it with SQLite's online backup API (`rusqlite::backup`), which is safe even while Retinue runs.
- **Start empty.**

Copy the database file only. Never copy secrets, tokens or the credential store.

**The client:**
- `rpc(method, params) -> Result<Value>`: `POST http://<address>/v1/rpc` with `Authorization: Bearer <capability>` and body `{method, params}`, read the same way the shell reads it.
- `events()`: the daemon's event stream over WebSocket, the same URL and auth as the shell. Reconnect with backoff, and refresh the snapshot after a `replay.gap`.
- Run both on a background executor, not the UI thread.

**Shutdown.** When the window closes, kill and wait for the daemon child process.

## 2. The window and the look (Haze, dark first)

**Window:**
- One main window, 1360×860, minimum 1000×680, titled "Retinue (Rust preview)".
- Backdrop: GPUI's `WindowBackgroundAppearance` (Mica Alt or Blurred), falling back to an opaque dark surface.

**Tokens.** Create `src/theme.rs` with the Haze dark tokens taken from `apps/desktop/src/ui/themes/haze.css` (lines 5–36) and `ui/theme.css`: surfaces, hover and selected surfaces, lines, the three text levels, status colours, radii, font sizes.
- Every colour and size in the UI comes from `theme.rs`. No raw colours in views.
- Copy the values exactly. Where a value is `rgb(r g b / a)`, keep the alpha.

**Fonts.** Use the system UI font (Segoe UI Variable, falling back to Segoe UI) at the sizes from `theme.css` (`--fs-*`).

**Shapes.** Follow the design language: rounded rectangles, no pills, no glow, plain sentence-case copy.

## 3. Screens in this slice (port behaviour from the React source)

Lay the window out like the current app, left to right: **sidebar (280px)**, **conversations column (248px)**, **conversation pane**.

### Sidebar

Port `apps/desktop/src/ui/TeamSidebar.tsx` (top part) and `ui/ProjectsSidebar.tsx` with `projectsSidebar.css`.

- **Top:** a search box (placeholder only in this slice) and the CEO row (the agent with `systemRole == "manager"`): its portrait or initial, then its name.
- **Projects | Team switch:** Projects is the default. Team lists every non-archived, non-CEO bot (one-line rows: avatar, name, then the status in words on the right).
- **Projects tree:** port `projectBlocks` (the same grouping and ordering, using `project.members` when the daemon answers). Project headers are 32px with a fold chevron. Employee rows have two lines: the name with the time or state on the right, then the role. Under the employee holding the selection, its newest four conversations show as two-line cards: the title, then `time · status`.
- **Status words:** port `conversationStatus`. *Needs you* gets a bell icon, *Working* the working colour (the shimmer can come later).
- **Fold state:** folding projects is remembered in a small JSON settings file in the native data dir. The rule for an employee's fold is: open while it holds the selection, until the owner folds or unfolds it.
- **New project:** a "New project" button that only shows a "coming in a later slice" note is fine.

### Conversations column

Port `ui/ConversationsPanel.tsx`, without the tree branches, rename or archive.

- The selected bot's conversations, grouped by project, as two-line cards: the title, then `time · status`.
- A **New conversation** button (`agent.conversation.create {agentId, projectId?}`).

### Conversation pane

Port the reading and sending path of `ui/App.tsx`.

- **Header:** the bot's name, then " / " and the project name.
- **Message list:**
  - User messages are right-aligned bubbles; assistant messages are plain text blocks.
  - Thinking is hidden. A tool step is one muted line ("Ran a command", "Read a file", as in `read_conversation`).
  - Render basic markdown: paragraphs, bold and italic, inline code, fenced code blocks (monospace, a surface background), and bullet and numbered lists. Port the subset of `ui/SafeMarkdown.tsx`, or use gpui-component's markdown.
  - The list is virtualised or uses GPUI's `uniform_list` or `list`, so long conversations stay smooth. It sticks to the bottom while new text streams.
- **Live updates:** apply streamed events for the selected session, using the same event types the React app handles in its `listenForDaemonEvents` callback (`App.tsx` around line 1090). At minimum: message delta, message complete, session state changed, turn started and completed. Fall back to `session.get` when unsure.
- **Composer:**
  - A multi-line input, at least 1 and at most 8 lines tall.
  - Enter sends, Shift+Enter adds a new line.
  - It sends with `session.prompt {sessionId, text}`. For a bot with no conversation yet, it creates one first.
  - While a turn runs, show **Stop** (`session.cancel`, or whatever the React app calls; check `App.tsx`).
  - IME must work (type Japanese through the Windows IME as a check).
- **Approvals:** when the selected session has a waiting permission (from the snapshot or events), show a simple card with Allow and Deny (`permission.reply`), so a turn can't hang. Proposals and questions can show "Open this in Retinue to answer" for now.
- **Empty states:**
  - no bots: "No employees yet";
  - no conversation: "Start a conversation" with the composer still usable.

## 4. Packaging so the owner can install it

**Script.** Add `apps/native/scripts/package.ps1`. It:
1. builds `retinued` in release from the repo root (`cargo build -p retinue-daemon --release`, with its own target dir on C:);
2. builds `retinue-native` in release;
3. puts both executables, plus the daemon's runtime files the shell bundles (check `apps/desktop/src-tauri/tauri.conf.json` `bundle.externalBin` and resources; include only what the daemon needs to run chats), into `dist/Retinue-Native-<version>/`;
4. zips that folder.

**Installer.** If `cargo-packager` (or NSIS through it) works offline here, also make an installer that installs per user to `%LOCALAPPDATA%\Programs\Retinue Native` with a Start menu shortcut. If not, the zip is enough for this slice; say which you produced.

**Identity.** The app icon is `apps/desktop/src-tauri/icons/icon.ico`. The product name is "Retinue (Rust preview)", so it never replaces the installed Retinue.

## 5. Tests and gates

**Unit tests** (`cargo test` in `apps/native`):
- the ready-line check: valid, wrong protocol, non-loopback address, empty capability;
- `projectBlocks`: port the four cases in `apps/desktop/src/ui/hiring.test.ts` (members, a gone project or agent, ordering, `conversationStatus`);
- the markdown subset parser, if you write your own;
- the first-run copy choice: no copy when the target exists.

**Gates:**
- `cargo build --release` and `cargo clippy --all-targets -- -D warnings` in `apps/native`;
- the existing `cargo check -p retinue-daemon` still passes, since you don't touch it.

**Live check.** Run the built app once against a temporary database (`RETINUE_DB_PATH` pointing to a temp file, start empty). It should:
1. start the daemon;
2. show the empty state;
3. close cleanly with no leftover `retinued` process.

Say what you saw. You can't see the screen, so describe what you checked through logs.

## Report

- What you built, per section, with `path:line`.
- The GPUI and gpui-component revs.
- Every file you changed.
- The last lines of each gate's output.
- Where the package or installer is.
- Every decision this brief didn't specify.
- What is unverified.
- A short "GPUI patterns we used" note (entity and context, views, async tasks, lists, text input) for the skill, written as proposed text for `.claude/skills/retinue-development/references/` (a new `native-gpui.md`).
