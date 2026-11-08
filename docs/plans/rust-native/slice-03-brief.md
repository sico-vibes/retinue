# Brief: Bitbots in Rust (GPUI), slice 3 — everyday conversations and bots

**Lane:** Gogo (OpenCode Go DeepSeek V4.1 Flash, high), in its own git worktree, assigned through the Bitbots MCP. The Director (Claude) mentors you, reviews every diff, screenshots the window, runs the gates and commits.

**Goal:** after this slice the owner can use the Rust app for a whole working day of chatting with bots without opening the React app. Slices 1 and 2 (`apps/native`, commits `f5424db` and `4b677f3`) are the base.

**Rules (unchanged):**
- Never run `git commit`, `push`, `merge`, `stash` or `reset`, and never switch branches. Change nothing outside your worktree.
- Never print or log tokens, the daemon capability or credentials.
- Don't modify `crates/` or `apps/desktop/`. Read them as the reference: port behaviour, copy and RPC calls from the React components named below.
- If something is impossible at the pinned GPUI rev, pick the smallest working alternative and say so. Stop and ask on decisions that are the owner's.
- Build output stays on C: (`CARGO_TARGET_DIR=C:\bbn\target`).

**Read first:** `.claude/skills/bitbots-development/references/native-gpui.md`, and the 2026-11-08 native entry at the end of `references/lessons.md`. It covers `.truncate()`, `min_h(0)` scroll areas and explicit line heights, the three things the last review caught. Apply them everywhere you add UI.

## 0. Shared primitives first

Build these once in `src/ui/` and use them everywhere below.

- **Popover/menu:** a floating panel anchored to a button (GPUI `anchored()` + `deferred()`), closing on Escape, on a click outside and on choosing an item. Replace slice 2's inline project "+" menu with it.
- **Dialog behaviour:** Escape and a backdrop click close every dialog. Focus moves into the dialog and back on close.
- **Pressed state** on buttons and rows. Use `.active(..)` if the pinned rev has it on interactive elements; otherwise track mouse down in the element.
- **Select:** a dropdown built on the popover (port `ui/Select.tsx`), for model, effort and permission choices.
- **Toast:** a short notice in the bottom corner that fades after 4 s, for "Copied", "Renamed" and errors.

## 1. The composer, complete

Port `ui/ComposerControls.tsx`, `ui/SessionExecutionControls.tsx`, `ui/PromptQueue.tsx`, `ui/SlashMenu.tsx` and the composer part of `App.tsx`.

- **Controls row** under the input: the model, effort and permission mode of this conversation. Use the same RPCs and the same rules as the React app (what writes the bot default and what pins the conversation). Use the same warning when switching model in a conversation that has context.
- **Queue while busy:** sending during a running turn queues the prompt, as the React app does. Queued prompts show above the composer with remove.
- **Slash commands:** typing `/` at the start opens the menu of the bot's commands (`ui/BotCommands.tsx`, `SlashMenu.tsx`). It supports arrow keys, Enter and Escape.
- **Images:** paste an image from the clipboard, or drop an image file on the window. The attachment shows as a thumbnail chip with remove and is sent the way the React app sends attachments. Check `App.tsx` for the RPC and size limits.
- **History:** Up in an empty composer recalls the last sent prompt.

## 2. Answering everything in the conversation

- **Approvals:** port `ui/ApprovalCard.tsx`, `approvalUi.tsx` and `PermissionChoiceButton.tsx`, so the card offers every choice the React card offers (allow once, always for this, deny), shows the tool and its input summary, and shows the diff for edits if the React card does.
- **Questions:** port whatever renders the bot's questions (`AskUserQuestion`-style; find it from `App.tsx`), with options, multi-select and free text, then submit. "Open this in Bitbots to answer" should no longer appear anywhere.
- **Other proposal kinds** still unported: list which they are in your report. Port each one that only needs a card and Approve/Skip.

## 3. Messages

Port `ui/MessageActions.tsx`, the message parts of `SafeMarkdown.tsx`, `RichTable.tsx` and `FileReferenceChip.tsx`.

