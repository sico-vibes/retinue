# Brief: Bitbots in Rust (GPUI), slice 6 — the full Settings sheet, the CEO, onboarding, analytics and updates

**Lane:** Gogo (OpenCode Go DeepSeek V4.1 Flash, high), in its own git worktree. **Two other lanes run at the same time:**
- slice 4 (tasks, changes, terminal), in another Gogo conversation;
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

  Verify with unit tests and code reading.
- `CARGO_TARGET_DIR=C:\bbn\target6`, your own.
- If something is impossible at the pinned GPUI rev, take the smallest working alternative and say so.

**Read first:** `.claude/skills/bitbots-development/references/native-gpui.md`, especially "Traps that kill or blank the window", and the native entries at the end of `references/lessons.md`. Also read `AGENTS.md`: Remote, machines, `usage.readCliSignIns`, dictation and secrets carry owner rules that apply here exactly as in the React app.

## File ownership

**You own:**
- `src/ui/settings_page.rs`, which you turn into a `src/ui/settings/` module with one file per page;
- new files `src/ui/onboarding.rs`, `src/ui/ceo.rs`, `src/ui/analytics.rs`, `src/ui/updates.rs`, `src/ui/project_settings.rs`;
- `src/state/settings.rs`.

**Shared files, minimal hooks only.** List every line in your report.
- `src/ui/root.rs`: new `Page` variants, the render dispatch and event arms.
- `src/ui/mod.rs` and `src/state/mod.rs`: `mod` lines.
- `src/ui/search.rs`: switcher actions.
- `src/settings_file.rs`: new keys.
- `Cargo.toml`.

**Don't edit:**
- `theme.rs`, `art.rs`, `sidebar.rs`, `conversations.rs`, `conversation.rs`, `message.rs`, `markdown.rs`, `composer.rs` or `team.rs` (the Director is restyling them);
- slice 4's new folders.

If a page needs a sidebar or header entry, add a public method on `Root` and say where the button goes.

## 1. The Settings sheet

Port `ui/SettingsSheet.tsx` and its pages: the same sections, order, copy, controls and RPCs (`settings.get/set` and each page's own).

**Pages:**
- **General and appearance** (`AppearancePage.tsx`, `DensityControl.tsx`, `AccentPicker.tsx`). Haze and Stone in the list: wire Stone as a stored choice even if its tokens arrive later, and note it.
- **Accounts** (`AccountsPage.tsx`), **Connections** (`ConnectionsPage.tsx`).
- **Memory** (`MemoryPage.tsx`, `BotMemoryGroup.tsx`), **Skills** (`SkillsPage.tsx`, `BotSkillsGroup.tsx`).
- **Secrets** (`SecretsPage.tsx`).
- **Approval rules** (`ApprovalRulesPage.tsx`).
- **Backup** (`BackupPage.tsx`).
- **Remote** (`RemoteSettings.tsx`, `NativeRemote.tsx`), **Machines** (`MachinesPage.tsx`, `MachineBots.tsx`).
- **Health and diagnostics** (`HealthPage.tsx`, `DiagnosticsDialog.tsx`, `ReportProblemDialog.tsx`).
- **Keybindings** (`KeybindingEditor.tsx`), bound to the native window's actions from slice 3 (`search.rs` `SHORTCUTS`).
- **Experimental** (`ExperimentalFeatures.tsx`).
- **Speech** (`SpeechModelDialog.tsx`): download and remove the on-device model, through the same native path as `apps/desktop/src-tauri/src/speech.rs`, so slice 5's dictation can use it. Writes slice 5's `DictationSettings`.
- **Companion:** slice 5's `CompanionSettings`, including sounds, volume, the hotkey on or off, and size.

**Rules that carry over exactly:**
- Secret values are never displayed, logged or returned to the window. Show names and "set" or "not set" only.
- The usage-bar sign-in read (`usage.readCliSignIns`) is off by default and needs the owner's explicit choice, with React's wording.
- Cloud dictation stays out (slice 5 left a `TODO(owner)`). Don't read any credential file.
- Remote and Native Remote settings are desktop-only, as in React. Show the certificate fingerprint and the pairing and approval flow exactly as React does.

## 2. The CEO page and project settings

Port `ui/ManagerPage.tsx` (the CEO's settings and playbook, with `PlaybookDiff.tsx`), `ui/ProjectSettingsDialog.tsx`, `ProjectSources.tsx` and `ProjectIcon.tsx`.

## 3. Onboarding

Port `ui/FirstRunOnboarding.tsx` ("Meet your CEO" and the steps) and `LaunchIntro.tsx`, if it's simple. Show onboarding on a fresh database only, after the native first-run copy question.

## 4. Analytics and updates

- **Analytics:** port `ui/AnalyticsView.tsx` and `RichChart.tsx`. Bar and line charts are drawn with GPUI paths, using the theme's tokens.
- **Updates:** port `ui/UpdatesPanel.tsx`, `UpdateBanner.tsx` and `WhatsNew.tsx` as **read-only**. Show the version and the release notes. **No self-update in the native preview:** there's no download and nothing runs. Say "Install the new preview zip to update".

## 5. Tests and gates

**Unit tests:**
- settings value round trips;
- secret rows never carry values;
- keybinding conflict detection;
- the chart scale maths;
- onboarding shows only on a fresh database;
- the speech model state machine.

**Gates** in `apps/native`, run once each at the end: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`. Package with `scripts/package.ps1 -NativeTarget C:\bbn\target6 -DaemonTarget C:\bbn\daemon-target6`.

## Report

- What you built, per page, with `path:line`.
- Every shared-file line you touched.
- Where entry buttons should go.
- The gate tails and the zip path.
- What's left and what's unverified.

Apply proposed `native-gpui.md` additions to the file (forms, charts, long settings pages).
