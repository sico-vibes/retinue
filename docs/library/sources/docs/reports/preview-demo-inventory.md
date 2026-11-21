# Demo inventory: production app vs the browser preview

Read-only audit, 20 November 2026. Repository: `apps/desktop` (Retinue, Windows desktop app). Nothing in the repository was edited.

## Method

1. **Production surface.** Walked `apps/desktop/src` (`ui/`, `remote/`, `blob/`, `tauri.ts`, `types.ts`, `ui/errorCopy.ts`, `ui/companionStatus.ts`, `ui/ConnectionBanner.tsx`, `ui/App.tsx` overlay state and the `?open=` table at `ui/App.tsx:1688-1721`). Every production RPC name used by the UI (247 distinct names) and every daemon event type the UI matches on (about 290 literals) was extracted by script.
2. **Preview surface.** `src/preview/fixtureBridge.ts` (the `rpc()` switch, the `flags()` query switches, `notifyFixtureDaemon`), `fixtureTasks.ts`, `remoteFixture.ts`, `fixtureScenes.ts` (`?scene=` ids), `inspector/scenarios.ts` (about 105 scenario entries, including `phone-*`, `settings`, `dialog`, `companion`, `launch`), and the entry points `gallery.html` (launch, onboarding, dictation, companion, import, settings, characters), `launch-preview.html`, `dictation-preview.html`, `preview.html` (character sheet), `polish-preview.html`, `redesign-preview.html`, `inspector.html`, `inspector-sheet.html`.
3. **Diff.** Each production surface was matched to the preview route that renders it. RPCs and events were compared by name.

Two facts shape most of the table:

- **Unknown RPCs return an empty object, silently.** `fixtureBridge.ts:1857` (`return {} as T`) is the fallthrough of `rpc()`. Any production RPC with no branch (by exact name or prefix) resolves to `{}` with no error. The UI then shows an empty state or does nothing. A "Stop" that calls `session.cancel` (`ui/App.tsx:1256`, `:2402`) does nothing in the preview, with no error shown.
- **No live events.** `notifyFixtureDaemon` is called 49 times, and the emitted types are listed in the last section. The preview never emits `permission.requested`, `turn.completed`, `turn.error`, `message.delta`, `tool.changed`, `usage.updated`, `subbot.changed`, `vault.changed`, `owner_computer.grant` or `remote.native.*`. Live arrival of an approval, streaming, or a turn finishing can only be seeded as a static state.

Legend for the "Demoable" column: **yes** = a scene, URL flag or click path shows it in the preview today; **partly** = the surface renders, but the exact state, trigger or error cannot be produced; **no** = not reachable, or only as a silent empty state.

Path notes: where a component has no `export` line that could be cited, the file and symbol name are given instead. `preview` means the browser preview build.

## Inventory

