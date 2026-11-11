# ADE interface research and delegation: how the best tools behave, and what Retinue should borrow

Date: 10 November 2026. Author: Grey (research lane). Second round, for the owner. Research only: no product code changed, no other file created.

Scope: Part 1 studies the core interactions of the 2026 agentic development environments; Part 2 studies amElnagdy's delegation skills against Retinue's own delegation; Part 3 looks for Katana by Max Blade; Part 4 ranks the ideas.

**Confidence labels:** **[code]** read in the repository; **[docs]** the product's own documentation or release notes; **[public]** a third-party article, post or video; **[unchecked]** could not verify.

## 1. Summary: the dozen findings that matter most

1. **One surface should speak, once.** CNVS's Distraction Guard is the clearest statement of the rule Retinue lacks: if the owner is looking at the agent, the card is the notification; elsewhere, a tile; background, one OS notification; just typed there, nothing. Today the same approval can be a card, a bell row, a badge and a phone push. Adopt it in `NeedsYou.tsx`, the permission events and `push.rs` (Part 4, idea 1).
2. **Order the conversation list by attention, not by time, and fold the busy ones.** Claude Code's agent view (Ready for review / Needs input / Working / Completed) and T3's Working shelf that returns a thread to the top when it needs you are the two best models; Retinue has the states already, not the ordering (`ProjectsSidebar.tsx`, `ConversationsPanel.tsx`).
3. **Queued messages are real objects, and the verbs are now settled across the field:** *Steer*, *Stop the agent and send this now*, *Edit queued message*, *Remove queued message* (Superset), *Send now* plus edit-in-composer (T3), one-button morphing (Orca, Cursor). Retinue's rows lack the first two in one place (Part 1.1).
4. **A usage limit belongs in the composer strip, not a transcript chip**, with "Resets 3:16 AM" / "Resumes tomorrow at 3:16 AM", two actions and an honest "reset time unavailable" (T3), plus a *Switch account* choice (Monocode). Retinue has the state in the daemon and shows it in the wrong place (Part 1.2).
5. **Review wants a notes tray and scopes.** Orca's line notes collect and then go to an agent as one message; Codex's five scopes (Unstaged / Staged / Commit / Branch / Last turn) are the best diff control in the survey. Retinue sends each comment separately and has one scope (Part 1.6; Part 4, ideas 3 and 7).
6. **A per-turn review card — "Changed 3 files · Undo · Keep · Commit · Review" — is the strongest recent addition to a conversation UI** (Monocode, Superset), and it fits Retinue's existing Changes-by-turn card exactly.
7. **A delegated bot's approval must be answerable where the work was asked for.** T3 renders a child's approval card in the parent conversation; Retinue shows a chip that only navigates away (Part 1.8, idea 4).
8. **The delegation brief needs named parts, and the report needs a shape.** amElnagdy's skills give a five-block brief (Task, Do not touch, Context, Rules/commands, Report) and a review that treats the implementer's report as a claim; Retinue's one sentence ("a self-contained brief: goal, context, paths, constraints and what to send back") is the whole gap (Part 2, §3.4 gives the replacement wording).
9. **Retinue's delegation machinery is already ahead of the skills**: a durable ledger, status words the owner can read, a board, Stop all, loop guards, worktree isolation. What is missing is *writing* — brief structure, a teammate report contract, a review rule — not plumbing.
10. **Katana is not an ADE.** It is a browser Counter-Strike-style shooter at `cnvs.dev/katana/`, titled "BANANA SHIPMENT", built inside Max Blade's ADE **CNVS** as a showcase; no repository, price or date is public. The relevant material is in CNVS's own release notes — the alerting rule above and on-device goal titles per agent (Part 3).
11. **Starting work should show fewer choices and reveal the rest.** Orca's single card (project → name → agent → Advanced) and Sculptor's controls that appear only after a prompt are the pattern; Retinue spreads project, model, effort and mode across pills (Part 1.7, idea 12).
12. **The polish that reads as "current" is unglamorous:** undo toasts for state changes with `⌘Z` (T3), empty states that are launchers (Sculptor), "a settled source may claim emptiness" (Superset — never flash an empty state while loading), scroll that disengages when the reader scrolls up (Sculptor), and drafts that survive a restart (Monocode's explicitly do not).

## 2. Part 1: how the leading ADEs handle the core interactions

A note on what I could see. Closed products (Claude Code desktop, Codex, Cursor, Conductor, Warp, Factory, Amp, Windsurf) are described from their own documentation, release notes and help pages: I could not run them, so where a behaviour is only implied I say "inferred". Open products (T3 Code, Monocode, Orca, Sculptor, Vibe Kanban, Superset, Crystal, Zed) are read in the code, with `path:line`. **Nuphos** and **OpenBot** were studied in the first report for their mechanics (their queue-with-steer and session-ownership models are cited there) and are not re-read here; their UI is not described this round. Of the owner's list, **Acepe** could not be found as a product under that name, and it is the only item dropped; **Crystal** and **Superset** (which presents itself as *Paradev*) are covered from code.

### 2.1 The composer while an agent is busy

**What the best do.**

- **Warp** has the most complete queue: a per-conversation queue shown as real, editable rows above the input, with a header count ("2 queued"), drag reorder, *Send now*, edit, delete, an "⏎ to send" hint, a clock-plus **auto-queue** toggle in the working indicator (`⌘+Shift+J`), `/queue`, and a documented pause on error [docs]. The queue is a first-class object, not a notification.
- **Superset** (repository `superset-sh/superset`; the UI calls it *Paradev*) puts the queue in a drawer with the clearest verbs found anywhere: each row offers *Steer*, *Stop the agent and send this now*, *Edit queued message*, *Remove queued message*; the composer's button tooltip reads "Queue; ⌘ to steer the running turn" (`QueuedPromptRow.tsx:83-155`, `ComposerBody.tsx:1321-1328`) [code]. A failed optimistic row says "Failed to send" with Retry / Discard (`UserMessageRow.tsx:141,242-250`).
- **Cursor** splits the keystrokes by intent: while the agent works, **Enter** adds to a visible queue you can drag to reorder, **Cmd+Enter** sends immediately, **Tab** queues for after the turn, and *Send now* (or double-Enter) steers "at the agent's next tool call" [docs].
- **T3 Code** makes steer the default and puts the queue at the *foot of the timeline*, not above the composer: the send icon morphs to a corner-arrow (steer) or list-plus (queue), the tooltip reads "Click to steer, ⌘-click or ⌥-click to queue", and the labels are *Queue message*, *Steer message*, *Submit message*, *Resume thread* (`ComposerPrimaryActions.tsx:88,224-250,284`) [code]. Queued items are right-aligned **dashed** bubbles in the timeline footer ("Dashed and unfilled: a turn that has not been sent yet", `QueuedRunsControl.tsx:406,452-460`), each with *Send now*, a pencil that edits **in the composer** ("Editing in the composer", `:293,310`), and *Remove from queue*; past three rows they collapse to "Show all N queued". There is **no reorder** — the queue is FIFO (`:56,389`). Stop is a separate red square, with a **Force stop** path when the agent ignores it (`:140-148`, `ThreadActivityBanners.tsx:147-162`).
- **Monocode** keeps the queue in a card *inside* the composer above the input, with paused states written as sentences — "Queue paused because you interrupted", "Messages paused", "A message couldn't be sent" — and *Resume* / *Retry* (`Composer.tsx:1864-1872`, `MessageQueue.tsx:70-97`) [code]. Rows edit inline (a textarea, Enter saves, Esc cancels), each has *Steer*, and status reads "Sending…" or "Waiting to send" (`MessageQueue.tsx:94-159,181-197`). No reorder.
- **Sculptor** allows exactly **one** queued message, shown as a bar with *Remove queued message*, *Edit message* and *⇧⏎ Interrupt and send*, and opens an undo dialog if an edit races the send (`queuedMessages/QueuedMessages.tsx:24-25`, `QueuedMessageBar.tsx:110-151`, `UndoQueuedMessageDialog.tsx`) [code]. Send never becomes Stop: the button stays "Send message" and Stop lives in the status pill (`ChatInput.tsx:1115-1123`, `StatusPill.tsx:369-387`).
- **Vibe Kanban** holds a single button that becomes "Queue" / "Cancel Queue" while running, confirms with a toast — "Message queued - will execute when current run finishes" — and puts the text back in the editor when a queued message is cancelled (`SessionChatBox.tsx:335-337`, `tasks.json:110,141-142`, `SessionChatBoxContainer.tsx:627-635`) [code].
- **Orca** morphs one button: while working, Send takes Stop's place (`NativeChatComposerActions.tsx:37`, label "Stop the agent" / "Stopping…" at `:163-165`), holding the modifier previews "{{shortcut}} to steer" / "to queue" and the button reads "Steer", then "Steer queued" (`:127-139`). The queue is an inline region above the composer with "Queued" / "Paused", "Clear all" and "Resume" (`queue/NativeChatQueuedMessageList.tsx:207-208,233,245`); each card offers "Send this message now", "Steer", "Edit message", "Delete" (`NativeChatQueuedMessageCard.tsx:107-113,304-337`).
- **Claude Code desktop** has no labelled queue: "type a correction and press Enter… Claude reads the correction as soon as the current action completes" — steer without stopping, with a separate stop button [docs]. **Amp** makes that the default: any message is steered after the current step, `⌘/Ctrl+Enter` queues until the agent finishes, `Esc` twice interrupts [docs]. **Windsurf** (legacy Cascade): type while working and press Enter to queue; press Enter again on an empty box to send immediately [docs].
- The counterexample is **Crystal**, which has no queue or steer at all: an input with `⌘+Enter` to send and nothing while a turn runs (`SessionInput.tsx:51,97`) [code]. That omission is felt immediately in a multi-agent app.

**What Retinue should do.** Keep the composer card's single status row (`ComposerStatus` in `ui/PromptQueue.tsx`) and improve the *rows* it unfolds, not the composer chrome:

1. Each queued row gets the Orca/Zed verbs as one icon group: **Send now** (moves into the turn once the daemon can steer, from the previous round's work), **Edit** (opens in the composer, with its attachments, as T3 does), **Remove** (already there), and drag reorder (already there as up/down buttons; a drag handle is the natural upgrade for a two-line row).
2. Add the missing state label the closed tools all show: when the daemon cannot steer, the Send now control is disabled with the reason in its tooltip ("This coding agent cannot take a message into a live turn"),
   the same sentence `busySend.ts:56` already uses.
3. The queue already pauses on failure and on restart (`queue.rs:471-484`). Surface that as "Paused" *in the row header* with **Resume**, which is Warp's pattern and already exists as `QueueBanner`'s paused strip (`PromptQueue.tsx:59-62`); make it the same control in the folded state, not a second thing.
4. Do not morph the send button into Stop: Retinue's design rules give Stop its own place and the composer already has "Stop and send instead" (`PromptQueue.tsx:143`). Orca's morphing button is elegant but it fights the rule that "the composer has no Steer or Queue control" (`design-language.md:114`).

Extends: `ui/PromptQueue.tsx` (`QueueBanner`, `ComposerStatus`), `ui/App.tsx` (`sendQueuedNow`, `updateQueued`, `reorderQueued`), `ui/busySend.ts`.

### 2.2 Usage limits

**What the best do.**

- **T3 Code** is the reference (round 1, §3): a warning bar in the thread — "Usage limit reached" with "Resets 3:16 AM", "Resumes tomorrow at 3:16 AM" when an auto-resume owns it, or "Reset time unavailable; retry manually" — two inline actions, *Resume at reset* / *Cancel auto-resume* and *Snooze until reset* / *Wake now*, with "Saving…" while pending and *Copy request for admin* for workspace limits (`UsageLimitRecoveryBanner.tsx:63-99,143-152`; `timestampFormat.ts:220`) [code]. It also offers a **model fallback** inline: "Using {fallback}", "{default} is out of usage · resets …", button "Use {model}" (`ComposerUsageLimits.tsx:89-99`), and marks the thread "Limited" in the sidebar (`SidebarThreadRow.tsx:766-771`).
- **Monocode** states the limitation instead of hiding it: an amber bar with "Resets 3:16 AM · in 4h 42m" or "Limit has reset", *Resume* when the reset has passed, *Resume at reset* / *Resuming at reset* (tooltip "Cancel the automatic resume"), and an inline **account** choice: *Switch account* / *Choose another account* (`UsageLimitNotice.tsx:53-64,129-153`, `UsageLimitAccountPicker.tsx:102`; `usageLimit.ts:59`). Its copy for the switch is "Your conversation is saved. Choose another model or account to keep going." (`UsageLimitAccountPicker.tsx:91`) — and its help text says plainly that automatic continuation needs the app open with that session loaded (`usageLimit.ts:95-140`).
- **Codex** and **Conductor** put a *goal* row above the composer with pause, resume, edit and clear; stopping a turn also pauses the goal [docs]. That is the same shape as a limit banner but for goals, and it confirms the placement: a row above the composer, not a chat message.
- **Claude Code desktop** and **Cursor** do not document a limit banner; their reset handling is a usage page [docs]. **inferred**: neither resumes automatically.

**What Retinue should do.** Move the limit state from a transcript chip (`UsageLimitNotice`, `TeamMessages.tsx:137-142`) to a strip inside the composer card, the one place the owner is already looking when they try to type:

1. One line in `ComposerStatus`'s status row, with the state word and the time: "Usage limit · resets 2:00 AM" or "Usage limit · resumes 2:00 AM", and the two actions as text links: *Resume at reset* (armed state) and *Snooze* — the previous round's daemon work supplies both.
2. Keep a transcript note for the record, as today; the strip is the control.
3. The composer already knows: while a conversation waits for a limit, the send button becomes a clock and the message goes out at the reset (`App.tsx:2107-2118`, `usageLimitSend.ts`). The strip should say that in one sentence: "A message you send now goes out at 2:00 AM."
4. When the provider gives no reset time, say "Reset time unknown · retry manually" — never a guessed time.

Extends: `ComposerStatus` (`ui/PromptQueue.tsx`), `ui/TeamMessages.tsx` (`UsageLimitNotice` stays as the record), `ui/App.tsx` (`limitUntil` already computed), `ui/usageLimitSend.ts`, plus the new banner from the previous round.

### 2.3 The conversation list with many agents running

**What the best do.**

- **Claude Code's agent view** is the clearest attention model: rows grouped **Ready for review**, **Needs input**, **Working**, **Completed**, each with a one-line model-written summary, icons whose colour *and* shape encode state, and PR numbers coloured by check status; `Space` opens a peek panel you can reply in without attaching [docs].
- **Factory** uses plainer words for the same idea — **Waiting on you**, **Working**, **Idle** — as a right-hand Agents tab beside the orchestrator chat, with PRs and Tasks as sibling tabs [docs]. **Orca** turns it into a kanban drawer: **Needs You / Working / Done / Idle**, rows reading "started 14:02" / "done 14:40" (`AgentKanbanBoard.tsx:36-47`, `DashboardAgentRow.tsx:232-243`) [code] — and the same buckets on its phone (`MobileHomeAgentsTab.tsx:307-311`).
- **T3 Code** separates *settled* from *archived* and folds working threads into a collapsed **Working** section that returns a thread to the top when it finishes, fails or needs an answer (`docs/user/thread-sidebar.md`) [docs].
- **Amp** adds *Snooze Until I'm Needed*: a snoozed thread stays quiet but "shows in the sidebar" the moment it waits for an approval or an answer [docs]. That is the cleanest definition of "interrupted only when needed" found anywhere.
- **Sculptor** shows two status dots on a workspace when only some of its agents errored (`StatusDot.tsx:41-53`), and a peek popover per workspace ("X of Y tasks done — waiting for input").

**What Retinue should do.** Retinue already has the structure — Projects → employees → conversations, with *Needs you* in amber and status words on the second line (`design-language.md:122-125`), plus the bell. The improvement is not a new view; it is **ordering and folding**:

1. Order conversations by *attention*, not recency, within an employee: Needs you, then Working, then done-but-unread, then the rest. The daemon already records each conversation's state (`presentedSessionState`), so this is a sort in `teamSelectors.ts`/`ProjectsSidebar.tsx`.
2. A **fold for busy conversations**, like T3's Working section: when three or more are working, collapse them under one "Working · 3" row inside the project, and pop one out the moment it needs an answer. `Collapse.tsx` already provides the motion.
3. Add Amp's snooze semantics to `NeedsYouBell`: "Snooze until it needs me" on a conversation hides its progress but not its approvals or questions. This is a per-conversation flag in the daemon, not a UI-only state, or it will disagree after a restart.
4. Show the one-line *what it is doing* under the name when busy: Retinue already computes it (`ui/doingNow.ts`: "Running npm test", "Editing App.tsx"); make it the second line for a working row instead of the time.

Extends: `ui/ProjectsSidebar.tsx`, `ui/ConversationsPanel.tsx`, `ui/TeamSidebar.tsx`, `ui/NeedsYou.tsx`, `ui/doingNow.ts`, `ui/sidebarOrder.ts`.

### 2.4 Approvals and questions from an agent

**What the best do.**

- **Claude Code** uses a card triad — **Allow once**, **Always allow**, **Deny** — and a plan gate that names the resulting mode: "Yes, and use auto mode", "Yes, manually approve edits", "No, keep planning" [docs]. **Zed** improves the repetition case with **Always for &lt;tool&gt;** and **Always for &lt;pattern&gt;** [docs].
- **Cursor** moves the decision up a level: **Settings → Agents → Approvals & Execution** with *Auto-review*, *Allowlist*, *Run Everything*, and a separate **Read Allowlist** with *System* / *Workspace* scopes [docs]. The prompt then appears only for what the policy cannot cover.
- **Warp** defines what to do when the model cannot proceed without several answers: an *agent notification* mailbox with **All tabs / Unread / Errors** and "Mark all as read", beside a per-tab attention badge [docs]. **Orca** docks the question card at the bottom of the pane, keeps a *Needs You* column in the fleet board, and lets the owner acknowledge in-window or in a pop-out (`NativeChatQuestionCard.tsx:238-283`, `AgentKanbanBoard.tsx:27-34`) [code].
- **Sculptor** keeps every question inline at the end of the transcript with an option pager and *Dismiss / Submit / Next*, and deliberately has **no** fleet-level decision queue [code].

**What Retinue should do.** Retinue is already ahead on the hard part: `ApprovalCard.tsx` has Allow once / Always allow / Deny with a *rememberable* request, the decision stays with the request, and `NeedsYou` lists everything oldest-first. Two borrowings:

1. **Pattern-level memory**, from Zed: an "Always allow" today is one saved rule per match key (`permission_rules`). Add a second, coarser choice on the card: "Always allow anything like this" that stores a *category* rule (the same categories `accept_edits` already uses), shown as a separate line so the owner sees the difference.
2. **A batch view for a pile-up**, from Warp/Orca, built on `NeedsYou`: when more than one request waits on the same bot, the bell's row shows "*2 more from Vex*" and opens that bot's conversation scrolled to the first unresolved card. No new decision surface: everything still decides on its own card.

Extends: `ui/ApprovalCard.tsx`, `ui/NeedsYou.tsx`, daemon `permission_rule_*` in `crates/retinue-storage/src/lib.rs`.

### 2.5 Showing what an agent is doing

**What the best do.**

- **Sculptor** has the richest status vocabulary in one pill: "Thinking…", "Streaming…", "Calling tools…", "Compacting…", "Stopping…", "Stopped", "Waiting for background tasks…", with task progress "3 / 8 · task", "Working on 2 tasks…", "X of Y done" (`useAgentStatus.ts:42-51`, `StatusPill.tsx:306-314`) [code].
- **Orca** collapses tool runs to one line — "{{count}} tool calls" with `aria-expanded` — shows a task list with *Completed / In progress / Pending* and a "Full task list" toggle, and uses `aria-live="polite"` for streamed tool text (`NativeChatToolRun.tsx:163-196`, `NativeChatTaskList.tsx:18-42,169`) [code].
- **Claude Code desktop** offers view modes **Normal / Thinking / Verbose** [docs] — one control for how much internal work is shown, rather than many expanders.
- **T3 Code** renders plans and native subagents as their own panels rather than prose, and keeps long output behind a stated limit [docs].
- **Sculptor** also has "Toggle tool call density — Switch between compact and expanded tool rows" in its palette (`builtinCommands/chat.ts:52-53`), and "Jump to bottom" / "Search within chat" [code].

**What Retinue should do.** Retinue's step rows already show live output and mask secrets (`ActivityDetails.tsx`, `ui/liveOutput.ts`, `design-language.md:165`). Close three gaps:

1. **One density control**, per conversation and remembered: *Compact* (one line per tool, as Orca) versus *Detailed* (today). Put it where Redesign already puts view controls: a small control in the conversation header, and a palette action.
2. **A task list row** when the agent maintains one: "3 of 8 done" with a fold, instead of a stream of separate tool rows (Sculptor/Orca).
3. **View modes for reasoning**, from Claude Code: *Normal* hides thinking, *Thinking* shows it. Retinue already receives reasoning from Claude and Codex adapters; a mode is cheaper than per-row expanders and matches the calm-interface rule.

Extends: `ui/ActivityDetails.tsx`, `ui/doingNow.ts`, `ui/conversation.ts` (`nestSubagentSteps`), `ui/SafeMarkdown.tsx` if reasoning renders there.

### 2.6 Reviewing what changed

**What the best do.**

- **Codex** has the best single pane: scopes **Unstaged / Staged / Commit / Branch / Last turn**, a repository selector showing *All repos*, hover-**+** inline comments, *Stage all*, *Revert all*, and a `/review` command [docs].
- **Orca** makes diff notes first-class: "Add note for the AI" on a line, cards marked *sent*, then "Send notes to" an agent or a new agent (`DiffCommentPopover.tsx:52-55`, `DiffCommentCard.tsx:147`, `ReviewNotesSendMenuContent.tsx:239-256`), with shortcuts *Add Review Note* (Mod+Shift+A) and *Send Review Notes to Agent* [code].
- **Conductor** treats the workspace as the unit: a **Diff Viewer** (`Cmd+Shift+D`), inline comments that "become composer attachments", a **Checks** tab (git status, CI, deployments, comments, todos), *Create PR*, merge, then archive — and it "recommends actions as a workspace moves toward merge", so the next step is a button [docs].
- **Sculptor** shows a live diff summary "+N −N · N files" in the header that jumps to the Changes panel scoped "vs-target-branch", with Flat/Tree, Unified/Split, Wrap lines, and a `Review All` combined diff (`DiffSummary.tsx:15-55`, `DiffViewerMenu.tsx:33-69`, `ReviewAllPanel.tsx`) [code]. Its PR button is a live pipeline chip: "Create" / "merged", "Merge queued", "Pipeline running/passed/failed", "Approved", "Review pending" (`PrButton.tsx:180-218`).
- **Amp** orders files by importance in its Changes pane with *Ship / Review / Sync* [docs]; **Windsurf** had a per-prompt revert arrow and hover-revert checkpoints [docs].

**What Retinue should do.** Retinue already has the pieces: `ChangesView.tsx` with `DiffBody.tsx` (unified and side by side, line comments), per-file undo in `SessionRailExtras.tsx`, `PullRequestDialog.tsx`, and the rail's "Create PR always asks first" rule. Three improvements, all connecting existing parts:

1. **Scope the diff like Codex.** The review screen gets four scopes — *This turn* (from `checkpoints.rs`), *Uncommitted* (today), *Branch*, *Last commit*. The data exists (`session.git.diff`, checkpoints); only the selector is missing.
2. **Make line comments a draft set with one send.** Today a comment goes back to the conversation as a message; Orca's pattern is a **notes tray**: collect comments across files, count them in the review header, then *Send notes to Vex* (or *a new bot*). This is the single strongest review idea in the survey and it fits `ChangesView` exactly.
3. **A next-step chip on the rail**, from Conductor: when the tree is clean and the branch is ahead, the rail's primary action reads *Create PR*; when checks fail, *Fix failing checks* (which pre-fills a message); when nothing is committed, *Review changes*. One primary action, chosen by state.

Extends: `ui/ChangesView.tsx`, `ui/DiffBody.tsx`, `ui/SessionRail.tsx`, `ui/SessionRailExtras.tsx`, `ui/PullRequestDialog.tsx`, daemon `git_ops.rs`.

### 2.7 Starting work

**What the best do.**

- **Orca** has the best single card: always-visible sections in order — Project (with a *Run on* host) → name → agent → Advanced, where Advanced hides branch name, note and sparse checkout; the submit button says "Create worktree" or "Create workspace"; creation shows a checklist "Prepare workspace → Fetch base branch → Create worktree → Set up workspace" with an inline failure and Retry/Dismiss (`NewWorkspaceComposerCard.tsx:292-331`, `WorktreeSetupSteps.tsx:14-20`, `WorktreeCreationPanel.tsx:80-106`) [code]. Its name section can start from a GitHub/GitLab/Linear/Jira item (`NewWorkspaceComposerNameSection.tsx:108-112`).
- **Sculptor** asks for the minimum and reveals the rest: repo → source branch → mode (*Worktree* / *Clone* / *In-place*) → name → prompt, and **agent, model, effort appear only once a prompt is typed** (`NewWorkspaceForm.tsx:668,733`; `ModeSelect.tsx:17-19`) [code].
- **T3 Code** keeps a *No project* escape hatch and creates scratch folders, with "Turn into project" later (`docs/user/thread-sidebar.md`) [docs]; **Codex** has **Handoff** to move a chat between Local and Worktree and keep the same worktree afterwards [docs].
- **Conductor** makes **New workspace** (`Cmd+Shift+N`) the only entry, and every workspace is a worktree [docs].

**What Retinue should do.** Retinue's first message already carries project, model, effort and mode, but the *choices* are spread across the composer's pills and the new-conversation row. Adopt Orca's ordering and Sculptor's reveal:

1. The new-conversation composer shows two rows only: **project** (with the employee already chosen by context) and the message. Branch/worktree, model, effort and permission sit behind one "Options" disclosure that remembers its last state — the same `Select` components used in the hire dialog.
2. Show the creation checklist Retinue already performs in the daemon (`project_setup.rs`: copy files, submodules, script) as four labelled steps in the conversation, with the failure and a Retry — instead of one spinner. This is a UI for work that already happens.
3. Keep the composer's "Send" honest with a one-line summary under the box only when a non-default choice is active: "New copy of retinue · branch retinue/fix-queue · Sonnet 5.5 · Ask".

Extends: `ui/App.tsx` (new-conversation state), `ui/ComposerControls.tsx` (`PermissionModeControl`, model pill), `ui/SessionExecutionControls.tsx`, `ui/ProjectSources.tsx` flows.

### 2.8 Delegation and orchestration

**What the best do.**

- **Conductor** says it in one line: "the workspace is the unit of delegation" — each workspace has its own worktree, branch, terminal and review path, listed in a sidebar so you "see at a glance what they're working on", with a Checks tab and an archive [docs].
- **Factory** shows two shapes for the same work: **Workstreams** (centre chat with the orchestrator, right pane **Agents** grouped *Waiting on you / Working / Idle*, plus PRs and Tasks) and **Mission Control** (workers on the left, a milestone view in the middle, a progress log on the right, with model dropdowns for Orchestrator / Worker / Validator) [docs]. Its autonomy ladder (**Off → Low → Medium → High**, `Ctrl+L`) is set per run.
- **Orca** draws the crew inside the pane: an *Orchestration* card with **Managed by**, **Managing**, **Talking with**, a "Mention all" and a count, plus tree connectors (`role="treeitem"`, `aria-level`) in its dashboard rows and "Open parent conversation" links (`OrchestrationCrewPanel.tsx:258-347`, `DashboardAgentRow.tsx:276-301`, `WorktreeCardAgents.tsx:335`) [code].
- **Sculptor** keeps subagents as pills with a popover ("Subagent") and workflows as a status popover with "No agents ran." when idle (`SubagentPopover.tsx:14`, `WorkflowPopover.tsx:24-27,269`) [code].
- **T3 Code** shows *delegated child approvals in the parent conversation* and renders native subagents as an Agents panel with status, model, effort, tokens and tools [docs].

**What Retinue should do.** Retinue's delegation UI is already the best of the open tools surveyed: a "Messaged" chip per turn, rows with recorded status, workspace/branch facts, Stop, Stop all, and a board with lanes. Do not add a tree view or a Mission Control. Four connections:

1. **Approvals from a delegated bot surface at the delegating conversation**, as T3 does: the `UpdateChip` already exists ("Approval" / "Question") but only appears as a notice; make it the same decision affordance as the side conversation's card, expanded in place. The owner should never have to leave the CEO's conversation to unblock the work they just asked for.
2. **The brief is readable from the row.** `DelegationRecord` already carries `briefPreview` and `doneGoal` (`ui/delegations.ts:33-34`); the expanded row shows them as two lines ("Asked for …" / "Done when …"). This is what makes "Checked:" meaningful to the owner.
3. **A result line with a verdict**, from delegate-skills: when a reply starts with "Checked:" or "Not done yet:", the reply chip shows that word as its label, so a scan of the CEO's conversation shows which results were verified.
4. **A crew count in the rail**: Orca's "Managed by / Managing" as one row in the CEO's session rail — "Managing 3 · Vex, Coco, Dex" — linking to the board. It is the same data the "Messaged" chip holds, in the place the owner already looks for state.

Extends: `ui/DelegationGroup.tsx`, `ui/TeamMessages.tsx` (`UpdateChip`, `ReplyChip`), `ui/delegations.ts`, `ui/SessionRailExtras.tsx`, `ui/DelegateBoard.tsx`.

### 2.9 Command palette, shortcuts and quick switchers

**What the best do.**

- **Orca**: *Switch worktree* on Mod+K opens a "Jump to…" palette grouped into Tasks, Open Tabs, Workspaces, and Actions & Settings, footer "Enter Open / Esc Close / Tab Filter" (`worktree-jump-palette-surface.tsx:39,162-169`, `use-worktree-jump-palette-list-entries.ts:74-169`) [code].
- **Sculptor**: Cmd+K command palette with drill-down pages ("Workspace actions…"), commands like "Save current arrangement as layout…", empty state "No matches for …" (`CommandPalette.tsx:484,642`, `builtinCommands/navigation.ts:56-83`) [code].
- **T3 Code**: `mod+shift+…` chords for model, effort, permissions, workspace, branch, host; `mod+Shift+Enter` = "Queue: Send First Queued Message"; `mod+Alt+Enter` = send and start a new thread; `Ctrl+Shift+1…4` period filters on Usage; every binding rebindable (`docs/user/keybindings.md`) [docs].
- **Warp**: `⌘+\` opens Warp Drive (Workflows, Notebooks, Prompts, Environment Variables) — saved prompts as a first-class object [docs].

**What Retinue should do.** Retinue has a quick switcher (`Ctrl+K`, `app.quickSwitcher`) and eight rebindable actions (`ui/shortcuts.ts`), plus `Ctrl+/` for the sheet. Two upgrades:

1. **Make the switcher a palette**: the same Ctrl+K surface should also list actions (new conversation, open tasks, toggle panel, usage, settings) and search conversation titles *and* messages, grouped like Orca's. No new shortcut; same component, more rows.
2. **Saved prompts in the composer's slash menu**, from Warp Drive: a small list stored with the project (`.retinue.json` already carries project settings), inserted by `/`.

Extends: the quick switcher in `ui/App.tsx`, `ui/shortcuts.ts`, `ui/settingsIndex.ts` for search parity.

### 2.10 Small things that make these apps feel smooth

**What the best do.**

- **Optimistic rows everywhere**: T3 draws a queued turn dashed and unfilled until the server acknowledges it (`QueuedRunsControl.tsx:452-460`); Orca's queue list has an in-flight test (`NativeChatQueuedMessageList.in-flight.test.tsx`); Sculptor renders a workspace before validation (`WorkspacePage.tsx:64-67`).
- **Skeletons that match the shape of the thing**: Retinue already does this (settings rows, archived bots); Sculptor has one for its sidebar and branch field; Orca for full pages.
- **Empty states that are a launcher**: Sculptor's empty section offers "Add panel", "Quick add", "New Claude", "New terminal" (`EmptySectionState.tsx:88-139`) — an empty pane teaches the next action.
- **Undo for state changes, not just text**: T3 gives every pin/settle/snooze/archive a five-second Undo toast, and `mod+z` triggers the most recent one (`docs/user/thread-sidebar.md`) [docs]; Sculptor opens an undo dialog when an edit races a send.
- **Scroll behaviour during streaming**: Sculptor disengages auto-scroll when the reader scrolls away and remembers position per agent (`ChatInterface.tsx:204-225`); Retinue does this in `App.tsx` for the conversation.
- **Focus handling**: Warp's queue keeps the composer focused and offers "⏎ to send"; Zed's per-message Steer keeps the toggle on the message [docs]. Retinue's popovers already return focus (`usePopoverDismiss`).
- **Draft persistence**: Retinue's `draftStore.ts` keeps a draft per conversation and per bot and holds sends; T3 keeps the previous composer text while a queued message is being edited.

**What Retinue should do.** Three small ones, all in the calm-interface spirit:

1. **Undo toasts for state changes** already half-exist (file undo, queued-message confirm). Add them where the owner can lose something with one click: archive a bot, delete a task, stop all assignments. Five seconds, one line, `mod+z`-able, as T3 does.
2. **A launcher empty state** for a conversation with no messages and for an empty project: two or three sentence-case choices ("Ask Vex to…", "Start from a task", "Open the board"), not a blank panel.
3. **A single "what changed" line under the composer** when a non-default choice applies to the next message (see 2.7), so the owner never wonders which model or mode a message will use.

Extends: `ui/App.tsx` (toasts), `ui/TasksView.tsx`, `ui/TeamSidebar.tsx`, `ui/EmptyState`-style markup in the conversation pane.

## 3. Part 2: delegation

### 3.1 amElnagdy's `delegate-skills` [code]

Repository: https://github.com/amElnagdy/delegate-skills at `7f8ae3435e5bc68a54efdca9f67797e905845562` (MIT). It is a Skills CLI package: 19 implementer skills (`claude-delegate`, `codex-delegate`, `opencode-delegate`, `cursor-delegate`, …) plus `delegate-setup`. Each implementer skill shares one shape — `SKILL.md`, `references/writing-the-brief.md`, `dispatch-and-poll.md`, `review-and-land.md`, `multi-task-queues.md`, `scripts/relay.mjs` — and the reference files are near-identical across implementers, differing only where a CLI's flags differ. I read `claude-delegate` in full, the shared references, `relay.mjs`'s contract, `delegate-setup/SKILL.md` and the repository's `AGENTS.md`.

**The idea.** The orchestrating agent stays the orchestrator. A separate CLI process is the *implementer*: it edits a real working tree, and the diff is the deliverable. The orchestrator writes the brief, dispatches it through `relay.mjs`, waits, reviews the diff and the gates, and **commits itself** — "Committing belongs to the reviewer, always" (`README.md:118-121`). The controlled vocabulary is fixed (orchestrator, implementer, brief, gates, dispatch, land, lane, fleet) and the repository's `AGENTS.md` forbids inventing synonyms (`AGENTS.md:26-60`).

**How a brief is structured.** The brief is self-contained XML-ish blocks (`references/writing-the-brief.md:35-75`):

- `<task>` — the concrete job, current and desired behaviour, where it lives, what must remain untouched, and any fact the implementer cannot discover from the tree.
- `<repo_constraints>` — the load-bearing rules copied from `AGENTS.md`, restated even though `CLAUDE.md` loads automatically, because "Claude Code does **not** generically auto-load `AGENTS.md`" (`writing-the-brief.md:20-30`).
- `<verification_loop>` — the *exact* commands to run, not "run the tests".
- `<action_safety>` — no unrelated cleanup, no `git add`/`commit`/`push`, no nested delegation, leave it uncommitted.
- `<structured_output_contract>` — end with: what changed and why; files touched; gate outcomes with counts; deviations and open questions.

"Remove empty blocks rather than adding ceremony" (`writing-the-brief.md:77`). Optional blocks exist for debugging (`<completeness_contract>`, `<missing_context_gating>`), read-only diagnosis (`<grounding_rules>`), and migrations (a repository-wide search and round-trip requirement).

**What context is passed and withheld.** Passed: the brief on stdin (never argv), and whatever the implementer can read in the target tree. Withheld: orchestrator chat history (there is none), shared memory, and — deliberately — MCP servers, skills, slash commands and Claude's own Agent tool, which the relay disables for the child (`SKILL.md:96-108`). The orchestrator must read `AGENTS.md` itself and copy the rules in; the implementer can read a file the brief points to, but nothing is assumed.

**How the task is bounded.** One time box per brief: "One brief → one separate Claude session → one reviewed commit keeps scope and rollback clear" (`writing-the-brief.md:110-115`). Mixed requests are split into separate dispatches; a resumed session is only for rework on the same task. The relay enforces tool surface (`--tools Read,Glob,Grep,Edit,Write,Bash`), permission mode, deny rules for `git commit`/`push`/nested `claude` (`SKILL.md:120-134`), optional `--max-turns`, `--max-budget-usd` and a `--timeout` watchdog (`references/dispatch-and-poll.md:31-46`). Autonomy is stated "in the CLI's own terms", and whatever it cannot enforce is said plainly — the read-only profile's tripwire is explicitly "a reporting tripwire, not an OS boundary" (`dispatch-and-poll.md:96-104`).

**How the result returns and is reviewed.** The relay blocks, then writes artifacts outside the repository: `brief.txt`, `events.jsonl`, `final.txt`, `stderr.txt`, `profile.json`, and `result.json` in a stable contract `delegate-relay.result.v1` with `status` (`completed | failed | timeout | aborted | claude_unavailable`), `exitCode`, `signal`, the implementer's `finalMessage`, `touchedFiles` (final `git status`, explicitly "the whole final tree, not attribution"), `sessionId`, `numTurns`, `usage`, `totalCostUsd`, and a tri-state `readOnlyViolation` (`dispatch-and-poll.md:106-140`; `relay.mjs:128`). The review is a fixed procedure: review test edits before trusting a green gate; re-run the gates yourself; inspect the complete tree including untracked and staged files; hold the diff against the brief for scope creep and shortfall; run an "implementer sweep" for hardcoded success, broad catches, absent APIs, duplicate idioms and tests that assert implementation details (`references/review-and-land.md:5-72`). "A green gate proves less if the implementer shortened the yardstick" (`review-and-land.md:19`).

**Failures and timeouts.** Usage errors exit 2 with no `result.json`; a missing CLI exits 127 with `status: "claude_unavailable"`; timeout and signals terminate the whole process tree and still write an outcome; the working tree may hold partial edits and must be inspected, never cleaned, before resume or re-dispatch (`dispatch-and-poll.md:150-170`). Rework resumes the same session with a `<review_delta>` that says only what changed (`writing-the-brief.md:117-140`). A queue keeps a progress file (status, commit hash, per-task review, "needs your eyes", session ids) and closes with a coherence gate: full gates, repository-wide search for the renamed concept, migrations replayed, then push and PR (`references/multi-task-queues.md:1-75`).

**When not to delegate.** Four rules (`SKILL.md:32-40`): the human asked the current agent to implement directly; the task is small enough to do inline and nobody asked; the CLI is missing or unauthenticated; the task needs a stronger host boundary than the CLI's own permissions. `delegate-setup` adds a fifth: setup never dispatches work, and it asks global-versus-this-repo before writing (`delegate-setup/SKILL.md:37-46`).

**The fleet idea.** `delegate-setup` proposes named lanes — a kind of work bound to an implementer and optional dials, e.g. **feature** → OpenCode, **tests** → Codex, **ui** → Cursor — shows the lane table *and* the JSON, and writes only on explicit approval (`delegate-setup/SKILL.md:12-60`). "Prefer 3–5 useful lanes over a kitchen-sink map." A lane binding is content-bound to that approval, so an edited project file fails closed until re-approved (`README.md:40-50`).

**What it deliberately does not do.** It never steers a running implementer ("There is no steering channel while the relay is running", `writing-the-brief.md:117-120`); it never commits; it does not parallelise into one tree ("Multiple implementers editing one tree destroy attribution", `multi-task-queues.md:18-21`); it is complementary to native subagents, not a replacement.

### 3.2 How Retinue delegates today [code]

**The CEO.** One built-in bot has `system_role='manager'`, shown as the CEO. Its standing instruction is one paragraph (`crates/retinue-daemon/src/main.rs:1032`):

> "You are the CEO. Your team works in projects. … Delegate with message_blob by writing a self-contained brief. … If a request is vague, ask one clarifying question instead of inventing scope."

and its per-turn instructions add the delegation rules (`crates/retinue-daemon/src/team.rs:478-488`):

> "A message to a teammate is not consequential: `message_blob` is sent at once with no approval, so use it for ordinary delegation and only use `assign_task` when the user wants the work tracked as a task. Fill the brief or message yourself, and for a task also doneGoal and reportBack; never ask the user to fill a task form. … After delegating, say briefly who will do what and end your turn."

> "Before you report a finished assignment, compare the reply with its 'Done when' line and start your report with 'Checked:' or 'Not done yet:' and why."

**Every bot** gets the generic rule (`team.rs:498`):

> "When the user tags a bot with @Name, or asks you to hand work to someone, write that teammate a self-contained brief (they cannot see this conversation: include the goal, context, paths, constraints and what to send back) and send it with `message_blob`. Then tell the user in one short sentence who you asked and end your turn: do not wait, poll or sleep."

**The playbook.** The CEO's editable playbook (`crates/retinue-storage/src/lib.rs:901-965`) covers how the user works, project onboarding, hiring and ground rules; it says nothing about how to write or review a brief. Its "Ground rules" say only: ask before hiring, creating a project, creating a bot or assigning a task; never store secrets; keep locked preferences; prefer durable facts; keep the playbook short.

**The tools.** `message_blob` (`team.rs:576`) takes `blob`, `message` ("A self-contained brief: goal, context, paths, constraints and what to send back"), optional `doneGoal` ("relayed back with the reply as a 'Done when' line"), `topic`, `conversation`, `project`. `assign_task` (a proposal, `crates/retinue-daemon/src/proposals.rs:200-215`) requires `brief`, `doneGoal` and `reportBack` and waits for the owner's approval; `create_task` puts an unassigned draft on the board (`team.rs:591`); `spawn_subbots` (`subbots.rs:24-28`) starts one to six hidden sub-bots, each with "a self-contained `brief`" and a model.

**What the daemon does with a brief.** `delegate_with` (`team.rs:936-1100`): deduplicates a replayed call by turn + text; refuses a self-message; refuses a target that is already waiting upstream (loop guard, `:964-966`); refuses a hidden, locked or unusable target; caps depth (`MAX_DEPTH`, `:975-976`); refuses while the owner's global delegation pause is on (`:994-996`); refuses a target that "failed twice in a row with the same error" (`:997-1001`); opens or continues a side conversation, giving a code-writing delegation its own worktree when another task is in the same repo (`:1032-1044`); records a ledger row with `doneGoal` and a `briefPreview` (`crates/retinue-daemon/src/delegations.rs:3-10`, `team.rs:1076-1081`); and records whether delivery started or queued and why ("busy" or "slots", `team.rs:1063-1067`).

**How the result comes back.** On the side turn's end, the daemon claims the reply once (`delegations.rs:54-61`), relays it into the CEO's conversation as `[Reply from NAME]` followed by the reply and a `Done when: …` line (`delegations.rs:232`, `team.rs:1410-1418`), and wakes the CEO (`offer_wake`, `delegations.rs:820`). A failed or cancelled turn relays the daemon's own sentence instead (`team.rs:1401-1408`). A restart re-offers an unsent reply (`delegations.rs:881`). The side conversation's own instructions include the interaction rules I was given (untagged above): notify the owner when a teammate is waiting on an approval, and never answer for them (`team.rs:483`).

**What the owner sees.** In the CEO's conversation, one centred "Messaged [arts] Vex, Coco +2 · 4 done · 58s" chip per CEO turn, opening in place to one row per bot with art, status word (Queued, Working, Needs approval, Done, Failed, Cancelled, Interrupted), elapsed time, Open and Stop (`apps/desktop/src/ui/DelegationGroup.tsx:1-40`; rules in `design-language.md:153-157`). A teammate's reply arrives as a message with a "Message from" header; a question from a teammate reads "Has a question" (`design-language.md:167`). The task board has an Unassigned list and one lane per bot, with drag-to-assign (`ui/DelegateBoard.tsx:1-30`). The bell lists every waiting approval, question and proposal across bots, oldest first (`ui/NeedsYou.tsx`).

### 3.3 The comparison

| Dimension | delegate-skills | Retinue today | Verdict |
| --- | --- | --- | --- |
| Brief structure | Five named blocks, each with a purpose; empty blocks removed | One sentence naming goal, context, paths, constraints, what to send back; no blocks | **Adopt the blocks, lightly** |
| What the brief must restate | Repo rules from `AGENTS.md`, exact gate commands, no-commit rule | Project context arrives through the conversation, but not the project's rules or gate commands | **Adopt** |
| Scope bounding | One task per brief; explicit "what must remain untouched" | No rule; the CEO may bundle | **Adopt** |
| Reporting contract | Four fixed sections, gate outcomes with counts | `reportBack` (task only) and the `Done when:` echo | **Adopt, as three lines** |
| Review before accepting | Tests reviewed first, gates re-run, implementer sweep, "surface, do not absorb" | The CEO is told to compare with "Done when" and start with "Checked:"/"Not done yet:"; no test-edit rule, no re-run rule (the CEO cannot run gates) | **Adopt the rule; route the gate re-run to the owner or a reviewer bot** |
| Rework | Resume the same session with a delta brief | `conversation` continues a side conversation, but nothing says to send only the delta | **Adopt the wording** |
| Failure and timeout | Terminal statuses, artifacts, process-tree kill, "preserve interrupted work" | Turn watchdog and `[Reply from]` failure sentence; no per-delegation deadline or explicit "partial work is preserved" line | **Partly adopt** |
| Loop and abuse guards | Not needed (one shot) | Upstream-wait guard, repeated-failure guard, depth cap, dedupe, global pause | **Retinue is ahead** |
| Visibility for the owner | Progress file the human can read | Ledger + "Messaged" chip + board + bell | **Retinue is ahead in UI, behind in content** |
| Choosing an implementer | Lanes (feature→OpenCode, tests→Codex, ui→Cursor) | Hire a bot; the CEO routes by role, description, tags | **Concept differs: bots are people, not lanes — keep bots, borrow the "lane" idea as routing knowledge** |
| Commit boundary | Never the implementer | Not stated to bots at all | **Adopt one sentence** |

**Where Retinue is already stronger.** The skills need the human to be the reviewer; Retinue has a CEO who can route, a durable ledger with status words the owner can read, a board, a bell, and guards against loops that a fire-and-forget relay never needs. The gap is not machinery. It is the *content of the brief*, the *shape of the report*, and the *review step*.

### 3.4 What Retinue's delegation should adopt

**1. A brief with named parts, in the tool description.** `message_blob`'s `message` description becomes:

> "A self-contained brief. Use these lines, and leave out a line only when it truly does not apply: **Task** — the concrete job, current and desired behaviour, and where it lives. **Do not touch** — what must stay as it is. **Context** — facts from this conversation the teammate cannot see. **Rules** — the project's own rules that matter (from the project's instructions or `.retinue.json`), and the exact commands to run. **Report** — what to send back: what changed, files touched, command outcomes, anything left open. One task per brief: split unrelated work into separate messages."

**2. One task per brief and the commit boundary, in the generic delegation rule** (`team.rs:498`, currently "write that teammate a self-contained brief (they cannot see this conversation: include the goal, context, paths, constraints and what to send back)"):

> "Write a brief with these lines: Task, Do not touch, Context, Rules (the exact commands to run), Report. One task per brief; split unrelated work. A teammate must never commit or push: the owner reviews and lands the work. Then tell the user in one short sentence who you asked and end your turn: do not wait, poll or sleep."

**3. A report contract for the teammate, not just for the CEO.** Teammates today are never told what a good report is. Add to `runtime_instructions_in` for a side conversation (`team.rs` around the delegation block):

> "This conversation is a side conversation with a teammate. When you finish, report in this order and keep it short: what you changed or found; the files you touched; each command you ran and its outcome with counts; anything you could not do, any decision you made that was not asked for, and any open question. If the brief has a 'Done when' line, start with `Checked:` when it holds, or `Not done yet:` and why when it does not. Leave the work uncommitted; the owner reviews and commits. If you changed a test to make it pass, say so plainly and say why."

**4. A review step for the CEO** (`team.rs:487` currently only compares with the "Done when" line):

> "Before you report a finished assignment: if the reply mentions a changed test, a skipped or deleted test, or a weakened assertion, treat that as a failure until the owner agrees. Compare the reply with its 'Done when' line and start with 'Checked:' or 'Not done yet:' and why. Report the teammate's own command outcomes as their claims, not as your evidence: when you cannot run the gates yourself, say which command the owner should re-run. Surface decisions the teammate made that the brief did not ask for; never absorb them silently."

**5. Rework as a delta.** In the delegation paragraph:

> "For rework, continue the same conversation with `conversation` and send only the delta: what is wrong, what to keep, and the exact commands again. Do not restate the whole brief."

**6. The playbook gains a delegation section** (ground rules today say only "ask before … assigning a task"). Proposed addition after "Hiring a team":

> "## Delegating
> - One task per brief, with Task, Do not touch, Context, Rules and Report lines.
> - Name the exact commands the teammate should run; never "run the tests".
> - The teammate never commits. I review and report; the owner lands.
> - When a reply says a test changed, read that first, and say so in my report.
> - Rework goes back to the same conversation as a delta, not a new brief.
> - Surface what the brief did not ask for: a quiet decision is still a decision the owner should see."

**7. Requirements that match the words.** `message_blob`'s `doneGoal` is optional while `assign_task` requires it. If the brief template above is adopted, make `doneGoal` required on `message_blob` too: it is one string, it is already relayed back as the `Done when:` line, and the CEO's "Checked:" rule has nothing to check without it. This is a schema change in `team.rs:576` plus the app-side callers in `proposals.rs` and `subbots.rs`.

**8. What not to adopt.** The relay machinery (subprocess, `result.json`, process-tree kills) does not map: our bots are conversations the daemon already owns, and the turn watchdog plus the ledger already give us status. The lane probe that discovers installed CLIs is also unnecessary: `providers.rs` and `runtime.discover` already know what is installed. Take the *writing*, not the plumbing.

## 4. Part 3: Katana, by Max Blade

**The short answer: Katana is not an ADE.** It is a playable, browser-based, Counter-Strike-style multiplayer first-person shooter, published at **https://cnvs.dev/katana/** under the in-game title **"BANANA SHIPMENT"**, and it appears to be Max Blade's public demonstration of what his coding-agent environment, **CNVS**, can build. The ADE is CNVS (`https://cnvs.dev`). The owner's premise — that Katana is a coding-agent product in the same space as the others — does not hold on the evidence I could reach, so this section reports what Katana actually is, then what CNVS (the product around it) does that is genuinely relevant to Retinue.

### 4.1 What I saw myself

- I opened **https://cnvs.dev/katana/** and it served a working game shell: a "BANANA SHIPMENT" wordmark, a LOADING bar, then **PLAY / LOADOUT / SETTINGS** with a **CALLSIGN — EDIT** chip. The PLAY panel offers **QUICK PLAY — OFFLINE · VS BOTS**, **PRIVATE MATCH — CREATE A LOBBY · INVITE BY LINK**, **PUBLIC MATCH — ALWAYS-ON SERVER · DROP IN ANYTIME**, with **MAP** and **MODE** selectors. The same page carries a match HUD ("YOUR TEAM 0 / ENEMY 0", bomb-site flags **A / B / C** in NEUTRAL state, ammo "30/90", "FRAG GRENADE · HOLD TO COOK", "KILLCAM — KILLED BY —, SPACE SKIP", "MATCH COMPLETE / VICTORY") and a CC-BY attribution for the de_dust2 map by vrchris on Sketchfab. That is a first-person shooter, not a coding tool.
- I read **https://cnvs.dev/** (the product site) and its **release notes** page (`https://cnvs.dev/release/`). CNVS is described as "a calmer native Mac canvas for building software with AI agents": "Canvas runs Claude, Cursor, and Codex in parallel on one canvas: you direct, they build, you ship." Built in Swift for macOS, one-time licence ($99 founder, then $129, then $169 by licence number). The changelog runs to version 1.5.2, dated 20 September 2026, and **never mentions Katana**.

### 4.2 What is second-hand, and from where

- **"KATANA IS COMING…"** appears in the description of every Max Blade livestream from 2 to 10 November 2026, including a three-hour stream titled *"the last vibe coder builds katana (fable 5.5 waiting room live)"*, streamed 2 November 2026 (`https://www.youtube.com/watch?v=CNawclgolVM`; also `oBUXts0Jzqo`, `MFgdudsD9mY`, `Z5iM4EEKygE`, `HALe-cmm5yM`). I could not obtain a transcript: YouTube's caption track returned empty through every route tried, there are no chapters, and no transcript mirror worked. The title and description are the evidence.
- **From the shipped build's own configuration files** (read by the research pass, not by me in this session): five maps — SHIPMENT, DUST II, ARES RELAY, NUKETOWN, and **KATANA**, the last described as "A mountain shrine-town in three terraces. Shrine, sword, old town." — six modes (Team Deathmatch, Free-for-All, Domination, Search & Destroy, Elimination, Gun Game), 12 players by default and up to 16 on the Katana map, killcam, play-of-the-game replay, MVP card, killfeed, spray tags (`/katana/assets/prefs-lFFlVWDU.js`, `/katana/assets/modes-DfBH2j-s.js`). There are in-world billboards reading "THE FUTURE OF VIBE CODING", "CNVS.DEV" and "SHIP IT" (`/katana/assets/dust-world-DNFbQryV.js`).
- A third-party post from 5 November 2026 says "@_MaxBlade is working on katana right now, tune in" (`https://x.com/philcampbell/status/2118374545149087817`) — a search snippet only; the post itself would not render for me.
- The lineage is documented by Max himself: *"I vibecoded counterstrike with grok 4.6, opus 5, and fable 5 over 8 days of looping"* (`https://www.youtube.com/watch?v=63qCx8AyJbg`, 30 August 2026).

**What I could not determine.** Whether "Katana" is the game's title, the name of the map, or both. The shipped page is titled "BANANA SHIPMENT"; "KATANA" is also a map label with its own blurb; the release manifest names the deploy `katana-live`. There is **no public repository for Katana**; Max's GitHub account (`MaximilianHansen`) has no Katana repo. His prior browser game, **Kong Royale**, is public and MIT (`https://github.com/MaximilianHansen/kongroyale`), which is a different game. No release date, price, waitlist, platform list or monetisation is announced anywhere I could reach. Several plausible domains (`katana.cnvs.dev`, `getkatana.dev`, `playkatana.com`, `getkatana.gg`) do not resolve; `katana.gg` is a password wall, `katana.game` an online casino, `katana.studio` and `katana.app` are for sale. The Wayback Machine has no snapshot of `/katana`.

**So, plainly:** Katana is a teaser plus a working prototype of a browser shooter, built inside CNVS with coding agents, and it is best read as a *demo of the ADE*, not a new ADE. If the owner was told otherwise, the source was wrong.

### 4.3 What Katana's UI does that a coding tool could learn from

The game itself is not our business, but three interface decisions in it are unusually disciplined and transfer well:

1. **One primary action per screen.** The main menu offers three tiles and nothing else; the match HUD carries only what a player acts on. There is no dashboard of metrics.
2. **A call sign, not a user account.** The only identity is "CALLSIGN — EDIT". No sign-in, no profile, no settings tree before play. Our equivalent is a bot's name and portrait; the lesson is that identity should be one editable chip.
3. **Loading states say what is happening.** The boot screen shows the map name, a rules line, a progress bar with stage text and a rotating tip — the same discipline Retinue applies to skeleton rows.

### 4.4 What CNVS itself does that Retinue should study

The CNVS release notes are the strongest single document I read this round for the owner's stated goal ("interrupted only when needed, calm and polished"). Four mechanisms, all from `https://cnvs.dev/release/1-5-2-distraction-guard/` [public, first-party release notes]:

- **"One surface speaks, never two."** Every agent event picks exactly one place to appear: if you are looking at the agent's card, the card *is* the notification (its body changes, its edge lights up); if you are elsewhere in the app, a tile; if the app is in the background, one macOS notification; if you just typed into that agent, nothing at all. The bell and the spoken callout stay silent while you are looking at that agent. This is the exact rule Retinue lacks: today an approval can be a card in the conversation, a row in the bell, a notification and a phone push at the same time.
- **Notifications that cannot be missed, but do not nag.** A tile is "never deleted by time or by navigation"; a finished tile stands for a few seconds, then tucks into a chip on the command bar — "Skye is waiting", "5 agents waiting", with an amber dot if a question or failure is among them; a question never tucks away until it is answered; a tile leaves only when you focus that agent, click it, press ×, or *Clear all*.
- **Verbatim, never paraphrased.** The tile, the macOS notification and the phone all "quote the agent exactly" — the reply, the question with the CLI's own options, or the failure — because CNVS reads these "from the CLI's own transcript rather than guessing from the screen". Retinue does the same (it never scrapes a terminal), but its notices are often a summary it wrote; the rule is worth adopting for the *alert*.
- **A work title per agent, written on-device.** Every agent gets a short goal title — "Command bar redesign", "Auth token refresh" — from the request, not from the spinner verb, using Apple's on-device model with a quiet fallback. Retinue has `doingNow.ts` ("Running npm test"), which is a *tool* label; a *goal* title is a different, calmer thing for a sidebar full of bots. Also notable: a steering message no longer retitles the whole job — a bug class Retinue could have in its auto-titles (`experiments.rs`).

Two more, structural rather than visual: the **command bar and ⌘K palette are one element** (typing in the bar grows ranked results upward; ⌘K opens the same bar in browse mode), and **status comes from the harness's own report** ("idle, working, needs you") rather than from screen output and timers.

### 4.5 What is relevant to Retinue

- The **alerting rule** (one surface speaks) and the **never-expire tile with a chip** are the most directly useful ideas in this whole round for the owner's stated wish. They attach to `NeedsYou.tsx`, the daemon's `session.changed`/permission events, and the push path — not to a new surface.
- The **goal title** per conversation is a small daemon-side change (`experiments.rs` already auto-titles conversations; this is a second, shorter title derived from the *current* request) that improves every sidebar row.
- The **unified command bar** is a caution as much as an idea: Retinue's `Ctrl+K` switcher and its composer are separate, and merging them would fight the two-panel design. Take the *ranked results in the same surface* part, not the literal merge.
- Nothing in Katana — the game — is directly relevant. Say so plainly in any planning: it is a showcase, and the showcase's lesson is that a public, playable demo is how this crowd markets an ADE.

## 5. Part 4: the ideas most worth building next

Ranked for this owner: a small team of named bots on their own subscriptions, delegating work and wanting to be interrupted only when it matters, in a calm interface. Each idea extends something Retinue already has. Excluded by instruction: "Send now" into the running turn, and resume-at-reset (both already going ahead).

**1. Let one surface speak, once.** Today an approval can appear as a card in the conversation, a row in the bell, a badge in the sidebar and a phone push, all for the same event. Make the rule explicit, as CNVS does: if the owner is looking at that conversation, the card *is* the alert and nothing else fires; if they are elsewhere in Retinue, one row in `NeedsYou`; if the window is in the background, one OS notification carrying the bot's own words; if they typed into that bot in the last minute, nothing at all. *Best done by:* CNVS's Distraction Guard (one surface per event, no timer-based duplicates) and Factory's *Waiting on you* bucket. *Extends:* `ui/NeedsYou.tsx`, `ui/ConnectionBanner.tsx`, the daemon's permission and `session.changed` events, `remote/push.rs` (which already knows "never twice" for a device). **M**

**2. Order the sidebar by attention, and fold the working ones.** Sort each project's conversations Needs you → Working → done-but-unread → the rest, and when three or more are working, collapse them under one "Working · 3" row that pops one out the moment it needs an answer. *Best done by:* Claude Code's agent view (grouped states with one-line summaries) and T3 Code's **Working** shelf that returns a thread to the top when it finishes, fails or needs an answer. *Extends:* `ui/ProjectsSidebar.tsx`, `ui/ConversationsPanel.tsx`, `ui/Collapse.tsx`, `ui/sidebarOrder.ts`. **S–M**

**3. A notes tray for diff comments, sent as one message.** While reviewing, "Add note" on a line collects the comment in a tray with a count in the review header; one button then sends every note to the bot (or to a new one) as a single numbered message. Today each comment goes back as its own message. *Best done by:* Orca (notes marked *sent*, "Send notes to" an agent or a new agent) and Codex (hover-+ inline comments). *Extends:* `ui/ChangesView.tsx`, `ui/DiffBody.tsx`, `ui/commentsMessage` (already assembles the text), `ui/SessionRail.tsx`. **S–M**

**4. Answer a delegated bot's approval without leaving the delegating conversation.** A delegated bot that stops for permission should be unblocked where the work was asked for: the "Approval" chip in the CEO's conversation expands into the same card the bot's conversation shows, with the same choices, and the decision is recorded once. *Best done by:* T3 Code (a child's approval card renders in the parent, "Approval needed from {title}", capped and scrollable) and Factory's *Waiting on you* pane. *Extends:* `ui/DelegationGroup.tsx`, `ui/TeamMessages.tsx` (`UpdateChip`), `ui/ApprovalCard.tsx` (reused in place), `crates/retinue-daemon/src/delegations.rs` (the ledger already knows the originating turn). **M**

**5. Show what was asked and whether it was checked, on the delegation row.** The expanded row gains two lines — *Asked for …* and *Done when …* — from `briefPreview` and `doneGoal`, and a reply that starts with "Checked:" or "Not done yet:" wears that word as its chip label. The owner can then scan a CEO conversation and see which results were verified. *Best done by:* the delegate-skills reporting contract (section 3.1) and T3's relationship labels ("Subagent", "Parent agent"). *Extends:* `ui/DelegationGroup.tsx`, `ui/TeamMessages.tsx` (`ReplyChip`), `ui/delegations.ts` (both fields already exist). **S–M** (the daemon side is the wording changes in section 3.4).

**6. A per-turn review card after every finished turn.** When a turn settles, the rail shows "Changed 3 files · +12 −4" with *Review*, *Undo*, *Keep* and *Commit*. Undo stays per file, as the design rules require; the card is the place to see the turn as a unit. *Best done by:* Monocode's `SessionReview` card ("Changed 3 files", Undo / Keep / Commit / Review, "Show 2 more files") and Superset's turn card with *Undo* / *Review* / *Show fewer files* and plain-language refusals ("Can't undo while a chat in this workspace is working"). *Extends:* `ui/SessionRailExtras.tsx` (Changes by turn), `ui/SessionRail.tsx`, `crates/retinue-daemon/src/checkpoints.rs`. **M**

**7. Scope the diff: this turn, uncommitted, branch, commit.** Add the four scopes to the review screen, defaulting to *This turn* right after a turn and *Uncommitted* otherwise. *Best done by:* Codex (Unstaged / Staged / Commit / Branch / Last turn) and T3 ("Latest turn", "Turn N", "Commit &lt;subject&gt;"). *Extends:* `ui/ChangesView.tsx`, `ui/DiffBody.tsx`, daemon `git_ops.rs` and `checkpoints.rs` (both already produce the data). **M**

**8. A goal title on every conversation row.** Not the tool ("Running npm test") but the job ("Fix the queue pause"): a short title written from the owner's own request, on-device if possible, falling back to the first line. *Best done by:* CNVS 1.4.0's Shorthand (per-agent work title from the request, on-device, with a silent fallback; a follow-up message must not retitle the job). *Extends:* `ui/doingNow.ts`, `ui/ProjectsSidebar.tsx` second line, `crates/retinue-daemon/src/experiments.rs` (auto-titles already exist). **S**

**9. Snooze until it needs me.** A conversation can be snoozed: no progress updates, no chips, no pushes — but the first approval, question or failure wakes it immediately. *Best done by:* Amp (*Snooze Until I'm Needed*, with the thread reappearing in the sidebar while it waits for a decision). *Extends:* `ui/ConversationsPanel.tsx` row menu, `ui/NeedsYou.tsx`, a per-conversation flag in the daemon (`sessions.data`) so a restart cannot disagree. **S–M**

**10. Let the quick switcher also be the action palette.** `Ctrl+K` today finds conversations; make the same list carry actions (new conversation, open tasks, toggle panel, open usage, settings) and message search, grouped like Orca's *Tasks / Open Tabs / Workspaces / Actions & Settings*. *Best done by:* T3 Code's ⌘K with command, file and content modes, and Orca's grouped palette with an "Enter Open / Esc Close / Tab Filter" footer. *Extends:* the quick switcher in `ui/App.tsx`, `ui/shortcuts.ts`, `ui/settingsIndex.ts`. **S–M**

**11. Undo toasts for state changes.** Five seconds of Undo on the actions that can lose something with one click — archive an employee, delete a task, stop all assignments — with `mod+z` triggering the most recent one. *Best done by:* T3 Code (a notice with Undo on pin, settle, snooze, archive and draft discard, "⌘Z to undo"). *Extends:* `ui/TeamSidebar.tsx`, `ui/TasksView.tsx`, `ui/DelegationGroup.tsx` (Stop all), `ui/App.tsx` toasts. **S**

**12. A calm start: two rows, then Options.** The new-conversation view shows only the project and the box, with a launcher empty state ("Ask Vex to…", "Start from a task", "Open the board"); branch, worktree, model, effort and permission sit behind one *Options* disclosure that remembers its last state, and a one-line summary under the box appears only when a non-default choice is active. *Best done by:* Orca (one card, Advanced hidden) and Sculptor (agent controls appear only once a prompt is typed); the launcher is Sculptor's empty section ("Add panel / Quick add"). *Extends:* `ui/App.tsx` (new-conversation state), `ui/ComposerControls.tsx`, `ui/SessionExecutionControls.tsx`. **S**

**13. A density control for the work log, and one task row.** *Compact* shows one line per tool call (Orca's "{{count}} tool calls"), *Detailed* keeps today's rows; a plan or to-do list shows as one row — "3 of 8 done" — that opens. *Best done by:* Sculptor ("Toggle tool call density") and Orca (collapsed tool runs, task list with Completed / In progress / Pending). *Extends:* `ui/ActivityDetails.tsx`, `ui/conversation.ts` (`nestSubagentSteps`), `ui/SafeMarkdown.tsx`. **S–M**

**14. Saved prompts in the slash menu.** `/` already opens commands; add a small per-project list of saved prompts, inserted rather than sent. *Best done by:* Warp Drive (Workflows, Notebooks, Prompts as first-class objects) and T3's `t3.json` scripts for project-level actions. *Extends:* the slash menu in `ui/App.tsx`, `projects.overrides_json` in `crates/retinue-storage` (already read by `shared_settings.rs`). **S**

**The one I would do first.** Number 1, because it changes how the app *feels* more than any feature can, and it needs no new surface: the bell, the approval card and the push path already exist, and the rule is a routing decision in the daemon and a focus check in the window. Number 2 is the natural second: once only one surface speaks, the list must be ordered so that the surface is looking at the right conversation.

## 6. Sources

### 6.1 Repositories read (shallow clone, commit, licence)

| Product | Repository | Commit | Licence |
| --- | --- | --- | --- |
| T3 Code | https://github.com/pingdotgg/t3code | `17a6a01d84e3adf9f29df2639d6b6079a434e1b2` | MIT |
| Monocode | https://github.com/hardbeat920/monocode | `7e5a9e3690d5b96a7d93ebf884397835e57b9b7e` | MIT |
| Vibe Kanban | https://github.com/BloopAI/vibe-kanban | `b4d98330da74be564c4a566ce4430c3b8c09f8a9` | Apache-2.0 |
| Superset (Paradev) | https://github.com/superset-sh/superset | `45118b8c8fae14eb42399e8a68bbf854b1a44d29` | MIT |
| Crystal | https://github.com/stravu/crystal | `1e18e0bc981225f75b5226f82a300fa741970c6f` | MIT |
| Orca | https://github.com/stablyai/orca | `ddab3a6e1a42ee3979bd296caf096d5e8f3ff373` | MIT |
| Sculptor | https://github.com/imbue-ai/sculptor | `e7e054a446b7518f56b0c7f3bf3b7771c800002c` | MIT |
| delegate-skills | https://github.com/amElnagdy/delegate-skills | `7f8ae3435e5bc68a54efdca9f67797e905845562` | MIT |
| Nuphos | https://github.com/nuphos/nuphos | `efee2c028fcb5f97d57d2d036342a7eab12e4cd1` | Apache-2.0 |
| OpenBot | https://github.com/nightly-labs/openbot | `712e797303c5d41ee256c8360645718a0beb13b2` | PolyForm Noncommercial 1.0.0 |

Nuphos and OpenBot were studied in the first report (10 November 2026) and are not re-read this round; their commit hashes are repeated so the earlier citations stay checkable. Zed was read as documentation only, not code.

### 6.2 Documentation, articles, posts and videos

**Closed products (all [docs] unless stated)**
- Claude Code desktop — https://code.claude.com/docs/en/desktop.md · agent view https://code.claude.com/docs/en/agent-view.md · permission modes https://code.claude.com/docs/en/permission-modes
- Codex — https://developers.openai.com/codex/projects · code review https://developers.openai.com/codex/code-review · permission modes https://developers.openai.com/codex/permission-modes · worktrees https://developers.openai.com/codex/environments/git-worktrees · long-running work https://developers.openai.com/codex/long-running-work
- Cursor — https://cursor.com/docs/agent/overview · agents window https://cursor.com/docs/agent/agents-window · run modes https://cursor.com/docs/agent/security/run-modes · multi-agent https://cursor.com/help/ai-features/multi-agent
- Conductor — https://conductor.build/docs/concepts/workflow · review and merge https://conductor.build/docs/guides/review-and-merge · agent modes https://conductor.build/docs/concepts/agent-modes · diff viewer https://conductor.build/docs/reference/diff-viewer
- Warp — prompt queueing https://docs.warp.dev/agents/local-agents/interacting-with-agents/prompt-queueing/ · notifications https://docs.warp.dev/agents/capabilities/agent-notifications/ · permissions https://docs.warp.dev/agents/capabilities/agent-profiles-permissions/ · Warp Drive https://docs.warp.dev/knowledge-and-collaboration/warp-drive/ · blocks https://docs.warp.dev/terminal/blocks/
- Factory — https://docs.factory.com/factory-app/overview.md · worktrees https://docs.factory.com/factory-app/worktrees.md · steering https://docs.factory.com/workstreams/steer.md · missions https://docs.factory.com/missions/running-app.md · autonomy https://docs.factory.com/autonomy-and-safety/auto-run.md
- Amp — https://ampcode.com/docs/threads · tools https://ampcode.com/docs/tools · cli https://ampcode.com/docs/cli
- Windsurf — Cascade https://docs.windsurf.com/windsurf/cascade/cascade.md · agent command centre https://docs.windsurf.com/windsurf/agent-command-center.md · terminal https://docs.windsurf.com/windsurf/terminal.md. Note: Cascade is documented as legacy; Devin Desktop replaces it.
- Zed — agent panel https://zed.dev/docs/ai/agent-panel · parallel agents https://zed.dev/docs/ai/parallel-agents · tool permissions https://zed.dev/docs/ai/tool-permissions

**Katana and CNVS**
- CNVS product site https://cnvs.dev/ — [public, first-party]; read by me.
- Katana build https://cnvs.dev/katana/ — [public, first-party]; opened by me; title "BANANA SHIPMENT".
- CNVS release notes, especially 1.5.2 Distraction Guard (20 Sep 2026) https://cnvs.dev/release/ and https://cnvs.dev/release/1-5-2-distraction-guard/ — [public, first-party]; read by me.
- Max Blade livestream, "the last vibe coder builds katana (fable 5.5 waiting room live)", streamed 2 Nov 2026 https://www.youtube.com/watch?v=CNawclgolVM — [public]; title and description only, no transcript obtainable.
- Later streams with the same "KATANA IS COMING…" description: https://www.youtube.com/watch?v=oBUXts0Jzqo, `MFgdudsD9mY`, `Z5iM4EEKygE`, `HALe-cmm5yM` — [public].
- Channel https://www.youtube.com/@MaxBladeTv/videos — [public]; no Katana upload or trailer.
- Prior CS-clone video, 30 Aug 2026 https://www.youtube.com/watch?v=63qCx8AyJbg — [public].
- Third-party post about Katana, 5 Nov 2026 https://x.com/philcampbell/status/2118374545149087817 — [public]; search snippet only, the post would not render.
- Max Blade's reply card, 10 Nov 2026 https://x.com/_MaxBlade/status/2120115591193436515 — [unchecked]; could not render x.com.
- Max's GitHub https://github.com/MaximilianHansen and Kong Royale (MIT) https://github.com/MaximilianHansen/kongroyale — [public]; no Katana repository.
- Build configuration cited from the shipped bundle: `/katana/assets/prefs-lFFlVWDU.js`, `/katana/assets/modes-DfBH2j-s.js`, `/katana/assets/dust-world-DNFbQryV.js`, `/katana/release.json` — [public], read by the research pass, not by me directly.

**delegate-skills**
- Repository README and `AGENTS.md`; `skills/claude-delegate/SKILL.md` and `references/{writing-the-brief,dispatch-and-poll,review-and-land,multi-task-queues}.md`; `skills/delegate-setup/SKILL.md`; `skills/claude-delegate/scripts/relay.mjs` — [code], at the commit above. It is also listed on skills.sh: https://skills.sh/amElnagdy/delegate-skills.

**Retinue (our own files, [code])**
- `crates/retinue-daemon/src/main.rs:1032` (CEO instructions), `:2444-2452`, `:4531-4584`
- `crates/retinue-daemon/src/team.rs:459-503` (per-turn instructions), `:576` (`message_blob` schema), `:591` (`create_task`), `:936-1100` (`delegate_with`), `:1401-1419` (`reply_text`, `relay_body`)
- `crates/retinue-daemon/src/delegations.rs:1-70,232,820,881`
- `crates/retinue-daemon/src/queue.rs`, `retry.rs:221-261,637-653`
- `crates/retinue-daemon/src/proposals.rs:200-215`, `subbots.rs:24-28`
- `crates/retinue-storage/src/lib.rs:901-965` (`DEFAULT_PLAYBOOK`), `:4038-4125` (limit state, queue)
- UI: `apps/desktop/src/ui/App.tsx`, `PromptQueue.tsx`, `TeamMessages.tsx`, `DelegationGroup.tsx`, `DelegateBoard.tsx`, `ConversationsPanel.tsx`, `NeedsYou.tsx`, `ChangesView.tsx`, `DiffBody.tsx`, `SessionRail.tsx`, `SessionRailExtras.tsx`, `ActivityDetails.tsx`, `delegations.ts`, `busySend.ts`, `usageLimitSend.ts`, `shortcuts.ts`
- Rules: `.claude/skills/retinue-development/references/design-language.md`, `AGENTS.md`

### 6.3 What I could not verify

- **Live behaviour of every closed product.** Claude Code, Codex, Cursor, Conductor, Warp, Factory, Amp, Windsurf and Zed's agent panel were read from documentation; I did not run them. Any statement about motion, focus or exact placement in those apps is from their docs, and some of it is marked "inferred".
- **Katana's identity.** Whether Katana is the game, the map, or both is unresolved; no first-party sentence says so. No transcript was obtainable for any stream, so everything about Katana comes from titles, descriptions and the shipped build's configuration. There is no public repository, licence, price or date.
- **Acepe.** No product or documentation could be found under that name; it is the one item from the owner's list I dropped.
- **Screenshots.** I did not view any screenshot from a closed product directly; the UI descriptions rest on their documentation's text and on first-party posts.
- **Crystal and Superset naming.** `superset-sh/superset` presents itself as *Paradev* in its UI; `stravu/crystal` is the Crystal I read. If the owner meant a different product with either name, that is where the substitution happened.
- **Windows path limits.** Superset's checkout hit Windows path-length limits, so a few hundred very deep files were read through `git show HEAD:<path>` rather than on disk; commits and line numbers are still exact.
