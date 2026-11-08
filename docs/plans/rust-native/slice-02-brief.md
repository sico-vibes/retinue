# Brief: Bitbots in Rust (GPUI), slice 2 — the look, the team and new projects

**Lane:** Gogo (OpenCode Go DeepSeek V4.1 Flash, high), in its own git worktree, assigned through the Bitbots MCP. The Director (Claude) mentors you, reviews every diff, checks the window, runs the gates and commits.

**Owner's goal (8 November 2026):** "a 1:1 rust version from what we have thus far with all the functions and UI and everything in place." Slice 1 (`apps/native`, commit `f5424db`) gave an installable core. Slice 2 makes it *look* like Bitbots, adds hiring and new projects, and makes it installable with a shortcut.

**Rules (unchanged from slice 1):**
- Never run `git commit`, `push`, `merge`, `stash` or `reset`, and never switch branches. Change nothing outside your worktree.
- Never print or log tokens, the daemon capability or credentials.
- Don't modify `crates/` or `apps/desktop/`. *Read* them as the reference. You may read data files from `apps/desktop/src` at build time (`include_str!`, `include_bytes!`, or a `build.rs`).
- If something here is impossible at the pinned GPUI rev, pick the smallest working alternative and say so. If a decision is the owner's or the Director's, stop and ask.
- Build output stays on C: (`CARGO_TARGET_DIR=C:\bbn\target`). Run long builds in the foreground and report in the same turn.

**Read first:** your slice 1 code in `apps/native`, `.claude/skills/bitbots-development/references/native-gpui.md`, `references/design-language.md`, and `docs/plans/hiring-brief.md` (the hiring RPCs).

## 1. Portraits and logos

Today every avatar is a coloured initial. Port what `apps/desktop/src/blob/haze/HazeMark.tsx` draws.

- **Character portraits:** `apps/desktop/src/blob/haze/characters/<name>/avatar.svg` (29 of them; ids and names in `characters.json`, the `char-*` → folder mapping in `characters.ts`). They are full-colour vector paths.
  - GPUI's `svg()` element draws one tint colour only, so it can't show these. Use `img()` with SVG bytes if the pinned rev rasterises SVG in colour (it uses resvg/usvg for images; check). Otherwise rasterise them in `build.rs` with `resvg` to PNG at 2× the largest size used (96 px is enough), embed the PNGs, and say which you did.
  - The animated variant (`animated.svg`) is out of scope.
- **Which art a bot gets:** port the selection logic from `HazeMark.tsx` and `runtimeRegistry.ts`: `agent.look.icon` is `logo`, `logo-tint`, `logo-mono` or a `char-*` id. A logo is the bot's coding agent's mark (`apps/desktop/src/providers/` and `runtimeRegistry.ts`). A bot whose agent has no logo shows its initial, as today. Logos may use `svg()` with a tint where the React app tints them.
- **Shape:** avatars are rounded squares or circles exactly as Haze draws them (check `haze.css` for the avatar radius at each size). Sizes: 28 px in the Projects tree, 24 px in Team rows, 32 px for the CEO row and conversation header, 64 px on Team page cards.
- **Assets** are `Arc`-shared and loaded once, not per frame.

## 2. Motion and states

Port these from `apps/desktop/src/ui/`:
- **Working shimmer** (`projectsSidebar.css`, `.ps-shimmer`): a light band sweeping across the word "Working", about 1.6 s per sweep. In GPUI, animate a gradient or an overlay with `with_animation`. With Windows "Show animations" off (`SPI_GETCLIENTAREAANIMATION`), show the plain word.
- **Marquee** (`Marquee.tsx`, `marquee.css`): a selected or hovered row whose title overflows scrolls left, pauses, and returns. It doesn't move otherwise.
- **Collapse** (`Collapse.tsx`): folding a project or an employee animates the height over 180 ms and rotates the chevron. Instant when animations are off.
- **Hover and pressed** states on every row and button, using the theme's hover and selected tokens.
- **Status words** use `StatusLabel`'s icons: a bell for *Needs you*, an alert circle for errors. Use Lucide icons as SVG (tinted with `svg()`), copied from the `lucide-react` package in `node_modules` (only the ones you use; check its licence is ISC and note it in the README).

## 3. Light theme and density

- Add the Haze **light** tokens (`themes/haze.css`, light block, and `theme.css`) next to dark in `theme.rs`. Follow the Windows app theme (light or dark) at start and when it changes. GPUI exposes `window.appearance()` and an observer; use those.
- Add the density tokens from `ui/density.css` (`--dn-*`, compact, regular and roomy) and use them for row heights and gaps.
- The theme and density are chosen in Settings (section 7) and saved in the native settings file.