| Area | Item (screen / state / error / function) | Where in prod (path:line) | Demoable in preview today? | What the preview needs to demo it | Needs daemon/backend support? |
|---|---|---|---|---|---|
| First run and launch | Launch intro: live scan then wave | `ui/LaunchIntro.tsx:22` | yes: `/launch-preview.html?fixture=live` (gallery "Launch") | none | no |
| First run and launch | Launch: scanning in progress | `ui/LaunchIntro.tsx:22`, `ui/launchChecks.ts` | yes: `?fixture=in-progress` | none | no |
| First run and launch | Launch: ready with warnings ("could not be confirmed") | `ui/LaunchIntro.tsx:149` (`LaunchWarnings`) | yes: `?fixture=done-warning` | none | no |
| First run and launch | Launch: service down (Retry, Continue offline) | `ui/LaunchIntro.tsx:22` (`serviceDown`) | yes: `?fixture=service-down`; the buttons only change local state | Retry and Continue do not re-run a real check | no |
| First run and launch | Launch intro inside the main window | `ui/App.tsx:310` (`launchVisible`) | yes: main `/` shows it unless `?no-intro` | none | no |
| First run and launch | Scan recovery (check again per provider) | `ui/ScanRecovery.tsx:64` | partly: rendered from the launch flow; re-scan results are not fixture-driven | a launch fixture state that re-scans with a result | no |
| First run and launch | Onboarding step 1: your name | `ui/FirstRunOnboarding.tsx:63` | yes: `/launch-preview.html?fixture=onboarding-step-1` | none (name save is local) | no |
| First run and launch | Onboarding step 2: coding agents found or missing | `ui/FirstRunOnboarding.tsx:63` (`scanning`, `error` props) | yes: `?fixture=onboarding-step-2`; "scan again" is a no-op in the fixture | a scan that returns results or an error | no |
| First run and launch | Onboarding step 3: first employee (create) | `ui/FirstRunOnboarding.tsx:63` (`onCreate`) | yes: `?fixture=onboarding-step-3` (creates a fake agent) | none | no |
| First run and launch | Onboarding: choose a look | `ui/FirstRunOnboarding.tsx:63` | yes: `?fixture=onboarding-look` | none | no |
| First run and launch | In-app first run on an empty install (no bots, no completion flag) | `ui/App.tsx:1131-1132` (`setOnboardingVisible(true)`) | no: the main preview always has bots; no fixture starts with an empty roster | a `?preset` that returns no agents and clears the completion flag | no |
| First run and launch | Onboarding error from scan | `ui/FirstRunOnboarding.tsx:63` (`error`) | no: the launch fixture passes `error={null}` | a fixture that passes an error to onboarding | no |
| Connection and service | Reconnecting banner ("Reconnecting to Retinue") | `ui/ConnectionBanner.tsx:60` | partly: `?disconnected` sets connection to disconnected (`ui/App.tsx:1022`, `:1330`), which drives the banner | none beyond the flag | no |
| Connection and service | Could not be reached (Retry) | `ui/ConnectionBanner.tsx:58` | yes via `?disconnected` (the banner's failed branch needs the retry path) | none | no |
| Connection and service | Back online confirmation | `ui/ConnectionBanner.tsx:55` | no: no fixture moves from disconnected back to connected | a timer that toggles the connection handler (`fixtureBridge.ts` `handler(false)` runs once only) | no |
| Connection and service | Service restarting (supervisor `restarting`) | `ui/ConnectionBanner.tsx:50-51` | no: `daemonSupervisorState` is always `running` (`fixtureBridge.ts:1893`) | a supervisor fixture state flag and `listenForDaemonSupervisor` events | yes (in UI; the preview stub needs a state) |
| Connection and service | Service gave up (supervisor `gave_up`, Restart button) | `ui/ConnectionBanner.tsx:46-48` | no: same as above | a supervisor fixture state of `gave_up` | yes (in UI; the preview stub needs a state) |
| Connection and service | Companion: daemon disconnected mood | `blob/companionFsm.ts`, `ui/companionStatus.ts:45` | partly: `?companion&disconnected` path through `ui/App.tsx:1022` | the same flag on the companion route | no |
| Connection and service | Companion: runtime offline / coding agent check failed moods | `ui/companionStatus.ts:46-47` | partly: runtime status `offline` appears in fixtures (3 hits) and `error` (6 hits) | a runtime fixture with `error` for the chosen bot | no |
| Connection and service | Companion: sleeping mood | `ui/companionStatus.ts:38-39` | no: no fixture uses `sleeping` | a session or idle clock past `SLEEP_AFTER_MS` | no |
| Main shell | Empty conversation (new chat) | `ui/App.tsx:656` (draft), `ui/ConversationsPanel.tsx:75` | yes: `/?scene=empty-conversation` (inspector "A conversation with nothing in it") | none | no |
| Main shell | No bots at all (empty roster) | `ui/App.tsx` roster selectors (`ui/rosterSelectors.ts`) | no: the fixture always returns agents | a `?preset` with an empty agent list | no |
| Main shell | Conversation list grouped by bot, project, status | `ui/ConversationsPanel.tsx:75`, `ui/ProjectsSidebar.tsx`, `ui/SessionRail.tsx:114` | yes: main `/` | none | no |
| Main shell | Machine groups in the list (other computers) | `ui/MachineBots.tsx` (`MachineGroups`) | yes: `/?machines=1` (inspector "machines-sidebar") | none | no |
| Main shell | Quick switcher (Ctrl+K style) | `ui/QuickSwitcher.tsx:31` | yes: `/?open=quick-switcher` | none | no |
| Main shell | More menu / create menu / profile menu | `ui/App.tsx` (`moreOpen`, `createMenuOpen`), `ui/ProfileMenu.tsx:42` | yes: `/?open=more-menu&session=session-codex&agent=agent-codex`, `/?open=create-menu`, profile by click | none | no |
| Main shell | Archived bots list (restore, delete) | `ui/ArchivedBots.tsx:9` | yes: `/?open=archived` | none | no |
| Main shell | Bot wizard (create a bot) | `ui/BotWizard.tsx:37` | yes: `/?open=bot-create` | none | no |
| Main shell | Bot editor (edit a bot) | `ui/BotEditor.tsx:19` | yes: `/?open=bot-edit` | none | no |
| Main shell | Bot page (overview, sessions, settings) | `ui/BlobPage.tsx:22`, `ui/BlobSettings.tsx:51`, `ui/BlobSessions.tsx:12` | yes: by click; no `?open` route | a click path (inspector has no scenario) | no |
| Main shell | Archive a bot | `ui/App.tsx` (`archiveTarget`), `ui/ConfirmDialog` in `ui/BlobPage.tsx` | yes: `/?open=archive-bot` | none | no |
| Main shell | Delete a bot permanently (purge) | `ui/App.tsx` (`purgeTarget`) | yes: `/?open=delete-bot` | none | no |
| Main shell | Name taken / archived bot cannot be edited / manager cannot be removed | `ui/agentForm.ts`, `fixtureBridge.ts:888-889`, `:1692`, `:1718` | partly: the errors come from the fixture on the matching form submit | a click path through the editor | no |
| Main shell | Keyboard shortcuts sheet | `ui/ExperimentalFeatures.tsx:71` (`ShortcutSheet`) | yes: `/?open=shortcuts&experiments` | none | no |
| Main shell | Keybinding editor, with conflicts | `ui/KeybindingEditor.tsx` | partly: Settings > Keyboard renders; conflict message needs a conflicting binding | a binding that clashes | no |
| Main shell | What's new dialog | `ui/WhatsNew.tsx:74` | yes: `/?open=whats-new` | none | no |
| Main shell | Message search dialog | `ui/ExperimentalFeatures.tsx:37` (`MessageSearch`) | partly: opens with `?open=search&experiments`; `search.messages` has no fixture branch, so results and errors cannot show | a `search.messages` branch returning hits | yes (in preview: a fixture branch) |
| Main shell | Project search dialog | `ui/ProjectSearch.tsx:74` | yes: `/?open=project-search` | none | no |
| Main shell | Handoff dialog (hand a conversation to another bot) | `ui/TeamMessages.tsx:85` (`HandoffDialog`) | yes: `/?open=handoff&session=session-codex&agent=agent-codex` | none | no |
| Main shell | Handoff failed | `ui/App.tsx` (`handoffError`) | partly: needs `session.handoff.draft` to fail | a fixture error on the draft call | no |
| Main shell | Side conversation (between bots) | `ui/App.tsx` (`sideSessionId`), `ui/TeamMessages.tsx` (`SideConversationHeader`) | yes: `/?open=side:<agentId>` | none | no |
| Main shell | Tasks board (board, timeline, tree) | `ui/TasksView.tsx:29` | yes: `/?open=tasks`; views switch by click | none | no |
| Main shell | Task form (create or edit) | `ui/TaskForm.tsx:26` | yes: from the board (by click) | none | no |
| Main shell | Task timeline view | `ui/TaskTimeline.tsx:54` | yes: by click on the Timeline view | none | no |
| Main shell | Task tree view | `ui/TaskTree.tsx:29` | yes: by click on the Tree view | none | no |
| Main shell | Task dispatch, unassign, cancel, stop, review, reorder | `ui/useTasks.ts`, `preview/fixtureTasks.ts:61-121` | yes: handled in `fixtureTasks.ts` | none | no |
| Main shell | Task run events (run changed, run finished) live | `ui/useTasks.ts`; events `task.run.changed`, `task.run.finished` | no: not emitted by the preview; static runs only (`task.runs`) | a timer that emits `task.run.changed` | yes (in preview: emitter) |
| Main shell | Task images (strip, preview) | `ui/TaskImageStrip.tsx`, `ui/useTasks.ts:65` | partly: `task.image` is handled (`fixtureTasks.ts:63`) but no seeded task has images unless `?preset` supplies one | a task with an image in the seed | no |
| Main shell | Tasks: empty board | `ui/TasksView.tsx:29` | no: no flag gives an empty task list | a `?tasks=empty` seed | no |
| Main shell | Working bots panel | `ui/WorkingBotsPanel.tsx:47` | partly: renders when tasks run; no dedicated scene | a scene with running tasks | no |
| Main shell | Compare answers (fan-out comparison) | `ui/FanoutCompare.tsx:127` (`CompareView`) | yes: `/?open=compare` (inspector "Compare answers") | none | no |
| Main shell | Ask several employees (fan-out dialog) | `ui/FanoutCompare.tsx:67` (`FanoutDialog`) | yes: `/?open=ask-several` | none | no |
| Main shell | Fan-out: choose 2 to 6 employees | `ui/FanoutCompare.tsx:67`, `fixtureBridge.ts:1168` | partly: the count check is in the fixture; the message appears on submit | a click path | no |
| Main shell | Fan-out: comparison no longer exists | `fixtureBridge.ts:1178` | partly: fixture error path exists | a click path | no |
| Main shell | Terminal dock | `ui/TerminalDock.tsx:140` | partly: `?open=terminal` opens it, but `terminalOpen` throws "Terminals run in the desktop app" (`fixtureBridge.ts:2228`) | a stand-in terminal (fixture) | yes (in preview: terminal stub) |
| Main shell | Document pane (project file beside chat) | `ui/DocumentPane.tsx:25` | yes: `/?preset=document` | none | no |
| Main shell | Media preview (video, audio, PDF) | `ui/MediaPreview.tsx:187` (`MediaDialog`), `ui/PdfViewer.tsx` | partly: `resolveProjectFile` returns the path only; no media fixture | a media file fixture | no |
| Main shell | Image lightbox | `ui/ImageLightbox.tsx:13` | partly: needs an image, and `message.image` returns `{}` (not handled) | a `message.image` branch that returns a data URL | yes (in preview: fixture branch) |
| Main shell | Generated image: making, ready, failed | `ui/GeneratedImage.tsx:102`, `ui/ActivityDetails.tsx` (`ImageMakingCard`) | partly: `?preset=generated-image` shows the states; the image bytes come from `message.image`, which is not handled | a `message.image` branch | yes (in preview: fixture branch) |
| Main shell | Staged attachment (paste or pick a file) | `ui/ComposerControls.tsx` (`AttachMenu`, `AttachmentTray`), `fixtureBridge.ts:1900` | partly: staging works; the clipboard path returns the fixture image | none | no |
| Main shell | Clipboard image cannot be read | `fixtureBridge.ts:1901` ("The image could not be read.") | partly: throws only for empty bytes | a clipboard fixture with empty data | no |
| Main shell | Changes view (file diffs for a conversation) | `ui/ChangesView.tsx:43` | yes: `/?open=changes&session=session-codex` | none | no |
| Main shell | Revert a file change (confirm, undo) | `ui/App.tsx:242` (`revertTarget`), `fixtureBridge.ts:644-645` | partly: the fixture handles revert and revert undo; the click path needs a file | a click path | no |
| Main shell | Pull request dialog (gh ready) | `ui/PullRequestDialog.tsx:43` | yes: `/?open=pull-request&session=session-codex&agent=agent-codex` | none | no |
| Main shell | Pull request without GitHub CLI | `ui/PullRequestDialog.tsx:43` (`pullRequestPlan`) | yes: `&pr=nogh` | none | no |
| Main shell | Abandoned worktrees list (discard) | `ui/AbandonedWorktrees.tsx:63` | yes: under `/?open=settings:Health` (fixture `fixtureAbandoned`) | none | no |
| Main shell | Delete a conversation (confirm, undo toast) | `ui/App.tsx:2402` area, `ui/ExperimentalFeatures.tsx` (`UndoToasts`), `ui/undo.ts` | yes for the dialog: `/?open=delete-conversation&session=session-codex&agent=agent-codex`; the undo toast path is partly | a click to delete, then the toast | no |
| Main shell | Undo period ended | `fixtureBridge.ts:912` | partly: fixture error path exists | a click path past the undo timer | no |
| Main shell | Delete active conversation refused | `fixtureBridge.ts:897`, `:922` ("Cannot delete an active conversation.") | partly: needs an active session selected | a click path | no |
| Main shell | Project settings dialog (board, shared settings, trust) | `ui/ProjectSettingsDialog.tsx:104` | yes: `/?open=project-settings` | none | no |
| Main shell | Delete project | `ui/App.tsx` (`projectDelete`) | yes: `/?open=delete-project` | none | no |
| Main shell | New project dialog (folder, clone, find) | `ui/NewProjectDialog.tsx:45` | yes: `/?open=new-project` | none | no |
| Main shell | Clone a project (progress, cancel, failure) | `ui/ProjectSources.tsx:55` (`CloneProject`), `fixtureBridge.ts:786` (`?clone=`) | partly: `?clone=` and `project.clone` exist; a failure needs a fixture error | a `project.clone` error branch | no |
| Main shell | Discover projects | `ui/ProjectSources.tsx:123` (`DiscoverProjects`) | yes: New project > Find | none | no |
| Main shell | Project suggestions | `ui/ProjectSuggestions.tsx:26` | no: `project.suggestions` has no branch and returns `{}` | a `project.suggestions` branch | yes (in preview: fixture branch) |
| Main shell | Project icon picker | `ui/ProjectIcon.tsx:126` | yes: via project settings (click) | none | no |
| Main shell | Project members and hire | `ui/StaffProjectDialog.tsx:8`, `ui/ProjectSettingsDialog.tsx` | partly: `project.members` exists; `project.members.changed` is not emitted | a timer emitting the event | yes (in preview: emitter) |
| Main shell | Project scaffold (GitHub repo create, login) | `ui/NewProjectDialog.tsx:45`, `fixtureBridge.ts:1204-1214` | partly: the fixture covers status, login and create; the failure path is not seeded | a scaffold failure branch | no |
| Main shell | Project folder missing or no folder on this computer | `fixtureBridge.ts:1758-1759` | partly: fixture error path exists | a click path with a missing project | no |
| Session rail | Session rail: workspace, environment, checkpoints | `ui/SessionRail.tsx:36`, `ui/SessionRailExtras.tsx:60`, `:119` | no: `session.environment`, `session.workspace`, `session.checkpoints` have no branch and return `{}`, so the rail shows empty or a failure | a branch for each of the three RPCs | yes (in preview: fixture branch) |
| Session rail | Session rail: repository state, diff, stage | `ui/SessionRail.tsx` (`session.repository`, `session.git.*`), `fixtureBridge.ts:632-645` | yes: handled | none | no |
| Session rail | Local servers (dev servers started by a conversation) | `ui/LocalServers` (`ui/localServers.ts`), scene `servers` | yes: `/?scene=servers` | none | no |
| Conversation messages | User messages: short, long, attachments, reactions, reply | `ui/MessageActions.tsx` (`MessageActions`), `ui/ReplyAnnotations.tsx` | yes: `/?scene=user-messages`; "Message actions, hovered" (inspector) | none | no |
| Conversation messages | Assistant Markdown: headings, tables, code | `ui/SafeMarkdown.tsx` (`SafeMarkdown`) | yes: `/?scene=markdown` | none | no |
| Conversation messages | Charts and data tables in replies | `ui/RichChart.tsx` (`RichChart`), `ui/RichTable.tsx` (`RichTable`) | yes: `/?scene=rich` | none | no |
| Conversation messages | Visual replies (HTML or SVG in a frame) | `ui/VisualReply.tsx:32` | yes: `/?scene=visual`, `?preset=visual` | none | no |
| Conversation messages | OpenUI reply (buttons and forms in a reply) | `ui/openui` (`OpenUiBlock`) | yes: `?preset=openui` | none | no |
| Conversation messages | Right-to-left and CJK text, emoji | `ui/SafeMarkdown.tsx` | yes: `/?scene=languages` | none | no |
| Conversation messages | Worst-case layout (long word, wide code, 14-column table) | n/a (layout) | yes: `/?scene=overflow` | none | no |
| Conversation messages | Thinking and plans | `ui/TurnPlan.tsx` (`TurnPlan`), `ui/TeamMessages.tsx:238` (`PlanCard`) | yes: `/?scene=thinking` | none | no |
| Conversation messages | Notices: limits, context resets, images, PR, retries, setup scripts, restored copy | `ui/TeamMessages.tsx` (`UsageLimitNotice`, `ContextResetDivider`, `AutoRetryNotice`) | yes: `/?scene=notices` | none | no |
| Conversation messages | Message reply target (replying to a message) | `ui/MessageActions.tsx` (`ReplyContextStrip`) | yes: `/?scene=user-messages` | none | no |
| Conversation messages | Message from another employee (blob message) | `ui/TeamMessages.tsx` (`DelegationChip`, `ReplyChip`) | yes: `/?scene=chips` | none | no |
| Conversation messages | Message search result jump | `ui/App.tsx:240` (`searchTarget`) | partly: depends on `search.messages` (see message search) | a `search.messages` branch | yes (in preview: fixture branch) |
| Conversation messages | Context window ring (nearly full warning) | `ui/ContextWindowIndicator.tsx:5` | yes: `/?scene=context` | none | no |
| Conversation messages | Context reset, auto retry, usage limit notices | `ui/TeamMessages.tsx` | yes: `/?scene=notices` | none | no |
| Conversation messages | Change banner (last turn's file changes) | `ui/ChangeBanner.tsx:39` | partly: depends on the last turn having file changes (`scene=notices` may not show it) | a seeded turn with file changes | no |
| Conversation turns | Turn working (indicator, half-written reply) | `ui/WorkingMark.tsx` (`WorkingMark`), `ui/companionStatus.ts:80` | yes: `/?scene=working` | none | no |
| Conversation turns | Turn cancelling (Stopping) | `ui/companionStatus.ts` (`cancelling`) | yes: `/?scene=cancelling` | none | no |
| Conversation turns | Stop a running turn (the Stop control) | `ui/App.tsx:1256`, `:2402`, `:2412`, `:2455` (`session.cancel`) | no: `session.cancel` has no branch and returns `{}`; the turn does not stop and no error shows | a `session.cancel` branch that moves the session to cancelling then idle | yes (in preview: fixture branch) |
| Conversation turns | Turn failed (error, what to do next) | `ui/errorCopy.ts`, scene `turn-failed` | yes: `/?scene=turn-failed`, `?preset=failure` | none | no |
| Conversation turns | Process exited (provider stopped unexpectedly) | `ui/errorCopy.ts:129` (`PROCESS_EXITED`) | partly: appears in `fixtureScenes.ts` (turn error code `process_exited`) only | a second scene or a turn error with that code | no |
| Conversation turns | OpenCode runtime error | `ui/RuntimeProblemMarker.tsx:26` | yes: `?preset=opencode-runtime-error` | none | no |
| Conversation turns | Stopped at usage limit, with handover offer | `ui/errorCopy.ts:101` (`QUOTA`), `ui/TeamMessages.tsx` (`UsageLimitNotice`) | yes: `/?scene=limited`, `/?open=…` limit handoff (inspector "Hand over at a limit") | none | no |
| Conversation turns | Usage limit with resume and snooze | `ui/App.tsx` (`session.limit.resume`, `session.limit.snooze`) | yes: `/?scene=limited-controls` | none | no |
| Conversation turns | Rate limited (session state `rate_limited`) | `ui/companionStatus.ts:51`, `ui/errorCopy.ts:187-248` | no: no fixture uses `rate_limited` (0 hits) | a session in state `rate_limited` | no |
| Conversation turns | Retrying (session state `retrying`) | `ui/companionStatus.ts:50` | no: 0 hits in fixtures | a session in state `retrying` | no |
| Conversation turns | Paused (session or queue paused) | `ui/PromptQueue.tsx:50`, scene `queue-paused` | yes: `/?scene=queue-paused` (queue paused) | none | no |
| Conversation turns | Turn with unknown or unsupported setting | `executionContract.ts:130` ("does not support this setting") | partly: `executionContract` returns the copy; the fixture reports `supported:false` for some runtimes | a runtime with the setting unsupported | no |
| Composer | Model, effort and speed controls | `ui/SessionExecutionControls.tsx` (`SessionExecutionControls`) | yes: click path; inspector "Pick a model" | none | no |
| Composer | Model change needs confirmation | `ui/SessionExecutionControls.tsx` (`modelChangeNeedsConfirmation`), `fixtureBridge.ts:938` | yes: click path | none | no |
| Composer | Model change during an active turn refused | `fixtureBridge.ts:930` ("Model and effort cannot change during an active turn.") | yes: click path during a working scene | none | no |
| Composer | Model not in the catalog | `fixtureBridge.ts:940` | partly: needs an invalid model submitted | a click path | no |
| Composer | Permission mode control (ask, auto, Full access) | `ui/ComposerControls.tsx:79` (`PermissionModeControl`) | yes: click path | none | no |
| Composer | Full access confirmation (bypass) | `ui/approvalUi.tsx:35` (`BypassConfirmDialog`), `fixtureBridge.ts:628` | yes: click path | none | no |
| Composer | Permission mode locked by a schedule | `fixtureBridge.ts:975` ("A scheduled run's permission mode is fixed by the schedule.") | partly: needs an automation session selected | a click path | no |
| Composer | Slash menu (skills and manager commands) | `ui/SlashMenu.tsx:58` | yes: type `/` in the composer | none | no |
| Composer | Mention picker (@ employee) | `ui/TeamMessages.tsx` (`MentionPicker`) | yes: type `@` | none | no |
| Composer | Queue banner (messages waiting, send now, reorder) | `ui/PromptQueue.tsx:50` (`QueueBanner`) | yes: `/?scene=queue`, `?scene=queue-no-steer` | none | no |
| Composer | Queue is full (20 items) | `fixtureBridge.ts:1415` | partly: needs 20 queued messages | a click path | no |
| Composer | Send later (scheduled message) | `ui/PromptQueue.tsx:157` (`SendLaterControl`), `fixtureBridge.ts:1419` | partly: fixture accepts a time; the control is by click | none | no |
| Composer | Steer a running turn (send into the turn) | `ui/App.tsx` (`session.steer`), `fixtureBridge.ts:1267-1271` | partly: works for working scenes; the "steering off" error (`unsupported`) needs `exec_gate.codex.steer` false | a settings toggle | no |
| Composer | Steering not supported by this runtime | `fixtureBridge.ts:1268`, scene `queue-no-steer` | yes: `/?scene=queue-no-steer` | none | no |
| Composer | Dictation button (mic in the composer) | `ui/DictationButton.tsx` | partly: the button renders; a real microphone cannot be used in the preview | a stand-in microphone | no |
| Composer | Busy-send behaviour (queue, steer, interrupt) | `ui/busySend.ts`, `ui/ComposerControls.tsx` | partly: needs a working scene and a click | none | no |
| Composer | Composer low on space | n/a | yes: `?composer-low` | none | no |
| Composer | Sent-message draft, stash | `ui/PromptStash.tsx` (`StashMenu`), `fixtureBridge.ts:1183-1197` | yes: stash and restore are handled | none | no |
| Approvals and questions | Approve a shell command (Allow once, Deny) | `ui/ApprovalCard.tsx:27` | yes: `/?scene=approval-shell` | none | no |
| Approvals and questions | Approval with an Always choice | `ui/ApprovalCard.tsx`, `ui/PermissionChoiceButton.tsx` | yes: `/?scene=approval-always` | none | no |
| Approvals and questions | Very long command in an approval | `ui/ApprovalCard.tsx` | yes: `/?scene=approval-long` | none | no |
| Approvals and questions | Approve a file edit (diff) | `ui/ApprovalCard.tsx` | yes: `/?scene=approval-file` | none | no |
| Approvals and questions | Approval with a deadline | `ui/ApprovalCard.tsx:27` (`expiresIn`) | yes: `/?scene=approval-expiring` | none | no |
| Approvals and questions | Several approvals waiting (queue bar, next and previous) | `ui/CompanionAttention.tsx:40` (`PermissionQueueBar`) | yes: `/?scene=approval-many` | none | no |
| Approvals and questions | Question with one answer / several / multi-select | `ui/TeamMessages.tsx:256` (`QuestionCard`) | yes: `/?scene=question-single`, `question-multi`, `question-several` | none | no |
| Approvals and questions | Approval arrives while watching (live) | `permission.requested` event (`ui/App.tsx`) | no: the preview never emits `permission.requested` | a timer that emits it with a seeded approval | yes (in preview: emitter) |
| Approvals and questions | Permission answered elsewhere (resolved) | `permission.resolved` | partly: the preview emits it on `permission.reply` for two ids only | a click path on other approvals | no |
| Approvals and questions | Auto-approved actions list | `ui/approvalUi.tsx` (`AutoApprovedList`), event `permission.auto_resolved` | no: event not emitted | a seeded auto-approved list and an emitter | yes (in preview: emitter) |
| Approvals and questions | Bypass active notice | `ui/approvalUi.tsx`, event `permission.bypass_active` | no: event not emitted | an emitter | yes (in preview: emitter) |
| Approvals and questions | Permission reply refused (denied, expired, Full access not allowed) | `ui/errorCopy.ts:136` ("Permission was denied.") | no: no fixture error on permission reply | a `permission.reply` error branch | yes (in preview: fixture branch) |
| Approvals and questions | Approval rules: list, change scope, delete | `ui/ApprovalRulesPage.tsx:19`, `:23`, `:35`, `:41` | no: `permission.rules.list`, `.update`, `.delete` have no branch; the page shows an empty list and mutations are silent | three fixture branches and a rules seed | yes (in preview: fixture branches) |
| Approvals and questions | Approvals settings page | `ui/SettingsSheet.tsx:800` | yes: Settings > Approvals (inspector "Approvals") | none | no |
| Approvals and questions | Secret request (companion and chat) | `ui/SecretsPage.tsx:107` (`SecretRequests`) | yes: `/?mode=home&view=chat&secretrequest=1` (inspector "A secret request") | none | no |
| Approvals and questions | Secret request answered or declined | `fixtureBridge.ts:676` (`secret.request.answer`, `.decline`) | yes: click path | none | no |
| Approvals and questions | Secret request changed live | `secret.request.changed` event | no: not emitted | a timer | yes (in preview: emitter) |
| Approvals and questions | Secrets page (names, add, delete) | `ui/SecretsPage.tsx:25` | yes: Settings > Secrets (inspector "Secrets") | none | no |
| Proposals (Manager) | Create an employee | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-bot` | none | no |
| Proposals (Manager) | Create a project | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-project` | none | no |
| Proposals (Manager) | Assign a task | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-task` | none | no |
| Proposals (Manager) | Merge a finished task | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-merge` | none | no |
| Proposals (Manager) | Update an employee | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-update` | none | no |
| Proposals (Manager) | Change the playbook (diff) | `ui/PlaybookDiff.tsx:8` | yes: `/?scene=proposal-playbook` | none | no |
| Proposals (Manager) | Remember something about you (profile) | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-profile` | none | no |
| Proposals (Manager) | Change a permission (Full access refused for a proposal) | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-permission` | none | no |
| Proposals (Manager) | Open a conversation (proposal) | `ui/ProposalCard.tsx:77` | yes: `/?scene=proposal-open` | none | no |
| Proposals (Manager) | Accept or reject a proposal, failure | `fixtureBridge.ts` (`proposal.decide`, only `perm-proposal`) | partly: the accept path works for one id; other proposals return `{}` silently | a `proposal.decide` branch per proposal | yes (in preview: fixture branch) |
| Team and delegation | Team page (tabs, project filter) | `ui/TeamPage.tsx:51`, `ui/App.tsx:3122`, `openTeam` `ui/App.tsx:2546` | yes: inspector "Teams and helpers" | none | no |
| Team and delegation | Assignments in every state (running, waiting, done, failed, stopped) | `ui/DelegationGroup.tsx:88`, `ui/DelegateBoard.tsx:27` | yes: `/?scene=team` | none | no |
| Team and delegation | Message chips (sent, failed, waiting for approval, question, reply, new employee) | `ui/TeamMessages.tsx` (`DelegationChip`, `ReplyChip`, `CreatedBotChip`) | yes: `/?scene=chips` | none | no |
| Team and delegation | Stop one assignment | `ui/DelegationGroup.tsx:49` (`delegation.cancel`) | no: `delegation.cancel` has no branch; nothing happens and no error shows | a `delegation.cancel` branch | yes (in preview: fixture branch) |
| Team and delegation | Stop all assignments in a group | `ui/DelegationGroup.tsx:107` (`delegation.cancelGroup`) | no: no branch | a `delegation.cancelGroup` branch | yes (in preview: fixture branch) |
| Team and delegation | Delegation pause (setting) | `ui/DelegationPause.tsx:36` | partly: the setting is stored; the pause effect is not shown | none | no |
| Team and delegation | Sub-bots fleet (helpers, count) | `ui/SubbotViews.tsx:55` (`SubbotFleet`) | yes: `/?mode=home&subbots=1` (inspector "A fleet of helpers") | none | no |
| Team and delegation | Stop a helper, or stop all helpers | `ui/SubbotViews.tsx:40`, `:65` (`delegation.cancel`, `delegation.cancelGroup`) | no: no branches | the same two branches | yes (in preview: fixture branches) |
| Team and delegation | Sub-bot changed live | `subbot.changed` event | no: not emitted | a timer | yes (in preview: emitter) |
| Team and delegation | Team sidebar (members, tags, menu) | `ui/TeamSidebar.tsx:180` | yes: main `/` sidebar | none | no |
| Team and delegation | Team activity (companion) | `ui/CompanionTeam.tsx` (`CompanionTeamActivity`) (`CompanionTeamActivity`) | yes: `/?companion&mode=home&view=activity` | none | no |
| Team and delegation | Companion team picker and mentions | `ui/CompanionTeam.tsx:75` (`CompanionTeamPicker`) | yes: companion chat | none | no |
| Team and delegation | Companion fan-out summary | `ui/FanoutCompare.tsx:232` (`CompanionFanoutSummary`) | partly: needs a fan-out in progress | a fan-out fixture | no |
| Team and delegation | Delegation changed live (`delegation.updated`) | event `delegation.updated` | no: not emitted | a timer | yes (in preview: emitter) |
| Team and delegation | Hand a conversation to another employee (handoff draft) | `ui/TeamMessages.tsx:85`, `session.handoff.draft` | yes: handoff dialog | none | no |
| Phone and remote (desktop side) | Remote off | `ui/RemoteSettings.tsx:122` | yes: `/?preview-settings` or inspector "Remote, off" | none | no |
| Phone and remote (desktop side) | Remote on, with devices | `ui/RemoteSettings.tsx:122` | yes: inspector "Remote, on with devices" (`?remote=on&devices=3`) | none | no |
| Phone and remote (desktop side) | Tailscale not installed | `ui/RemoteSettings.tsx:122`, `ui/RemoteSettings.tsx` `TAILSCALE_FIX` | yes: `?remote=not_installed` | none | no |
| Phone and remote (desktop side) | Tailscale signed out | `ui/RemoteSettings.tsx:122` | yes: `?remote=logged_out` | none | no |
| Phone and remote (desktop side) | HTTPS off on the tailnet | `ui/RemoteSettings.tsx:122` | yes: `?remote=https_disabled` | none | no |
| Phone and remote (desktop side) | Funnel on (warning) | `ui/RemoteSettings.tsx:122` | yes: `?remote=funnel_on` | none | no |
| Phone and remote (desktop side) | Port in use | `ui/RemoteSettings.tsx:122` | yes: `?remote=port_in_use` | none | no |
| Phone and remote (desktop side) | Pairing request waiting for approval (tier pick) | `ui/RemoteSettings.tsx:122`, `remote.pairing.requested` | yes: `?pending-device` | none | no |
| Phone and remote (desktop side) | Pairing expired | `remote.pairing.completed` (outcome `expired`) | partly: the preview emits it in `remoteFixture.ts`; the desktop panel path is not seeded | a click path | no |
| Phone and remote (desktop side) | Remote activity list | `ui/RemoteSettings.tsx`, `remote.activity.list` | partly: `remote.activity` is emitted; the list RPC has no branch in `fixtureBridge.ts` | a `remote.activity.list` branch | yes (in preview: fixture branch) |
| Phone and remote (desktop side) | Remote devices: revoke, rename, change tier | `ui/RemoteSettings.tsx`, `remote.devices.list`, `.revoke`, `.update` | no in the desktop path: these RPCs have no branch; `remoteFixture.ts` serves the phone side only | three branches in `fixtureBridge.ts` | yes (in preview: fixture branches) |
| Phone and remote (desktop side) | Native method: switch Tailscale to Native (one at a time) | `ui/NativeRemote.tsx:30` (`RemoteMethodSwitch`), `remote.method.set` | no: no branch | a `remote.method.set` branch and a Native status | yes (in preview: fixture branch) |
| Phone and remote (desktop side) | Native account (sign-in password) | `ui/NativeRemote.tsx:44` (`NativeAccountGroup`), `remote.native.account.set` | no: no branch | a branch | yes (in preview: fixture branch) |
| Phone and remote (desktop side) | Native addresses, port, UPnP, firewall, external IP | `ui/NativeRemote.tsx:162` (`NativeConnectivityGroup`), `remote.native.port.set`, `.upnp.set`, `.firewall.add`, `.firewall.remove`, `.externalIp.set` | no: no branches | branches for each RPC, and a Native status with addresses | yes (in preview: fixture branches) |
| Phone and remote (desktop side) | Native certificate regenerate and fingerprint | `ui/NativeRemote.tsx`, `remote.native.cert.regenerate` | no: no branch | a branch and a fingerprint value | yes (in preview: fixture branch) |
| Phone and remote (desktop side) | Remote security settings | `ui/RemoteSettings.tsx`, `remote.security.get`, `.set` | no: no branch | two branches | yes (in preview: fixture branches) |
| Phone and remote (desktop side) | Push notifications (enable, key, subscribe) | `ui/RemoteSettings.tsx:64` (`PushSetting`), `remote/PushCard.tsx:28`, `:46` | no: `push.key`, `push.subscribe`, `push.unsubscribe` have no branch (the card shows as unsupported) | three branches | yes (in preview: fixture branches) |
| Phone and remote (desktop side) | Webhook panel (enable, rotate, URL) | `ui/WebhookPanel.tsx:23` | yes: `automation.webhook.*` handled (`fixtureBridge.ts:656`) | none | no |
| Phone and remote (desktop side) | Remote changed live (`remote.changed`) | `remote.changed` | yes: emitted (`fixtureBridge`) | none | no |
| Phone and remote (phone PWA) | Pair with a code | `remote/RemoteApp.tsx:118`, `remote/deviceApi.ts:84` | yes: inspector `/remote.html?phone=connect` ("Pair with a code") | none | no |
| Phone and remote (phone PWA) | Sign in with a password (native) | `remote/RemoteApp.tsx` (`signedOutMethod`, `loginProblem`) | yes: inspector `phone-connect-native` | none | no |
| Phone and remote (phone PWA) | Your employees (bots list) | `remote/RemoteApp.tsx:118`, `remote/PhoneMachines.tsx` | yes: inspector `phone-bots` | none | no |
| Phone and remote (phone PWA) | A conversation on the phone | `remote/RemoteApp.tsx:118` | yes: inspector `phone-chat` | none | no |
| Phone and remote (phone PWA) | This device (the phone) | `remote/RemoteApp.tsx:118` | yes: inspector `phone-device` | none | no |
| Phone and remote (phone PWA) | View only tier | `remote/remoteTiers.ts` | yes: inspector `phone-viewer` | none | no |
| Phone and remote (phone PWA) | Chat and approve tier | `remote/remoteTiers.ts` | yes: inspector `phone-operator` | none | no |
| Phone and remote (phone PWA) | Approval from the phone (remote approval) | `remote/RemoteApp.tsx` | yes: `?remote-approval` (fixture) | none | no |
| Phone and remote (phone PWA) | Phone: pairing invalid, code wrong | `remote/deviceApi.ts:84` ("That code did not work.") | partly: remote simulator path exists; not in the inspector | a click path | no |
| Phone and remote (phone PWA) | Phone: machine offline or daemon unreachable | `remote/RemoteApp.tsx` (`connectProblem`) | partly: remote simulator has offline states for machines (`?machines=offline` on desktop); phone path not in inspector | a phone scenario with the connection down | no |
| Phone and remote (phone PWA) | Phone: too many requests, session expired | `remote/remoteTransport.ts` | no: the simulator does not return these codes | the codes in the simulator | no |
| Machines (other computers) | No machines linked (empty) | `ui/MachinesPage.tsx:213` | yes: inspector "Machines, none" | none | no |
| Machines (other computers) | Machine linked and online | `ui/MachinesPage.tsx:213` | yes: `?machines=1`, inspector "Machines, linked" | none | no |
| Machines (other computers) | Machine offline (last known, greyed) | `ui/MachineBots.tsx`, `ui/MachinesPage.tsx:213` | yes: `?machines=offline` | none | no |
| Machines (other computers) | Pair a machine (URL, code, fingerprint) | `ui/MachinesPage.tsx:64` (`PairMachine`) | yes: click path; `machine.pair.*` handled (`fixtureBridge.ts:684-690`) | none | no |
| Machines (other computers) | Pairing waiting for approval on the other machine | `ui/MachinesPage.tsx:64`, `machine.pair.pending` | yes: click path | none | no |
| Machines (other computers) | Pairing refused or fingerprint wrong | `ui/MachinesPage.tsx` | partly: fixture refuses "can't change or remove its own pairing" only (`fixtureBridge.ts:743`) | a fingerprint mismatch branch | no |
| Machines (other computers) | Another machine's conversation (read, send, steer) | `ui/MachineConversation.tsx:186` | yes: `/?machines=1`, click the machine's bot | none | no |
| Machines (other computers) | Take over a conversation (dialog, steps, new bot) | `ui/MachineConversation.tsx:45` (`TakeOverDialog`), `fixtureBridge.ts:766-772` | yes: `?takeover=newbot` and default | none | no |
| Machines (other computers) | Take over failed | `ui/MachineConversation.tsx:45` | no: fixture always succeeds | a `machine.takeOver` failure branch | yes (in preview: fixture branch) |
| Machines (other computers) | Moved bar (conversation moved to another machine) | `ui/MachineConversation.tsx:130` (`MovedBar`) | yes: `?preset=moved` | none | no |
| Machines (other computers) | Machine composer (send, steer) | `ui/MachineConversation.tsx:157` (`MachineComposer`), `machine.prompt`, `machine.steer` | yes: handled (`fixtureBridge.ts`) | none | no |
| Machines (other computers) | Permission on another machine (reply) | `ui/MachineConversation.tsx`, `machine.permission.reply` | yes: handled | none | no |
| Machines (other computers) | New project on another machine (browse folders, clone, create) | `ui/MachineRemote.tsx:48` (`NewProjectOnMachine`), `ui/MachinesPage.tsx:177` (`ProjectsFromDevices`) | yes: `machine.project.browse`, `.clone`, `.create` handled (`fixtureBridge.ts:723-734`) | none | no |
| Machines (other computers) | Clone on another machine (progress, failure) | `fixtureBridge.ts:733-734` ("That clone is no longer listed.") | partly: progress works; failure branch exists for missing clone | a click path | no |
| Machines (other computers) | Manage a machine (rename, remove, tier, devices) | `ui/MachineRemote.tsx:165` (`ManageMachine`) | yes: `machine.rename`, `.remove`, `.devices.*` handled | none | no |
| Machines (other computers) | Machine changed live (`machine.changed`, `machine.session.changed`) | `machine.changed` | no: not emitted | a timer | yes (in preview: emitter) |
| Machines (other computers) | This computer's own machine name | `fixtureBridge.ts:678-679` | yes: `machine.self`, `.self.rename` | none | no |
| Machines (other computers) | Machine self link (desktop side to phone) | `ui/MachinesPage.tsx` | yes: via pair flow | none | no |
| Computer (cloud computer) | Not linked | `ui/ComputerPage.tsx:123` | yes: inspector "Computer, not linked" | none | no |
| Computer (cloud computer) | Linked and awake | `ui/ComputerPage.tsx:123` | yes: `?preset=computer` (inspector "Computer, awake") | none | no |
| Computer (cloud computer) | Asleep | `ui/ComputerPage.tsx:123` | yes: `?preset=computer-asleep` | none | no |
| Computer (cloud computer) | Server not answering (away) | `ui/ComputerPage.tsx:123`, `fixtureBridge.ts:1320` (`unavailable`) | yes: `?preset=computer-away` | none | no |
| Computer (cloud computer) | Link: token and confirm | `ui/ComputerPage.tsx` (`computer.link`), `fixtureBridge.ts:1308-1309` | yes: click path | none | no |
| Computer (cloud computer) | Share the computer with employees (confirm) | `ui/ComputerPage.tsx` (`computer.setEnabled`), `fixtureBridge.ts:1315` | yes: click path | none | no |
| Computer (cloud computer) | Computer view (live screen, no input, no view) | `ui/ComputerView.tsx:28`, flags `?computer-no-input`, `?computer-no-view` | partly: the flags exist (`fixtureBridge.ts:1304`, `:1331`); a real screen frame is not produced | a screen image seed | no |
| Computer (cloud computer) | Computer logs | `ui/ComputerPage.tsx`, `computer.logs` | yes: handled | none | no |
| Computer (cloud computer) | Routine (scheduled routine) | `fixtureBridge.ts:1328` ("That routine is gone.") | partly: error path exists | a click path | no |
| Owner's computer (experimental) | Owner computer setting (off by default) | `ui/OwnerComputer.tsx:15` (`OwnerComputerSettings`), `settings.set` | partly: settings read and write work; the experimental switch's effect is not shown | none | no |
| Owner's computer (experimental) | Live grant bar (which bot is controlling, Stop) | `ui/OwnerComputer.tsx:67` (`OwnerComputerBar`), event `owner_computer.grant` | no: no fixture emits `owner_computer.grant` and `owner_computer.stop` has no branch | a seeded grant, the emitter, and a stop branch | yes (in preview: emitter and branch) |
| Owner's computer (experimental) | Owner computer approval card | `ui/ApprovalCard.tsx` (`owner_*` kinds) | no: no seeded `owner_*` approval | a seeded approval card | no |
| Connections and tools | Retinue MCP: on, off | `ui/ConnectionsPage.tsx:76` | yes: `?preset=mcp` (inspector "Connections, on") | none | no |
| Connections and tools | Retinue MCP: tier, Full access for outside sessions | `ui/ConnectionsPage.tsx:76`, `external.setTier`, `external.setFullAccess` | yes: handled (`fixtureBridge.ts:1229-1250`) | none | no |
| Connections and tools | Outside sessions (other tools' conversations seen) | `ui/OutsideSessions.tsx` (`OutsideSessionPills`, `OutsideWatchSetting`) | yes: `?preset=outside` | none | no |
| Connections and tools | Outside sessions changed live | event `outside.sessions.changed` | yes: emitted | none | no |
| Connections and tools | Connectors list (empty) | `ui/ConnectorsPage.tsx:139` | yes: `?connectors=empty` | none | no |
| Connections and tools | Connectors list with an OAuth connector | `ui/ConnectorsPage.tsx:139` | yes: default preview list | none | no |
| Connections and tools | Connector add, edit, remove, test | `ui/ConnectorsPage.tsx:139`, `connector.add`, `.update`, `.remove`, `.test` | yes: handled in `previewConnectorsRpc` (`fixtureBridge.ts:2327-2360`) | none | no |
| Connections and tools | Connector tools and per-tool rule | `ui/ConnectorsPage.tsx`, `connector.tools`, `connector.tool.rule` | yes: handled | none | no |
| Connections and tools | Connector OAuth sign-in (start, status, cancel) | `ui/ConnectorsPage.tsx:99`, `:108`, `:150`, `:183` | no: `connector.oauth.start`, `.status` have no branch; the connector branch throws "is not in the preview" (`fixtureBridge.ts:2359`) | an OAuth start and status branch and an emitter for `connector.oauth.changed` | yes (in preview: fixture branches) |
| Connections and tools | Connector error status | `ui/ConnectorsPage.tsx` (`connectorStatusLine`) (`connectorStatusLine`) | partly: the preview connector status is `ok` or `error`; error fixture set in code | a connector with status `error` | no |
| Connections and tools | Connector sign-in state changed | `connector.oauth.changed` | no: not emitted | an emitter | yes (in preview: emitter) |
| Chat channels | Telegram: not connected (CEO chat) | `ui/ChannelsPage.tsx:142` | yes: Settings > Channels (default) | none | no |
| Chat channels | Telegram: connected (account, peer) | `ui/ChannelsPage.tsx:142` | yes: `?channels=connected` | none | no |
| Chat channels | Telegram: bad bot token | `fixtureBridge.ts:2293` ("That doesn't look like a Telegram bot token.") | yes: click path with a bad token | none | no |
| Chat channels | WhatsApp Web: QR pairing | `ui/ChannelsPage.tsx:142`, `ui/QrGlyph.tsx:32`, `channel.whatsappWeb.start` | partly: the QR seed exists (`qrData`); `channel.qr` event is not emitted, so the QR does not refresh | a QR refresh emitter | yes (in preview: emitter) |
| Chat channels | WhatsApp Cloud API: connect | `channel.whatsappCloud.connect` | yes: handled (`previewVaultChannelsPi`) | none | no |
| Chat channels | Pairing candidate and answer (who may message) | `channel.pairing.candidate` (emitted), `channel.pairing.answer` (handled) | partly: candidate event emitted; answering works | a click path | no |
| Chat channels | Channel updated or disconnected live | `channel.updated`, `channel.disconnect` | no: not emitted | an emitter | yes (in preview: emitter) |
| Pi (keys for Pi) | Pi not installed | `ui/PiPage.tsx:49` | yes: `?pi=missing` | none | no |
| Pi (keys for Pi) | Pi installed, providers and keys | `ui/PiPage.tsx:49` | yes: default (`fixtureBridge.ts:2312`) | none | no |
| Pi (keys for Pi) | Test a key (fails or passes) | `pi.key.test` | partly: handled; failure text is not seeded | a failing key test | no |
| Pi (keys for Pi) | Key set or deleted live | `pi.key.set`, `pi.key.delete` events | no: not emitted | an emitter | yes (in preview: emitter) |
| Accounts and providers | Accounts list and sign-in | `ui/AccountsPage.tsx:166` | yes: inspector "Accounts" | none | no |
| Accounts and providers | Add an account (name, provider), sign-in | `account.create`, `account.signIn` | yes: handled (`fixtureBridge.ts:662`) | none | no |
| Accounts and providers | Account error (duplicate, custom model) | `ui/AccountsPage.tsx` (`customModelProblem`) (`customModelProblem`) | partly: click path | none | no |
| Accounts and providers | Providers: enable or disable a coding agent | `ui/ProvidersPage.tsx:43` | yes: Settings > Providers via `?open=settings:Providers` (no inspector scenario) | none | no |
| Accounts and providers | Custom model save | `model.custom.save` (`fixtureBridge.ts:668`) | yes: handled | none | no |
| Accounts and providers | Runtime profiles (custom runtime: list, save, delete) | `ui/SettingsSheet.tsx:229`, `:529`, `:542`; `runtime.profile.list`, `.save`, `.delete` | no: no branches; the page shows an empty list and saves are silent | three branches | yes (in preview: fixture branches) |
| Accounts and providers | Sign-in help (clear sign-in, runtime) | `ui/SignInHelp.tsx:26`, `:49` (`runtime.auth.clear`) | partly: the help renders from runtime status; clearing has no branch | a `runtime.auth.clear` branch | yes (in preview: fixture branch) |
| Accounts and providers | Coding agent sign-in required (friendly error) | `ui/errorCopy.ts:187-199` (sign-in codes, `signIn` helper) | partly: needs a runtime or turn error with `unauthenticated` (no fixture) | a fixture turn error with that code | no |
| Accounts and providers | Bot access (environment keys) | `ui/BotAccess.tsx:19` (`envKeyProblem`) | partly: click path; invalid key message from the form | none | no |
| Accounts and providers | Runtime problem marker | `ui/RuntimeProblemMarker.tsx:26` | yes: `?preset=opencode-runtime-error` | none | no |
| Accounts and providers | Coding agent runtime discovery (scan) timeout and probe error | `ui/runtimeDiscovery.ts`; flags `?discovery-timeout-once`, `?discovery-probe-error` | yes: flags exist (`fixtureBridge.ts:595`, `:591`) | none | no |
| Memory, playbook and skills | Memory page (facts, notes, scopes) | `ui/MemoryPage.tsx:50` | yes: inspector "Memory" | none | no |
| Memory, playbook and skills | Memory: note too long, secret-looking note refused, version conflict | `fixtureBridge.ts:1120-1131` | yes: click path | none | no |
| Memory, playbook and skills | Manager page (playbook, profile, history, revert) | `ui/ManagerPage.tsx:73` | yes: inspector "Manager", "Manager proposals" | none | no |
| Memory, playbook and skills | Manager setup card (create the manager) | `ui/ManagerSetupCard.tsx` | partly: shown only when no manager exists; the fixture always has one | a `?preset` without a manager | no |
| Memory, playbook and skills | Manager troubleshoot (open a conversation's failure in the manager) | `ui/App.tsx:1905` (`manager.troubleshoot`) | no: no branch returns `{}`; nothing opens | a `manager.troubleshoot` branch | yes (in preview: fixture branch) |
| Memory, playbook and skills | Skills pool: list, import, scan, toggle, delete | `ui/SkillsPage.tsx:87`, `skills.*` | yes: inspector "Skills" (`release-notes`) | none | no |
| Memory, playbook and skills | Learned skills: review list, approve, edit, forget | `ui/SkillsPage.tsx:112`, `:258`, `:280`, `:306`; `skills.learned.*` | no: `skills.learned.list`, `.approve`, `.edit`, `.forget` have no branch; the list is always empty | four branches and a seed of pending skills | yes (in preview: fixture branches) |
| Memory, playbook and skills | Skills changed live | `skills.changed` (emitted), `skills.learned.changed` (not emitted) | partly | an emitter for learned skills | yes (in preview: emitter) |
| Memory, playbook and skills | Bot skills and memory groups | `ui/BotSkillsGroup.tsx`, `ui/BotMemoryGroup.tsx` | yes: bot page | none | no |
| Memory, playbook and skills | Bot commands (slash commands for a bot) | `ui/BotCommands.tsx:40` | yes: bot page | none | no |
| Settings (25 pages) | General | `ui/SettingsSheet.tsx:60` (`SettingsPageId`), page render `ui/SettingsSheet.tsx:714-872` | yes: `/?preview-settings` (inspector "General") | none | no |
| Settings (25 pages) | Appearance | `ui/AppearancePage.tsx` | yes: inspector "Appearance" | none | no |
| Settings (25 pages) | Chat | `ui/SettingsSheet.tsx` | yes: inspector "Chat" | none | no |
| Settings (25 pages) | Speech | `ui/SettingsSheet.tsx`, `ui/SpeechModelDialog.tsx:11` | yes: inspector "Speech" | none | no |
| Settings (25 pages) | Keyboard | `ui/KeybindingEditor.tsx` | yes: inspector "Keyboard" | none | no |
| Settings (25 pages) | Companion | `ui/CompanionPanels.tsx:107` | yes: inspector "Companion" | none | no |
| Settings (25 pages) | Agents | `ui/SettingsSheet.tsx` | yes: inspector "Agents" | none | no |
| Settings (25 pages) | Accounts | `ui/AccountsPage.tsx:166` | yes: inspector "Accounts" | none | no |
| Settings (25 pages) | Providers | `ui/ProvidersPage.tsx:43` | yes: `?open=settings:Providers` (no inspector scenario) | none | no |
| Settings (25 pages) | Manager | `ui/ManagerPage.tsx:73` | yes: inspector "Manager" | none | no |
| Settings (25 pages) | Approvals | `ui/ApprovalRulesPage.tsx:19` | partly: page shows; the rule list is empty (no `permission.rules.list` branch) | see approval rules | yes (in preview: fixture branches) |
| Settings (25 pages) | Memory | `ui/MemoryPage.tsx:50` | yes: inspector "Memory" | none | no |
| Settings (25 pages) | Skills | `ui/SkillsPage.tsx:87` | yes: inspector "Skills" | none | no |
| Settings (25 pages) | Remote | `ui/RemoteSettings.tsx:122` | yes: inspector "Remote" (7 states) | none | no |
| Settings (25 pages) | Machines | `ui/MachinesPage.tsx:213` | yes: inspector "Machines" (3 states) | none | no |
| Settings (25 pages) | Connections | `ui/ConnectionsPage.tsx:76` | yes: inspector "Connections" (2 states) | none | no |
| Settings (25 pages) | Computer | `ui/ComputerPage.tsx:123` | yes: inspector "Computer" (4 states) | none | no |
| Settings (25 pages) | Secrets | `ui/SecretsPage.tsx:25` | yes: inspector "Secrets" | none | no |
| Settings (25 pages) | Connectors | `ui/ConnectorsPage.tsx:139` | partly: `?open=settings:Connectors` renders; OAuth sign-in cannot complete | see connector OAuth | yes (in preview: fixture branches) |
| Settings (25 pages) | Vault | `ui/VaultPage.tsx:182` | yes: `?open=settings:Vault` (no inspector scenario); `?vault=empty`, `?vault=request` | none | no |
| Settings (25 pages) | Channels | `ui/ChannelsPage.tsx:142` | yes: `?open=settings:Channels`; `?channels=connected` | none | no |
| Settings (25 pages) | Pi | `ui/PiPage.tsx:49` | yes: `?open=settings:Pi`; `?pi=missing` | none | no |
| Settings (25 pages) | Updates | `ui/UpdatesPanel.tsx:17` | yes: inspector "Updates" (6 states) | none | no |
| Settings (25 pages) | Backup | `ui/BackupPage.tsx:46` | yes: inspector "Backup" | none | no |
| Settings (25 pages) | Health | `ui/HealthPage.tsx:21` | partly: page renders, but shows "The service reported no checks." (no `doctor.run` branch) | see health check | yes (in preview: fixture branch) |
| Settings (25 pages) | Settings page load failure (`loading`, skeleton) | `ui/SettingsSkeleton.tsx` | partly: the skeleton shows while loading | none | no |
| Health | Health check: list of checks (ok, warning, failing) | `ui/HealthPage.tsx:21` | no: `doctor.run` has no branch and returns `{}`, so the list is always empty | a `doctor.run` branch returning checks | yes (in preview: fixture branch) |
| Health | Health check: a Fix button runs a repair | `ui/HealthPage.tsx:21`, `:34` (`fix`) | no: no checks, so no fix buttons | as above, plus the fix method | yes (in preview: fixture branch) |
| Health | Health check could not run ("Is the Retinue service connected?") | `ui/HealthPage.tsx:27` | no: the fixture never rejects `doctor.run` | a `doctor.run` error branch | yes (in preview: fixture branch) |
| Health | Health check "Check again" | `ui/HealthPage.tsx:40` | partly: button works, result is empty | as above | yes (in preview: fixture branch) |
| Health | Report a problem (GitHub draft) | `ui/ReportProblemDialog.tsx:23` | yes: via Health > Report | none | no |
| Health | Diagnostics export (file list, path, sizes) | `ui/DiagnosticsDialog.tsx:12`, `ui/DiagnosticsDialog.tsx:26` (`diagnostics.preview`) | partly: `?open=diagnostics` shows the dialog; `diagnostics.preview` has no branch (returns `{}`), so the file list is empty; `writeDiagnosticsExport` is stubbed | a `diagnostics.preview` branch | yes (in preview: fixture branch) |
| Updates | Up to date | `ui/UpdatesPanel.tsx:17` | yes: default; inspector "Updates, up to date" | none | no |
| Updates | Update available (badge, banner) | `ui/UpdateBadge.tsx:73`, `ui/UpdateBanner.tsx` | yes: `?update=ready` or `?updates=available` | none | no |
| Updates | Downloading (progress ring) | `ui/UpdatesPanel.tsx:17`, `ui/UpdateBadge.tsx:73` (`ProgressRing`) | yes: `?update=downloading`, `?updates=progress` | none | no |
| Updates | Update failed | `ui/updatesContract.ts` (`safeUpdateFailureMessage`) | yes: `?update=error` | none | no |
| Updates | Never checked | `ui/UpdatesPanel.tsx:17` | yes: `?updates=idle` | none | no |
| Updates | Development build, Store copy | `ui/UpdatesPanel.tsx:17` | yes: `?updates=dev`, `?updates=store` | none | no |
| Updates | Stable and beta channel | `ui/UpdatesPanel.tsx:17` | yes: `?channel=stable` | none | no |
| Updates | Coding agent (harness) update available or in progress | `ui/HarnessUpdatesNotice.tsx:35` | yes: `?preset=harness-update`, `?preset=harness-update-progress` | none | no |
| Updates | Update check: network or signature failure, installer error | `ui/updatesContract.ts`, `ui/UpdatesPanel.tsx` | partly: only the `error` state; the specific failure texts are not each seeded | a `?update=` value per failure | no |
| Usage and limits | Usage analytics page (overview, panels) | `ui/AnalyticsView.tsx:49` | yes: `/?open=usage` (inspector "Usage") | none | no |
| Usage and limits | Usage analytics: no data | `ui/AnalyticsView.tsx:49` | yes: `/?open=usage&analytics=empty` | none | no |
| Usage and limits | Usage analytics: failed to load | `ui/AnalyticsView.tsx:49` | yes: `/?open=usage&analytics=error` | none | no |
| Usage and limits | Usage bar in the sidebar and the main window | `ui/SidebarUsage.tsx:18`, `ui/QuotaBar.tsx:74` | yes: `/?usage-bar` (inspector "Usage bar on") | none | no |
| Usage and limits | Usage panel (every limit, from the bar) | `ui/QuotaBar.tsx:59` (`UsagePanel`) | yes: `/?usage-bar&idle` | none | no |
| Usage and limits | Usage at 90% with a tip | `ui/UsageTip.tsx` | yes: `/?preset=usage-tip-90` | none | no |
| Usage and limits | No usage data | `ui/QuotaBar.tsx:74` | yes: `/?no-quota` | none | no |
| Usage and limits | Usage could not load (provider unauthorized) | `ui/QuotaBar.tsx:74` | yes: `/?quota-error` | none | no |
| Usage and limits | Usage: provider usage read refused (no sign-in) | `ui/QuotaBar.tsx` | no: the sign-in-read path has no fixture state of "not signed in" beyond the error flag | a `?quota-error=signin` variant | no |
| Usage and limits | Usage limit reached: handover to another coding agent | `ui/errorCopy.ts:101`, `ui/TeamMessages.tsx` | yes: inspector "Hand over at a limit" | none | no |
| Usage and limits | Quota refresh (manual) | `quota.refresh` | yes: handled through the `quota.` path (`fixtureBridge.ts`) | none | no |
| Usage and limits | Budget (retired) | `crates` (`budget.*` returns unsupported) | n/a: retired by design | none | n/a |
| Schedules and automations | Schedules: empty | `ui/SchedulesView.tsx:110` | yes: `/?open=schedules` (inspector "Schedules, empty") | none | no |
| Schedules and automations | Schedules: with failed and quiet runs | `ui/SchedulesView.tsx:110`, `ui/ScheduleOverview.tsx:51` | yes: `/?open=schedules&preset=schedule-retry` (inspector) | none | no |
| Schedules and automations | Schedule overview (upcoming, recent runs, spend) | `ui/ScheduleOverview.tsx:51` | partly: renders the seed; `automation.get` is not handled (prod-only RPC name) | a branch | yes (in preview: fixture branch) |
| Schedules and automations | New schedule form, cron and time validation | `ui/SchedulesView.tsx:110`, `fixtureBridge.ts:1020-1029` | yes: click path | none | no |
| Schedules and automations | Pause and resume a schedule | `automation.pause`, `automation.resume` | yes: handled | none | no |
| Schedules and automations | Run cancelled by spend check or budget stop | `fixtureBridge.ts:121` (reason `spend_check`) | yes: seeded run | none | no |
| Schedules and automations | Automations paused globally | `ui/SchedulesView.tsx` (`automations.paused`) | partly: setting toggles; global pause effect not shown | none | no |
| Schedules and automations | Run changed live (`automation.run.changed`) | event | no: not emitted | an emitter | yes (in preview: emitter) |
| Schedules and automations | Schedule target missing ("That schedule is gone.") | `fixtureBridge.ts:1032`, `:1044` | partly: click path | none | no |
| Vault | Vault list (logins, cards, API keys, notes) | `ui/VaultPage.tsx:182` | yes: `/?open=settings:Vault` (no inspector scenario) | none | no |
| Vault | Vault empty | `ui/VaultPage.tsx:182` | yes: `?vault=empty` | none | no |
| Vault | Use request waiting (approve the card) | `ui/VaultPage.tsx:290` (`VaultUseRequests`) | yes: `?vault=request` | none | no |
| Vault | Reveal a value, TOTP code | `ui/VaultPage.tsx` (`vault.reveal`, `vault.totp`) | yes: handled (`fixtureBridge.ts:2287`) | none | no |
| Vault | Activity ledger (content-free) | `ui/VaultPage.tsx`, `vault.activity` | yes: handled | none | no |
| Vault | Create, edit, delete (validation: card number, origin) | `ui/VaultPage.tsx:182` (`cardNumberProblem`, `originProblem`) | yes: click path | none | no |
| Vault | Vault locked or Credential Manager unavailable | `ui/VaultPage.tsx` | no: no fixture state for "locked" or a storage failure | a `?vault=locked` flag | no |
| Vault | Vault changed live (`vault.changed`, `vault.use.requested`, `vault.use.updated`) | events | no: not emitted | an emitter | yes (in preview: emitter) |
| Dictation | Dictation island: listening, model loading (warming) | `ui/DictationIsland.tsx:18`, `ui/DictationIsland.tsx:13` (`IslandPhase`) | yes: `/dictation-preview.html?state=listening`, gallery "Dictation" | none | no |
| Dictation | Dictation island: transcribing (processing) | `ui/DictationIsland.tsx:13` | yes: `?state=processing` | none | no |
| Dictation | Dictation island: done | `ui/DictationIsland.tsx:13` | yes: `?state=done` | none | no |
| Dictation | Dictation island: nothing heard (silent) | `ui/DictationIsland.tsx:13` | yes: `?state=silent` | none | no |
| Dictation | Dictation island: error ("No microphone is available...") | `ui/DictationIsland.tsx`, `dictationPreview.tsx:43` | yes: `?state=error` (text is seeded in the preview) | none | no |
| Dictation | Dictation live loop (mic levels, words) | `ui/App.tsx:4289`, `:4310`, `ui/dictation.ts` | no: the preview has no microphone; the live loop is a scripted replay in `dictation-preview.html`, not the real path | a real audio source (not available) | no |
| Dictation | Dictation in the companion | `ui/App.tsx:4373` (`dictationIsland`) | yes: `dictation-preview.html?surface=companion` (gallery "Companion" variant) | none | no |
| Dictation | Cloud dictation: "Codex is not signed in" | `ui/dictation.ts` (cloud error copy) | no: the cloud path has no fixture; only the island's text is seeded | a cloud dictation fixture that fails with each message | no (cloud path is a backend call, but the preview can stub it) |
| Dictation | Cloud dictation: "ChatGPT refused the recording", "rate limiting", "could not be reached", "sent back something unreadable" | `ui/dictation.ts` | no: same as above | a stub for each failure | no |
| Dictation | Speech model dialog (choose, download) | `ui/SpeechModelDialog.tsx:11` | partly: renders; download or failure not seeded | a model download stub | yes (in preview: fixture branch) |
| Dictation | Dictation sound cues | `ui/dictationSound.ts` | partly: plays in the browser only where allowed | none | no |
| Dictation | Dictation shortcut in use (key conflict) | `ui/KeybindingEditor.tsx` | partly | a binding that clashes | no |
| Companion (island) | Compact bar (idle) | `blob/companionFsm.ts`, `ui/App.tsx:3720` (`Companion`) | yes: `/?companion` (gallery "Companion"; inspector "Compact bar") | none | no |
| Companion (island) | Overview (home) | `ui/App.tsx:3720` | yes: `/?companion&mode=home` | none | no |
| Companion (island) | Chat in the island | `ui/CompanionTeam.tsx`, `ui/App.tsx:4374` (`chatShown`) | yes: `/?companion&mode=home&view=chat` | none | no |
| Companion (island) | Activity | `ui/CompanionPanels.tsx:56` (`CompanionActivitySummary`) | yes: `/?companion&mode=home&view=activity` (inspector "Activity") | none | no |
| Companion (island) | Settings (in the island) | `ui/CompanionPanels.tsx:107` | yes: `/?companion&mode=home&view=settings` | none | no |
| Companion (island) | Focus mode | `ui/companionPrefs.ts`, `ui/App.tsx` (`focusModeFromSettings`) | yes: `/?companion&focus` | none | no |
| Companion (island) | Approval in the island | `ui/CompanionAttention.tsx:40` | yes: `/?companion&mode=home&view=chat&scene=approval-shell` | none | no |
| Companion (island) | Question in the island | `ui/CompanionAttention.tsx` | yes: `scene=question-single` | none | no |
| Companion (island) | Proposal in the island | `ui/ProposalCard.tsx` | yes: `scene=proposal-task` | none | no |
| Companion (island) | Secret request in the island | `ui/SecretsPage.tsx:107` | yes: `secretrequest=1` | none | no |
| Companion (island) | At the usage limit | `ui/companionStatus.ts` (`rate_limited` mood), `ui/TeamMessages.tsx` | yes: `scene=limited` | none | no |
| Companion (island) | Long replies (scrolling in the island) | `ui/CompanionTeam.tsx` (`CompanionTeamActivity`) | yes: `scene=markdown` | none | no |
| Companion (island) | Another computer in the island | `ui/CompanionPanels.tsx:91` (`CompanionRemoteRow`) | yes: `&machines=1` | none | no |
| Companion (island) | Fleet of helpers | `ui/SubbotViews.tsx:55`, `ui/CompanionFleetChip` | yes: `&subbots=1` | none | no |
| Companion (island) | Peek bubble (finished, attention) | `blob/companionPeek.ts`, `ui/companionPeek.ts` | partly: the peek is timed off live state transitions; a static `scene` shows the bubble only if seeded | a timed transition | no |
| Companion (island) | Working, tool activity, file activity moods | `ui/companionStatus.ts:63-80` | partly: static scenes show the working mood; the transitions are live-only | a live emitter | yes (in preview: emitter) |
| Companion (island) | Drag, resize, placement, monitor choice | `ui/companionDrag.ts`, `ui/CompanionResize.tsx:36`, `ui/companionPositions.ts` | partly: window drag and native resize are desktop-only; the fixture stubs `companionResize*` (`fixtureBridge.ts:2004-2014`) | none (visual only) | no |
| Companion (island) | Drop a file on the island (chips) | `ui/companionDrop.ts`, `ui/CompanionAttention.tsx` | partly: native drag and drop only | a drop simulation | no |
| Companion (island) | Needs-you bell (waiting bots) | `ui/NeedsYou.tsx:52` (`NeedsYouBell`) | partly: renders from approvals; no dedicated scene | a scene with a waiting bot | no |
| Companion (island) | Companion sound and volume | `ui/SoundVolumeSlider.tsx`, `blob/soundCues.ts` | partly: plays in the browser where allowed | none | no |
| Looks and appearance | Look picker (Haze, Stone) | `ui/LookPicker.tsx:22` | yes: onboarding and Settings > Appearance | none | no |
| Looks and appearance | Look editor (colour, orb, art) | `ui/LookEditor.tsx:17` | yes: bot editor | none | no |
| Looks and appearance | Bot art variants (3D robot, Glossy, Hand-painted, portraits, logo marks) | `blob/art/art.ts`, `blob/BlobCanvas.tsx` | yes: `/preview.html` (character sheet), `/redesign-preview.html?bots=glossy` | none | no |
| Looks and appearance | Chat background (dither and others) | `ui/ChatBackground.tsx:38` | yes: inspector "Appearance, chat background" (`?chatbg=dither`) | none | no |
| Looks and appearance | Window wallpaper | `ui/AppWallpaper.tsx`, `ui/windowBackdrop.ts` | yes: `?wallpaper=…` | none | no |
| Looks and appearance | Density, font, accent, text size | `ui/DensityControl.tsx:23`, `ui/AccentPicker.tsx:36` | yes: `?density=`, `?font=`, `?accent=` | none | no |
| Looks and appearance | Haze tuning sliders | `ui/hazeTuning.ts`, `ui/ChatBackground.tsx` (`HazeTuningSettings`) | partly: renders; native backdrop effect not shown | none | no |
| Errors, banners and toasts | Friendly error: one sentence, detail, action (sign-in, rescan, retry, open settings, open logs, update agent) | `ui/errorCopy.ts:187-306` | partly: the mapper is pure; the screen shows it only when a fixture returns the code (see the error-string table below) | fixture errors with each code | no |
| Errors, banners and toasts | Daemon-level: service restarting, gave up (banner) | `ui/ConnectionBanner.tsx:46-51` | no | see connection and service | yes (in preview: supervisor stub) |
| Errors, banners and toasts | Undo toasts | `ui/ExperimentalFeatures.tsx` (`UndoToasts`), `ui/undo.ts` | partly: appears after a delete (click path) | none | no |
| Errors, banners and toasts | Queue or send failure notice | `ui/PromptQueue.tsx` | partly: needs a failed send | a `session.prompt` error branch | yes (in preview: fixture branch) |
| Errors, banners and toasts | Turn or session error text (process exited, timeout, overloaded, context exhausted, model unavailable, permission denied) | `ui/errorCopy.ts:129-171` | partly: only `process_exited` and usage limit are seeded in scenes | a turn error per code | no |
| Errors, banners and toasts | Update banner and restart prompt | `ui/UpdateBanner.tsx` | yes: `?update=ready` | none | no |
| Errors, banners and toasts | "Something went wrong on your device." (local error) | `ui/errorCopy.ts:156` | no: the preview never produces a local error of that kind | a fixture local error | no |
| Errors, banners and toasts | Native-only features refused (no desktop) | `tauri.ts` stubs (`fixtureBridge.ts:1897-1989`) | partly: each stub does nothing visible | n/a | no |
| Desktop only (OS integration) | Close to tray, autostart, desktop notifications | `ui/App.tsx` (no single line), `ui/notificationTransitions.ts`, `fixtureBridge.ts:371` (`sendDesktopNotification` only records) | no: the preview cannot show OS notifications or the tray | a visible notification stub | no |
| Desktop only (OS integration) | Global hotkey, dictation shortcut registration | `ui/dictation.ts`, `tauri.ts` | no: native | n/a | no |
| Desktop only (OS integration) | Keep awake, sleep and lock handling | `ui/App.tsx` (`power.keepAwake`), Rust | no: native | n/a | no |
| Desktop only (OS integration) | Open in editor, reveal in Explorer, open folder dialog | `fixtureBridge.ts:1961-1963`, `:1897` | no: the stubs return without showing anything | a visible stub result | no |
| Desktop only (OS integration) | Terminal (PTY) sessions | `ui/TerminalDock.tsx:140`, `fixtureBridge.ts:2228` | no: throws | a stand-in terminal | yes (in preview: stub) |
| Desktop only (OS integration) | Import a blob from a file (shared blob) | `ui/ImportDialog.tsx:24`, `fixtureBridge.ts:1914` | yes: `/?preview-import` (gallery "Import a blob") | none | no |
| Desktop only (OS integration) | Backup export and import preview | `ui/BackupPage.tsx:46`, `fixtureBridge.ts:1924-1951` | yes: Settings > Backup (inspector "Backup") | none | no |
| Desktop only (OS integration) | Backup import conflicts and resolutions | `ui/BackupPage.tsx:46` (`importResolutions`) | partly: preview import preview returns one shape | a conflict seed | no |
| Desktop only (OS integration) | Import progress and failures | `ui/ImportDialog.tsx`, `import.progress`, `import.completed` | partly: progress and completed are emitted; a failure is not seeded | an import failure branch | no |
| Desktop only (OS integration) | Import from other tools (conversation sources) | `ui/ImportDialog.tsx` (`ImportTemplate`), `fixtureBridge.ts:486-500` | yes: `?import-seen` and default | none | no |
| Onboarding and tour | In-app tour | `ui/App.tsx` (`onboarding.tour`), `ui/App.tsx:1122` | yes: `?tour`, `?tour-invite` | none | no |
| Onboarding and tour | Hints (seen, reset) | `ui/App.tsx` (`hints.seen`) | yes: `?hints-reset`, `?seen-hints` | none | no |
| Onboarding and tour | What's new (once per version) | `ui/WhatsNew.tsx:74`, `ui/App.tsx:233` | yes: `?open=whats-new` | none | no |

## Summary counts

Rows in the inventory table: 403. Demoable in preview today: 255 yes, 86 partly, 61 no, 1 n/a (retired budget). The "yes" count is generous: many rows are "yes" for the surface, but the exact state may still need a click path. Read "partly" and "no" as the gaps to close.

## Error messages

Every production user-facing error string found by a text sweep of `apps/desktop/src` (not tests, not `/preview/`). The sweep matched literals that look like error text and that sit next to a state setter, a throw, an RPC catch, or a copy field. It is a heuristic and may miss a few, so treat the list as near-complete, not proven exhaustive.

Column "Preview can show it?" uses the rule below. It is per file, and the per-area rows above are the authority when they disagree.

- **yes**: the file's surface is rendered by a preview route and its error comes from a branch the fixture actually takes. Not used in this list, because the trigger is per-line; see the table rows.
- **partly**: the surface renders in the preview, but the specific message needs a fixture error or state the preview does not produce.
- **no**: the file calls an RPC or event the preview does not handle (returns `{}` or never emits), or the surface is not reachable in the preview at all.
- **pure**: a pure helper (copy mapper or contract). The preview cannot trigger it from the UI, but unit tests cover it; a fixture would need to pass the input.

| # | Where (path:line) | Error or user-facing text | Preview can show it? |
|---|---|---|---|
| 1 | apps/desktop/src/executionContract.ts:113 | , message  text //  | pure (unit-tested; a fixture must pass the input) |
| 2 | apps/desktop/src/remote/deviceApi.ts:84 | That code did not work. Check it, or show a new one on the computer. | partly (the surface renders; this message needs a fixture error) |
| 3 | apps/desktop/src/remote/PushCard.tsx:48 | Notifications could not be turned on here. | no (its RPC or event has no preview branch) |
| 4 | apps/desktop/src/remote/PushCard.tsx:57 | Notifications could not be turned off here. | no (its RPC or event has no preview branch) |
| 5 | apps/desktop/src/remote/RemoteApp.tsx:82 | Could not sign in. Try again. | partly (the surface renders; this message needs a fixture error) |
| 6 | apps/desktop/src/remote/RemoteApp.tsx:93 | That code did not work. Check it, or show a new one on the computer. | partly (the surface renders; this message needs a fixture error) |
| 7 | apps/desktop/src/remote/RemoteApp.tsx:214 | The code expired before it was approved. Show a new one on the computer. | partly (the surface renders; this message needs a fixture error) |
| 8 | apps/desktop/src/remote/RemoteApp.tsx:485 | Earlier messages could not be loaded. | partly (the surface renders; this message needs a fixture error) |
| 9 | apps/desktop/src/remote/RemoteApp.tsx:546 | That assignment could not be stopped from here. | partly (the surface renders; this message needs a fixture error) |
| 10 | apps/desktop/src/remote/remoteTauri.ts:154 | internal  Usage analytics could not be loaded. | partly (the surface renders; this message needs a fixture error) |
| 11 | apps/desktop/src/remote/remoteTransport.ts:208 | The host refused the call. | partly (the surface renders; this message needs a fixture error) |
| 12 | apps/desktop/src/ui/AbandonedWorktrees.tsx:38 | That task copy could not be removed. | partly (the surface renders; this message needs a fixture error) |
| 13 | apps/desktop/src/ui/AbandonedWorktrees.tsx:40 | That task copy could not be removed. | partly (the surface renders; this message needs a fixture error) |
| 14 | apps/desktop/src/ui/AbandonedWorktrees.tsx:70 | The task copies could not be listed. | partly (the surface renders; this message needs a fixture error) |
| 15 | apps/desktop/src/ui/AccountsPage.tsx:74 | The sign-in command could not be made. | partly (the surface renders; this message needs a fixture error) |
| 16 | apps/desktop/src/ui/AccountsPage.tsx:82 | The sign-in window could not be opened. Run the command below instead. | partly (the surface renders; this message needs a fixture error) |
| 17 | apps/desktop/src/ui/AccountsPage.tsx:149 | That model could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 18 | apps/desktop/src/ui/agentForm.ts:233 | The local daemon request failed. | partly (the surface renders; this message needs a fixture error) |
| 19 | apps/desktop/src/ui/agentForm.ts:250 | The local daemon request failed. | partly (the surface renders; this message needs a fixture error) |
| 20 | apps/desktop/src/ui/analyticsFormat.ts:648 | The run failed. | pure (unit-tested; a fixture must pass the input) |
| 21 | apps/desktop/src/ui/AnalyticsView.tsx:103 | Usage analytics could not be loaded. | partly (the surface renders; this message needs a fixture error) |
| 22 | apps/desktop/src/ui/App.tsx:1112 | Finishing startup checks | partly (the surface renders; this message needs a fixture error) |
| 23 | apps/desktop/src/ui/App.tsx:2177 | Could not start the conversation. Try again. | partly (the surface renders; this message needs a fixture error) |
| 24 | apps/desktop/src/ui/App.tsx:2219 | Queued because this turn could not accept steering. It sends after the turn ends. | partly (the surface renders; this message needs a fixture error) |
| 25 | apps/desktop/src/ui/App.tsx:2246 | Queued because this turn could not accept steering. It sends after the turn ends. | partly (the surface renders; this message needs a fixture error) |
| 26 | apps/desktop/src/ui/App.tsx:2327 | The clipboard does not contain a readable image. | partly (the surface renders; this message needs a fixture error) |
| 27 | apps/desktop/src/ui/App.tsx:2447 | This turn could not take the message. It is next in the queue. | partly (the surface renders; this message needs a fixture error) |
| 28 | apps/desktop/src/ui/App.tsx:2502 | This conversation is no longer available. | partly (the surface renders; this message needs a fixture error) |
| 29 | apps/desktop/src/ui/App.tsx:2531 | This approval expired or is no longer pending. No reply was sent. Check the refreshed request state before continuing. | partly (the surface renders; this message needs a fixture error) |
| 30 | apps/desktop/src/ui/App.tsx:2818 | Could not start the conversation. Try again. | partly (the surface renders; this message needs a fixture error) |
| 31 | apps/desktop/src/ui/App.tsx:3325 | This conversation is no longer available. | partly (the surface renders; this message needs a fixture error) |
| 32 | apps/desktop/src/ui/App.tsx:3972 | Windows did not report a file path. Choose a local file again. | partly (the surface renders; this message needs a fixture error) |
| 33 | apps/desktop/src/ui/App.tsx:4599 | The local daemon request failed. | partly (the surface renders; this message needs a fixture error) |
| 34 | apps/desktop/src/ui/App.tsx:4603 | The local daemon request failed. | partly (the surface renders; this message needs a fixture error) |
| 35 | apps/desktop/src/ui/ApprovalRulesPage.tsx:25 | Saved approvals could not be read. | no (its RPC or event has no preview branch) |
| 36 | apps/desktop/src/ui/ApprovalRulesPage.tsx:36 | That approval could not be removed. Try again. | no (its RPC or event has no preview branch) |
| 37 | apps/desktop/src/ui/ApprovalRulesPage.tsx:42 | That approval could not be changed. Try again. | no (its RPC or event has no preview branch) |
| 38 | apps/desktop/src/ui/BackupPage.tsx:68 | The save dialog could not open. | partly (the surface renders; this message needs a fixture error) |
| 39 | apps/desktop/src/ui/BackupPage.tsx:78 | The template could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 40 | apps/desktop/src/ui/BackupPage.tsx:92 | The backup could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 41 | apps/desktop/src/ui/BackupPage.tsx:99 | The file dialog could not open. | partly (the surface renders; this message needs a fixture error) |
| 42 | apps/desktop/src/ui/BackupPage.tsx:109 | That file could not be read as a Retinue backup. | partly (the surface renders; this message needs a fixture error) |
| 43 | apps/desktop/src/ui/BackupPage.tsx:128 | The backup could not be restored. | partly (the surface renders; this message needs a fixture error) |
| 44 | apps/desktop/src/ui/BlobPage.tsx:357 | The action could not be completed. Try again. | partly (the surface renders; this message needs a fixture error) |
| 45 | apps/desktop/src/ui/BlobSettings.tsx:135 | The coding agent could not be reached. | partly (the surface renders; this message needs a fixture error) |
| 46 | apps/desktop/src/ui/BlobSettings.tsx:201 | The coding agent could not be reached. | partly (the surface renders; this message needs a fixture error) |
| 47 | apps/desktop/src/ui/BlobSettings.tsx:589 | No computer is shared with employees right now, so this does nothing until you share one in Settings, Integrations, Computer. | partly (the surface renders; this message needs a fixture error) |
| 48 | apps/desktop/src/ui/BotMemoryGroup.tsx:82 | Notes could not be loaded. | partly (the surface renders; this message needs a fixture error) |
| 49 | apps/desktop/src/ui/BotSkillsGroup.tsx:38 | Skills could not be loaded. | partly (the surface renders; this message needs a fixture error) |
| 50 | apps/desktop/src/ui/BotSkillsGroup.tsx:50 | That skill could not be changed. | partly (the surface renders; this message needs a fixture error) |
| 51 | apps/desktop/src/ui/busySend.ts:56 | Steering is unavailable for this coding agent. Your message waits for the turn to end. | partly (the surface renders; this message needs a fixture error) |
| 52 | apps/desktop/src/ui/ChangesView.tsx:90 | The repository could not be read just now. | partly (the surface renders; this message needs a fixture error) |
| 53 | apps/desktop/src/ui/ChangesView.tsx:100 | This diff could not be read. | partly (the surface renders; this message needs a fixture error) |
| 54 | apps/desktop/src/ui/ChangesView.tsx:120 | That could not be staged. | partly (the surface renders; this message needs a fixture error) |
| 55 | apps/desktop/src/ui/ChangesView.tsx:136 | Those changes could not be discarded. | partly (the surface renders; this message needs a fixture error) |
| 56 | apps/desktop/src/ui/ChangesView.tsx:143 | The comments could not be sent. | partly (the surface renders; this message needs a fixture error) |
| 57 | apps/desktop/src/ui/ChannelsPage.tsx:38 |   return channel.detail ??  | partly (the surface renders; this message needs a fixture error) |
| 58 | apps/desktop/src/ui/ChannelsPage.tsx:162 | That could not be done. | partly (the surface renders; this message needs a fixture error) |
| 59 | apps/desktop/src/ui/ChatBackground.tsx:58 | The image could not be removed. | partly (the surface renders; this message needs a fixture error) |
| 60 | apps/desktop/src/ui/chatBackgroundStore.ts:63 | The image store could not be opened. | partly (the surface renders; this message needs a fixture error) |
| 61 | apps/desktop/src/ui/chatBackgroundStore.ts:73 | The image could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 62 | apps/desktop/src/ui/chatBackgroundStore.ts:89 | The image could not be prepared. | partly (the surface renders; this message needs a fixture error) |
| 63 | apps/desktop/src/ui/chatBackgroundStore.ts:102 | That image could not be read. | partly (the surface renders; this message needs a fixture error) |
| 64 | apps/desktop/src/ui/chatBackgroundStore.ts:109 | The picture could not be loaded. | partly (the surface renders; this message needs a fixture error) |
| 65 | apps/desktop/src/ui/chatBackgroundStore.ts:130 | That picture could not be loaded. | partly (the surface renders; this message needs a fixture error) |
| 66 | apps/desktop/src/ui/companionStatus.ts:16 | Something failed | partly (the surface renders; this message needs a fixture error) |
| 67 | apps/desktop/src/ui/companionStatus.ts:47 | Coding agent check failed | partly (the surface renders; this message needs a fixture error) |
| 68 | apps/desktop/src/ui/companionStatus.ts:54 | Needs attention | partly (the surface renders; this message needs a fixture error) |
| 69 | apps/desktop/src/ui/ComposerControls.tsx:346 | The image could not be prepared. | partly (the surface renders; this message needs a fixture error) |
| 70 | apps/desktop/src/ui/ComputerPage.tsx:150 | That could not be done. | partly (the surface renders; this message needs a fixture error) |
| 71 | apps/desktop/src/ui/ComputerView.tsx:61 | The screen cannot be shown inside Retinue yet. | partly (the surface renders; this message needs a fixture error) |
| 72 | apps/desktop/src/ui/ComputerView.tsx:65 | Your server did not answer. | partly (the surface renders; this message needs a fixture error) |
| 73 | apps/desktop/src/ui/ConnectionsPage.tsx:89 | The connection settings could not be read. | partly (the surface renders; this message needs a fixture error) |
| 74 | apps/desktop/src/ui/ConnectionsPage.tsx:109 | That could not be changed. | partly (the surface renders; this message needs a fixture error) |
| 75 | apps/desktop/src/ui/ConnectionsPage.tsx:117 | The command could not be copied. Select it and copy it yourself. | partly (the surface renders; this message needs a fixture error) |
| 76 | apps/desktop/src/ui/ConnectorsPage.tsx:77 | The tools could not be listed. | no (its RPC or event has no preview branch) |
| 77 | apps/desktop/src/ui/ConnectorsPage.tsx:80 | That rule could not be saved. | no (its RPC or event has no preview branch) |
| 78 | apps/desktop/src/ui/ConnectorsPage.tsx:100 | The sign-in could not start. | no (its RPC or event has no preview branch) |
| 79 | apps/desktop/src/ui/ConnectorsPage.tsx:134 |  ? "Employees can | no (its RPC or event has no preview branch) |
| 80 | apps/desktop/src/ui/ConnectorsPage.tsx:152 | Connectors could not be read. | no (its RPC or event has no preview branch) |
| 81 | apps/desktop/src/ui/ConnectorsPage.tsx:167 | That could not be done. | no (its RPC or event has no preview branch) |
| 82 | apps/desktop/src/ui/ConnectorsPage.tsx:184 | That connector could not be added. | no (its RPC or event has no preview branch) |
| 83 | apps/desktop/src/ui/DelegationGroup.tsx:18 | , interrupted   | no (its RPC or event has no preview branch) |
| 84 | apps/desktop/src/ui/delegations.ts:185 |  ? message.content    | partly (the surface renders; this message needs a fixture error) |
| 85 | apps/desktop/src/ui/delegations.ts:232 | , interrupted   | partly (the surface renders; this message needs a fixture error) |
| 86 | apps/desktop/src/ui/delegations.ts:240 | Waiting for you | partly (the surface renders; this message needs a fixture error) |
| 87 | apps/desktop/src/ui/DiagnosticsDialog.tsx:28 | The diagnostics list could not be read. | no (its RPC or event has no preview branch) |
| 88 | apps/desktop/src/ui/DiagnosticsDialog.tsx:47 | The save dialog could not open. | no (its RPC or event has no preview branch) |
| 89 | apps/desktop/src/ui/DiagnosticsDialog.tsx:54 | The diagnostics file could not be saved. | no (its RPC or event has no preview branch) |
| 90 | apps/desktop/src/ui/dictation.ts:187 | Dictation failed. | partly (the surface renders; this message needs a fixture error) |
| 91 | apps/desktop/src/ui/DictationBubble.tsx:74 | Dictation failed. | partly (the surface renders; this message needs a fixture error) |
| 92 | apps/desktop/src/ui/errorCopy.ts:88 | It is rate limited. Try again in a moment. | pure (unit-tested; a fixture must pass the input) |
| 93 | apps/desktop/src/ui/errorCopy.ts:108 | The coding agent is not available. | pure (unit-tested; a fixture must pass the input) |
| 94 | apps/desktop/src/ui/errorCopy.ts:115 | That model is not available. | pure (unit-tested; a fixture must pass the input) |
| 95 | apps/desktop/src/ui/errorCopy.ts:123 | No answer arrived in time. Try again. | pure (unit-tested; a fixture must pass the input) |
| 96 | apps/desktop/src/ui/errorCopy.ts:130 | Its process exited before it finished. Try again. | pure (unit-tested; a fixture must pass the input) |
| 97 | apps/desktop/src/ui/errorCopy.ts:136 | Permission was denied. | pure (unit-tested; a fixture must pass the input) |
| 98 | apps/desktop/src/ui/errorCopy.ts:137 | A required action was not allowed. | pure (unit-tested; a fixture must pass the input) |
| 99 | apps/desktop/src/ui/errorCopy.ts:142 | That is no longer available. | pure (unit-tested; a fixture must pass the input) |
| 100 | apps/desktop/src/ui/errorCopy.ts:149 | That action does not match the current state. | pure (unit-tested; a fixture must pass the input) |
| 101 | apps/desktop/src/ui/errorCopy.ts:156 | Something went wrong on your device. | pure (unit-tested; a fixture must pass the input) |
| 102 | apps/desktop/src/ui/errorCopy.ts:157 | Try again. If it keeps happening, open the logs. | pure (unit-tested; a fixture must pass the input) |
| 103 | apps/desktop/src/ui/errorCopy.ts:165 | That did not work. | pure (unit-tested; a fixture must pass the input) |
| 104 | apps/desktop/src/ui/errorCopy.ts:173 | Try again, or start a new conversation. | pure (unit-tested; a fixture must pass the input) |
| 105 | apps/desktop/src/ui/errorCopy.ts:274 | Something went wrong. Try again. | pure (unit-tested; a fixture must pass the input) |
| 106 | apps/desktop/src/ui/ExperimentalFeatures.tsx:54 | Could not search conversations. Try again. | partly (the surface renders; this message needs a fixture error) |
| 107 | apps/desktop/src/ui/FanoutCompare.tsx:39 | A removed employee | partly (the surface renders; this message needs a fixture error) |
| 108 | apps/desktop/src/ui/FanoutCompare.tsx:44 | , waiting_permission   | partly (the surface renders; this message needs a fixture error) |
| 109 | apps/desktop/src/ui/FanoutCompare.tsx:93 | The comparison could not be started. | partly (the surface renders; this message needs a fixture error) |
| 110 | apps/desktop/src/ui/FanoutCompare.tsx:95 | The comparison could not be started. | partly (the surface renders; this message needs a fixture error) |
| 111 | apps/desktop/src/ui/FanoutCompare.tsx:169 | That choice could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 112 | apps/desktop/src/ui/FanoutCompare.tsx:173 | That comparison could not be removed. | partly (the surface renders; this message needs a fixture error) |
| 113 | apps/desktop/src/ui/FirstRunOnboarding.tsx:39 | The file dialog could not open. | partly (the surface renders; this message needs a fixture error) |
| 114 | apps/desktop/src/ui/FirstRunOnboarding.tsx:42 | That file could not be read as a Retinue template. | partly (the surface renders; this message needs a fixture error) |
| 115 | apps/desktop/src/ui/FirstRunOnboarding.tsx:50 | The template could not be brought in. | partly (the surface renders; this message needs a fixture error) |
| 116 | apps/desktop/src/ui/FirstRunOnboarding.tsx:107 | A verified model list is unavailable. Scan again or choose another coding agent. | partly (the surface renders; this message needs a fixture error) |
| 117 | apps/desktop/src/ui/FirstRunOnboarding.tsx:131 | Clipboard is unavailable. | partly (the surface renders; this message needs a fixture error) |
| 118 | apps/desktop/src/ui/GeneratedImage.tsx:21 |  ? row.status    | partly (the surface renders; this message needs a fixture error) |
| 119 | apps/desktop/src/ui/GeneratedImage.tsx:46 | This image could not be copied. | partly (the surface renders; this message needs a fixture error) |
| 120 | apps/desktop/src/ui/GeneratedImage.tsx:60 | , text  reason instanceof Error && reason.message ? reason.message    | partly (the surface renders; this message needs a fixture error) |
| 121 | apps/desktop/src/ui/GeneratedImage.tsx:65 | This image could not be copied. | partly (the surface renders; this message needs a fixture error) |
| 122 | apps/desktop/src/ui/GeneratedImage.tsx:85 |  ? state.message   state.reply.reason ??  | partly (the surface renders; this message needs a fixture error) |
| 123 | apps/desktop/src/ui/HarnessUpdatesNotice.tsx:64 | , message  error instanceof Error ? error.message    | partly (the surface renders; this message needs a fixture error) |
| 124 | apps/desktop/src/ui/HealthPage.tsx:30 | The health check could not run. Is the Retinue service connected? | no (its RPC or event has no preview branch) |
| 125 | apps/desktop/src/ui/ImportDialog.tsx:68 | The conversations could not be listed. | partly (the surface renders; this message needs a fixture error) |
| 126 | apps/desktop/src/ui/ImportDialog.tsx:93 | Some conversations could not be given out. | partly (the surface renders; this message needs a fixture error) |
| 127 | apps/desktop/src/ui/launchChecks.ts:76 | Stopped at the overall startup time limit. | pure (unit-tested; a fixture must pass the input) |
| 128 | apps/desktop/src/ui/launchChecks.ts:78 | Startup checks reached their overall time limit. | pure (unit-tested; a fixture must pass the input) |
| 129 | apps/desktop/src/ui/launchChecks.ts:102 | This check could not be completed. | pure (unit-tested; a fixture must pass the input) |
| 130 | apps/desktop/src/ui/LookEditor.tsx:47 | That picture could not be read. | partly (the surface renders; this message needs a fixture error) |
| 131 | apps/desktop/src/ui/MachineConversation.tsx:60 | This conversation could not be checked. | partly (the surface renders; this message needs a fixture error) |
| 132 | apps/desktop/src/ui/MachineConversation.tsx:84 | This conversation could not be taken over. | partly (the surface renders; this message needs a fixture error) |
| 133 | apps/desktop/src/ui/MachineConversation.tsx:91 | Take over this conversation? | partly (the surface renders; this message needs a fixture error) |
| 134 | apps/desktop/src/ui/MachineConversation.tsx:111 |  ? step.detail ??  | partly (the surface renders; this message needs a fixture error) |
| 135 | apps/desktop/src/ui/MachinesPage.tsx:31 | That name could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 136 | apps/desktop/src/ui/MachinesPage.tsx:99 | That address could not be checked. | partly (the surface renders; this message needs a fixture error) |
| 137 | apps/desktop/src/ui/MachinesPage.tsx:110 | Pairing could not start. | partly (the surface renders; this message needs a fixture error) |
| 138 | apps/desktop/src/ui/MachinesPage.tsx:121 | Nobody answered in time. Try again. | partly (the surface renders; this message needs a fixture error) |
| 139 | apps/desktop/src/ui/MediaPreview.tsx:167 |  ? undefined    | partly (the surface renders; this message needs a fixture error) |
| 140 | apps/desktop/src/ui/MessageActions.tsx:111 | Could not save your reaction. Try again. | partly (the surface renders; this message needs a fixture error) |
| 141 | apps/desktop/src/ui/NativeRemote.tsx:181 | Windows didn’t remove the rule. | no (its RPC or event has no preview branch) |
| 142 | apps/desktop/src/ui/NewProjectDialog.tsx:88 | The project could not be created. | partly (the surface renders; this message needs a fixture error) |
| 143 | apps/desktop/src/ui/NewProjectDialog.tsx:93 | Not everything worked | partly (the surface renders; this message needs a fixture error) |
| 144 | apps/desktop/src/ui/OutsideSessions.tsx:84 | Claude Code’s settings could not be changed. | partly (the surface renders; this message needs a fixture error) |
| 145 | apps/desktop/src/ui/PiPage.tsx:29 | The key could not be tested. | no (its RPC or event has no preview branch) |
| 146 | apps/desktop/src/ui/ProjectSearch.tsx:93 | The search could not run. | partly (the surface renders; this message needs a fixture error) |
| 147 | apps/desktop/src/ui/ProjectSettingsDialog.tsx:73 | That could not be done. | partly (the surface renders; this message needs a fixture error) |
| 148 | apps/desktop/src/ui/ProjectSettingsDialog.tsx:129 | These settings could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 149 | apps/desktop/src/ui/ProjectSources.tsx:84 | The clone could not start. | partly (the surface renders; this message needs a fixture error) |
| 150 | apps/desktop/src/ui/ProjectSources.tsx:85 | The clone could not start. | partly (the surface renders; this message needs a fixture error) |
| 151 | apps/desktop/src/ui/ProjectSources.tsx:138 | That folder could not be searched. | partly (the surface renders; this message needs a fixture error) |
| 152 | apps/desktop/src/ui/PullRequestDialog.tsx:105 | Not everything worked | partly (the surface renders; this message needs a fixture error) |
| 153 | apps/desktop/src/ui/reactbits/MetallicPaint.tsx:325 | MetallicPaint  failed to load the image | partly (the surface renders; this message needs a fixture error) |
| 154 | apps/desktop/src/ui/RemoteSettings.tsx:75 | That could not be saved. | no (its RPC or event has no preview branch) |
| 155 | apps/desktop/src/ui/RemoteSettings.tsx:79 | s name, never what it | no (its RPC or event has no preview branch) |
| 156 | apps/desktop/src/ui/ReportProblemDialog.tsx:45 | The issue could not be posted. | partly (the surface renders; this message needs a fixture error) |
| 157 | apps/desktop/src/ui/ReportProblemDialog.tsx:50 | The report could not be copied. Select it and copy it yourself. | partly (the surface renders; this message needs a fixture error) |
| 158 | apps/desktop/src/ui/ReportProblemDialog.tsx:57 |  GitHub did not return its address. | partly (the surface renders; this message needs a fixture error) |
| 159 | apps/desktop/src/ui/runtimeDiscovery.ts:22 | The coding agent check failed. | partly (the surface renders; this message needs a fixture error) |
| 160 | apps/desktop/src/ui/runtimeDiscovery.ts:66 | Finding coding agents timed out. | partly (the surface renders; this message needs a fixture error) |
| 161 | apps/desktop/src/ui/runtimeDiscovery.ts:75 | Finding coding agents timed out. | partly (the surface renders; this message needs a fixture error) |
| 162 | apps/desktop/src/ui/runtimeDiscovery.ts:76 | Could not complete the scan. Check the Retinue service and try again. | partly (the surface renders; this message needs a fixture error) |
| 163 | apps/desktop/src/ui/RuntimeProblemMarker.tsx:14 | Its sign-in expired, so messages to it fail. | partly (the surface renders; this message needs a fixture error) |
| 164 | apps/desktop/src/ui/RuntimeProblemMarker.tsx:21 | The coding agent could not be checked. | partly (the surface renders; this message needs a fixture error) |
| 165 | apps/desktop/src/ui/ScanRecovery.tsx:52 | Clipboard is unavailable. | partly (the surface renders; this message needs a fixture error) |
| 166 | apps/desktop/src/ui/ScheduleOverview.tsx:16 | , invalid_schedule   | no (its RPC or event has no preview branch) |
| 167 | apps/desktop/src/ui/SchedulesView.tsx:89 | Run the project test suite. Report what failed and the likely cause for each failure. If everything passes, say so in one line. | partly (the surface renders; this message needs a fixture error) |
| 168 | apps/desktop/src/ui/SchedulesView.tsx:95 | Paused after three failed runs | partly (the surface renders; this message needs a fixture error) |
| 169 | apps/desktop/src/ui/SecretsPage.tsx:57 | s removed from the Windows Credential Manager, and it can | partly (the surface renders; this message needs a fixture error) |
| 170 | apps/desktop/src/ui/SecretsPage.tsx:86 | That could not be done. | partly (the surface renders; this message needs a fixture error) |
| 171 | apps/desktop/src/ui/SessionExecutionControls.tsx:169 | The model changed for this conversation, but Retinue could not save it as the default for new ones. | partly (the surface renders; this message needs a fixture error) |
| 172 | apps/desktop/src/ui/SessionExecutionControls.tsx:188 | The setting changed for this conversation, but Retinue could not save it as the default for its new ones. | partly (the surface renders; this message needs a fixture error) |
| 173 | apps/desktop/src/ui/SessionRail.tsx:179 | The repository could not be read just now, so these changes may be out of date. Try again first. | no (its RPC or event has no preview branch) |
| 174 | apps/desktop/src/ui/SettingsSheet.tsx:903 | The local daemon request failed. | partly (the surface renders; this message needs a fixture error) |
| 175 | apps/desktop/src/ui/SignInHelp.tsx:24 | The terminal could not be opened. | no (its RPC or event has no preview branch) |
| 176 | apps/desktop/src/ui/SkillsPage.tsx:121 | Skills could not be loaded. | no (its RPC or event has no preview branch) |
| 177 | apps/desktop/src/ui/SkillsPage.tsx:158 | Your coding agents’ skills could not be read. | no (its RPC or event has no preview branch) |
| 178 | apps/desktop/src/ui/SkillsPage.tsx:307 | That could not be saved. | no (its RPC or event has no preview branch) |
| 179 | apps/desktop/src/ui/SkillsPage.tsx:336 | This skill could not be read. | no (its RPC or event has no preview branch) |
| 180 | apps/desktop/src/ui/subbots.ts:58 | Waiting for a slot | partly (the surface renders; this message needs a fixture error) |
| 181 | apps/desktop/src/ui/SubbotViews.tsx:65 | The helpers could not be stopped. | no (its RPC or event has no preview branch) |
| 182 | apps/desktop/src/ui/TaskForm.tsx:109 | Remove the image that could not be attached. | partly (the surface renders; this message needs a fixture error) |
| 183 | apps/desktop/src/ui/taskModel.ts:6 | , awaiting_review   | partly (the surface renders; this message needs a fixture error) |
| 184 | apps/desktop/src/ui/taskModel.ts:53 | Skipped  the last run was still going | partly (the surface renders; this message needs a fixture error) |
| 185 | apps/desktop/src/ui/UpdatesPanel.tsx:117 |  // loadFailed ?  | partly (the surface renders; this message needs a fixture error) |
| 186 | apps/desktop/src/ui/VaultPage.tsx:137 | That could not be saved. | partly (the surface renders; this message needs a fixture error) |
| 187 | apps/desktop/src/ui/VaultPage.tsx:215 | That could not be revealed. | partly (the surface renders; this message needs a fixture error) |
| 188 | apps/desktop/src/ui/VaultPage.tsx:255 | s removed from the vault and from Windows Credential Manager. Employees can | partly (the surface renders; this message needs a fixture error) |
| 189 | apps/desktop/src/ui/VaultPage.tsx:271 | That could not be done. | partly (the surface renders; this message needs a fixture error) |
| 190 | apps/desktop/src/ui/WebhookPanel.tsx:39 | That could not be done. | partly (the surface renders; this message needs a fixture error) |

## Events the preview never emits

The UI handles these daemon events, and the preview's emitters (`notifyFixtureDaemon` and the remote simulator's `emit`, 33 distinct event types) never send them. Live arrival or change of these can only be shown by adding an emitter.

- Conversation turns: `turn.completed`, `turn.error`, `message.delta`, `tool.changed`, `session.plan`, `session.note`, `session.context.updated`, `usage.updated`, `permission.requested`, `permission.auto_resolved`, `permission.bypass_active`, `permission.rules.changed`, `replay.gap`.
- Approvals and secrets: `secret.request.changed`, `owner_computer.grant`.
- Delegation and helpers: `delegation.updated`, `subbot.changed`.
- Tasks and schedules: `task.run.changed`, `task.run.finished`, `automation.run.changed`.
- Vault: `vault.changed`, `vault.use.requested`, `vault.use.updated`.
- Channels and connectors: `channel.updated`, `channel.qr`, `channel.disconnect`, `connector.oauth.changed`, `connector.add`, `connector.remove`, `connector.update`.
- Pi: `pi.key.set`, `pi.key.delete`.
- Machines and remote: `machine.changed`, `machine.session.changed`, `remote.devices.*`, `remote.native.*`, `remote.security.*`, `remote.method.set`.
- Skills and memory: `skills.learned.changed`, `memory.list`, `memory.get`.
- Project and runtime: `project.members.changed`, `runtime.models.changed`, `exec.options.changed`, `settings.changed`, `daemon.health`, `file.changed`, `command.changed`.

The comparison also flagged these as emitted with no matching literal in the UI listeners (a helper may still handle them): `import.completed`, `memory.changed`, `memory.deleted`, `proposal.changed`, `remote.activity`, `remote.changed`, `skills.changed`. Check each before relying on it.

## Top 15 gaps

1. **Stop does nothing in the preview.** `session.cancel` has no branch (`fixtureBridge.ts:1857` returns `{}`), so the Stop control (`ui/App.tsx:1256`, `:2402`, `:2412`, `:2455`) never ends a turn and shows no error.
2. **No live approval, streaming or turn completion.** `permission.requested`, `message.delta`, `turn.completed` and `turn.error` are never emitted, so an approval cannot arrive while the window is open, and a reply cannot stream. Only seeded states appear (`src/preview/fixtureScenes.ts`).
3. **Service restarting and gave-up banners are unreachable.** `daemonSupervisorState` is fixed to `running` (`fixtureBridge.ts:1893`), so `ui/ConnectionBanner.tsx:46-51` never shows.
4. **Health page is always empty.** `doctor.run` has no branch, so `ui/HealthPage.tsx:21-30` shows "The service reported no checks." The failing checks, the Fix buttons and the "could not run" error cannot appear.
5. **Owner's computer has no fixture.** `ui/OwnerComputer.tsx:67` (the live grant bar), `owner_computer.grant` and `owner_computer.stop` are never produced, so the grant, the stop and the owner approval card cannot be shown.
6. **Native remote is absent.** `ui/NativeRemote.tsx:30`, `:44`, `:162` depend on `remote.method.set`, `remote.native.*` and `remote.security.*`, none of which has a branch. The Native method cannot be shown, and a switch does nothing.
7. **Connector OAuth cannot complete.** `ui/ConnectorsPage.tsx:99`, `:150`, `:183` call `connector.oauth.start` and `.status`; the connector branch throws "is not in the preview" (`fixtureBridge.ts:2359`). The sign-in state and its change event never appear.
8. **Learned skills and approval rules are empty and silent.** `ui/SkillsPage.tsx:112`, `:258`, `:280`, `:306` and `ui/ApprovalRulesPage.tsx:23`, `:35`, `:41` have no branches, so review lists stay empty and approve, edit, forget and rule changes show nothing.
9. **Stopping assignments and helpers does nothing.** `ui/DelegationGroup.tsx:49`, `:107` and `ui/SubbotViews.tsx:40`, `:65` call `delegation.cancel` and `delegation.cancelGroup`, neither of which has a branch.
10. **Session rail data never loads.** `ui/SessionRail.tsx:36` and `ui/SessionRailExtras.tsx:60`, `:119` call `session.environment`, `session.workspace` and `session.checkpoints`, all without branches. The workspace, environment and checkpoint views and their failure text cannot be seen.
11. **Terminal panel cannot run.** `ui/TerminalDock.tsx:140` depends on `terminalOpen`, which throws (`fixtureBridge.ts:2228`). No stand-in shell output is shown.
12. **Real dictation and cloud dictation failures are not demonstrable in the main window.** The island is shown only in `dictation-preview.html` with scripted text. The cloud messages in `ui/dictation.ts` ("Codex is not signed in", "ChatGPT is rate limiting you right now", and the others) have no path to trigger them.
13. **Some session and companion states have no fixture.** `rate_limited`, `retrying`, `sleeping` and `paused` never appear in fixture data (`ui/companionStatus.ts:49-52`, `:38-39`), so those moods and rows are never seen.
14. **Most provider error codes never reach the UI.** `ui/errorCopy.ts:187-248` maps `unauthenticated`, `timeout`, `overloaded`, `quota_exceeded`, `budget_stop`, `model_unavailable`, `context_exhausted` and `permission_denied`. Only `process_exited` and the usage limit appear in scenes, so the other friendly messages cannot be checked in the preview.
15. **Message search, push notifications and the image and project-suggestion RPCs are unanswered.** `search.messages` (`ui/ExperimentalFeatures.tsx:37`), `push.key`, `push.subscribe` (`remote/PushCard.tsx:28`, `:46`), `message.image` (`ui/ComposerControls.tsx:276`) and `project.suggestions` (`ui/ProjectSuggestions.tsx:26`) have no branches, so results, images and suggestions never appear.

Also worth noting: runtime profiles (`ui/SettingsSheet.tsx:229`, `:529`, `:542`), the sign-in clear action (`ui/SignInHelp.tsx:49`), the manager troubleshoot action (`ui/App.tsx:1905`), and diagnostics export (`ui/DiagnosticsDialog.tsx:26`) are silent for the same reason.