- **Hover actions** on a message: Copy, plus Retry/Edit where the React app offers them, using the same RPCs.
- **Markdown:** add tables, links (they open in the browser after the same check the React app does), headings and block quotes. Code blocks get a Copy button and a language label.
- **File references** become chips. Clicking one opens the file with the system's default app, through the same path check the React app uses.
- **Images in messages:** show sent and generated images as thumbnails (`GeneratedImage.tsx`, `MediaPreview.tsx`); a click opens them larger.
- **Time:** a message's time appears on hover.

## 4. Conversations

Port the remaining parts of `ui/ConversationsPanel.tsx`, `ui/SessionRail.tsx` and the conversation-list parts of `App.tsx`.

- Rename (inline), Archive and Unarchive, and Pin and Unpin, from a "…" popover on each card and from right-click.
- Pinned conversations show in a Pinned group at the top of the sidebar, as `TeamSidebar.tsx` does.
- **Branch lines:** conversations started from another one show the tree lines the React column draws.
- An **Archived** toggle at the bottom of the column.

## 5. Search and switching

- **Quick switcher** (Ctrl+K), ported from `ui/QuickSwitcher.tsx`: type to find bots, conversations and projects; arrows and Enter.
- The sidebar search box becomes real. Port `ui/ProjectSearch.tsx` behaviour: it filters the tree as you type.
- **Keyboard shortcuts** from `ui/Keys.tsx` that apply to screens that exist so far. At least: new conversation, quick switcher, settings, focus the composer, and next or previous conversation.

## 6. Bots: create and edit

Port `ui/BotWizard.tsx`, `ui/BotEditor.tsx`, `ui/BlobSettings.tsx` and `ui/LookPicker.tsx` (Haze portraits and logos only).

- **Create your own employee:** the Team page's "Someone else" card and a "New bot" button on the Team page open the wizard. It covers name, role and tags, coding agent and model, effort, permission mode, instructions, project, and portrait or logo with colour.
- **Bot settings page:** opened from a bot's "…" popover (sidebar Team view and Team page). Same fields as the wizard, plus Archive and Unarchive. Use the same RPCs as React (`agent.update`, `agent.archive`…).
- **Archived bots:** a list on the Team page with Restore.

## 7. Usage and attention

- **Usage bar:** port `ui/QuotaBar.tsx` and `ui/SidebarUsage.tsx` at the bottom of the sidebar, shown when the `usageBarEnabled` setting is on, in the Haze mist colours from `themes/haze.css`.
- **Needs you:** port `ui/NeedsYou.tsx`, the list of conversations waiting on the owner, where the React app shows it.
- **Connection banner** (`ui/ConnectionBanner.tsx`) while the daemon is restarting.
- **Notifications:** a Windows toast when a turn finishes or needs the owner while the window isn't focused, if the React app does this (check `App.tsx`). Use the same wording.

## 8. Tests and gates

**Unit tests** for:
- the slash menu filter;
- the queue order;
- the question answer payload;
- the markdown table and link parsing, including the link-safety check;
- the quick switcher ranking;
- pin and archive grouping;
- the wizard's validation;
- the popover close rules, as a pure state function.

**Gates:** in `apps/native`, `cargo build --release`, `cargo test` and `cargo clippy --all-targets -- -D warnings`. Also `cargo check -p bloblex-daemon`.

**Live check** against a temporary database:
1. create a bot through the wizard (RPC-level is fine);
2. send a prompt with an image attachment;
3. queue a second prompt while the first runs;
4. rename, pin and archive a conversation;
5. close cleanly.

Report what the logs showed. Rebuild the zip with `scripts/package.ps1`.

## Report

- What you built, per section, with `path:line`.
- Every file you changed.
- The gate tails.
- Where the zip is.
- Decisions this brief didn't make.
- What is unverified, and which screens you're least sure of.
- Proposed `native-gpui.md` additions (popovers, focus, clipboard images, drag and drop, toasts), applied to the file.

## After this slice (for planning, not now)

- **Slice 4:** Tasks board and schedules (`TasksView`, `TaskForm`, `SchedulesView`), diffs and changes (`ChangesView`, `DiffBody`, `ChangeBanner`), terminal dock, pull requests.
- **Slice 5:** the companion window and dictation (`Companion*`, `Dictation*`), sounds, onboarding, launch intro.
- **Slice 6:** the full Settings sheet (accounts, connections, memory, skills, secrets, approval rules, backup, remote, machines, health, diagnostics, keybindings), analytics, Stone look, updates.