## 4. The Team page and hiring

Port `ui/TeamPage.tsx` and `teamPage.css`. It opens in the main area, replacing the conversation pane, from the sidebar's Team switch header ("Open team page") and from the project "+" menu.

- **Your team** tab: each employee's portrait, nickname, role, the projects they're in (as chips), and status. Actions: Open (selects the bot), Hire into…, Release from….
- **Hire** tab: the preset catalogue from `apps/desktop/src/team/presets.json` (`include_str!`), grouped by department in `DEPARTMENTS` order, with portrait, nickname, role, a two-line description and tags. A search box filters by `presetMatches` rules. A preset already on the team shows "On your team".
- **Hire dialog:** pick projects, the coding agent (`runtime.list`; label it the way `providerBrand` does), and model. It calls `project.hire`, then refreshes.
- **Release:** `project.release`. Conversations stay.
- **Live:** handle the `project.members.changed` event, so the tree and the Team page refresh without reopening (slice 1's known gap).

## 5. New projects and staffing

- **New project** (sidebar button and the "+" menu): a dialog with a name and a folder, chosen with the Windows folder picker (`rfd` is fine, or GPUI's `prompt_for_paths` if it works on Windows at this rev). Call the same RPC the React app uses (check `App.tsx` for project creation).
- After creating, show the staffing choice from `ui/StaffProjectDialog.tsx`:
  - **Interview with the CEO**: create a CEO conversation in that project and send the kickoff text `askCeoToStaff` sends in `App.tsx`.
  - **I'll hire myself**: open the Team page's Hire tab with that project chosen.
  - **Skip**.
- The project "+" menu offers New conversation, Hire an employee, Ask the CEO.

## 6. Proposals in the conversation

Slice 1 shows "Open this in Bitbots to answer" for proposals. Port `ui/ProposalCard.tsx` for:
- `hire` proposals, grouped by `groupId` as `hireSiblings` does: each candidate with portrait, nickname, role and reason, Hire and Skip, plus **Hire all N**.
- `bot` proposals: name, role and Create or Skip.

Use the same RPCs the React card calls. Questions can still say "Open this in Bitbots to answer".

## 7. Settings, first pass

A Settings page (gear button at the bottom of the sidebar) with only:
- Appearance: theme (System, Light, Dark) and density (Compact, Regular, Roomy).
- Data: where the native database is, and "Copy my Bitbots data again", which stops the daemon, backs the old file up beside itself, copies with the backup API and restarts. Confirm first.
- About: version and the daemon's version.

## 8. Daemon resilience

- If `bitbotsd` exits unexpectedly, show a notice, restart it with backoff (1, 2, 5, 10 s, then stop and offer Retry), and reload the snapshot. A clean shutdown on window close doesn't restart.
- Unit test the backoff schedule.

## 9. Install

`cargo-packager` isn't available offline. Instead add `apps/native/scripts/install.ps1`, shipped inside the zip:
- copies the folder to `%LOCALAPPDATA%\Programs\Bitbots Native`;
- creates a Start menu shortcut "Bitbots (Rust preview)" with the app icon;
- `-Uninstall` removes both, never the data folder.

Also embed the app icon in `bitbots-native.exe` (`winres` or `embed-resource` in `build.rs`; if the apostrophe path breaks the resource compiler, the package script builds from a short-path copy as slice 1 already notes). `package.ps1` produces `Bitbots-Native-<version>.zip` with `install.ps1` and a short `README.txt`.

## 10. Tests and gates

**Unit tests:**
- art selection (logo, tint, a character, no logo → initial);
- the preset catalogue parses and every portrait id resolves to an asset;
- `hireSiblings` grouping;
- the restart backoff;
- the theme choice (System follows appearance, explicit choice wins);
- density tokens per level.

**Gates:** `cargo build --release`, `cargo test` and `cargo clippy --all-targets -- -D warnings` in `apps/native`, and `cargo check -p bloblex-daemon` unchanged.

**Live check:** against a temporary database, start empty:
1. create a project;
2. hire two presets into it, and one of them into a second project;
3. confirm through `project.members` that there's one agent with two memberships;
4. kill `bitbotsd` and confirm it restarts and the snapshot reloads;
5. close with no leftover process.

Run `install.ps1` and then `install.ps1 -Uninstall` once, and report the paths.

## Report

- What you built, per section, with `path:line`.
- Every file you changed.
- The last lines of each gate.
- Where the zip is.
- Every decision this brief didn't make, and what is unverified. You can't see the window: the Director will check it, so say which screens you're least sure of.
- Proposed additions to `references/native-gpui.md` (images and SVG, animation, appearance, dialogs and file pickers), as text.
