# Brief: Bitbots in Rust (GPUI), slice 5 — the companion and dictation

**Lane:** Gogo (OpenCode Go DeepSeek V4.1 Flash, high), in its own git worktree. **Slice 3 runs at the same time in another Gogo conversation**, so file ownership below matters. The Director reviews, screenshots the window, merges both and commits.

**Base:** `apps/native` at `4b677f3` (slices 1 and 2).

**Rules (unchanged):**
- Never run `git commit`, `push`, `merge`, `stash` or `reset`, and never switch branches. Change nothing outside your worktree.
- Never print or log tokens, the capability or credentials.
- Don't modify `crates/` or `apps/desktop/`. Read them as the reference. You may depend on `crates/bloblex-speech` by path.
- If something is impossible at the pinned GPUI rev, pick the smallest working alternative and say so.
- `CARGO_TARGET_DIR=C:\bbn\target5`. Use a separate target dir, because slice 3 is building in `C:\bbn\target` at the same time.

## File ownership (so the two slices merge cleanly)

Slice 3 is rewriting `src/ui/root.rs`, `sidebar.rs`, `conversation.rs`, `conversations.rs`, `team.rs`, `dialogs.rs`, `settings_page.rs` and adding popovers, select, toast and the composer.

**You own new files only:**
- `src/companion/` (window, view, layout, state machine, positions, attention);
- `src/dictation/` (engine glue, the bubble window, the shortcut);
- `src/hotkeys.rs`;
- `src/sound.rs`.

**Touch shared files minimally**, and list every such line in your report:
- `main.rs`: open the companion and register hotkeys;
- `ui/mod.rs`: `pub mod` lines;
- `state/mod.rs`: only if you need a read-only accessor;
- `Cargo.toml`.

Don't add settings UI. Expose a `CompanionSettings` / `DictationSettings` struct with the defaults, stored in the native settings file under new keys. Slice 6 builds the UI.

**Shared state:** the main window and the companion read the same daemon snapshot and events. Lift what both need into a GPUI global or a shared `Entity` created in `main.rs`, rather than a second daemon connection. If that needs a change inside `root.rs`, keep it to the smallest hook (one constructor parameter) and say so.

## 1. The companion window

Port the companion from `apps/desktop/src-tauri/src/lib.rs` (`make_companion` ~2960, positions ~2297–2960, resize ~2037–2140, `set_companion_mode`, the Ctrl+Alt+B hotkey ~2556) and the UI from `apps/desktop/src/ui/Companion*.tsx`, `companion*.ts`, `companion.css`, `companionAttention.css`, and `src/blob/companionFsm.ts`, `companionLayout.ts` and `characterState.ts`. Haze look only.

**The window:**
- A second GPUI window: frameless, always on top, transparent, not in the taskbar, with Haze's backdrop.
- It starts at the bottom centre of the primary monitor's work area.
- It can be dragged freely. Its position and size are remembered per monitor, as `companionPositions.ts` does, and clamped to the work areas.
- It resizes from its edges and has the size presets, with the same limits as the shell.

**Modes:** compact and expanded, as `companionFsm.ts` and `set_companion_mode` define. Ctrl+Alt+B toggles it, through a Windows global hotkey (`RegisterHotKey` on a message-only window, or a crate that does it).

**Contents (Haze):**
- The team row (`CompanionTeam.tsx`), with portraits from `art.rs` and status dots.
- The attention state (`CompanionAttention.tsx`): who needs you, with Allow and Deny for a waiting permission through the same RPC as the main window.
- The conversation picker (`CompanionConversationPicker.tsx`).
- A small composer that sends to the picked conversation. A plain multi-line input from `text_input.rs` is enough; don't wait for slice 3's composer.
- The status gradient from `haze.css`, where a solid colour can stand in if GPUI can't draw it.
- "Open in Bitbots" brings the main window forward on that conversation.

**File drop:** dropping a file on the companion attaches it to the picked conversation, as `companionDrop.ts` does, or opens a new one.

**Peek:** port `companionPeek.ts`, showing the latest reply briefly when a turn finishes.

**Welcome:** the first-show greeting animation from `greetingScene.ts` / `characterMotion`, simplified to the portrait plus a short wave motion if the full scene isn't feasible. Say which.

## 2. Sounds

Port `src/blob/soundCues.ts`, `soundVolume.ts` and `ui/dictationSound.ts`:
- the same cues, off by default as in the React app, with the volume in `CompanionSettings`;
- play through `rodio` or the `cpal` that `bloblex-speech` already pulls;
- if the React app synthesises the sounds in Web Audio, synthesise the same envelopes in Rust; don't add audio files unless the React app ships them.

## 3. Dictation (on-device)

Port the native half from `apps/desktop/src-tauri/src/speech.rs` and the commands `dictation_*`, `speech_*`, `set_dictation_shortcut` and `make_dictation` (lib.rs ~3041–3100). Port the UI from `ui/DictationIsland.tsx`, `DictationBubble.tsx`, `DictationButton.tsx`, `dictation.ts` and `dictationCopy.test.ts`.

- **The engine:** `bloblex-speech` with its `native` feature (sherpa-onnx + cpal), by path.
  - If depending on it from the separate workspace fails (workspace-inherited fields), say exactly why. Then try `[patch]` or a thin re-export crate under `apps/native/`. Don't edit `crates/`.
  - If sherpa-onnx's native libraries need bundling, add them to `package.ps1` the way `tauri.conf.json` bundles them.
- **Model:** use the same model directory and catalogue as the installed app, so a model it already downloaded is reused (read `speech.rs` for the path). Downloading a model can wait for slice 6's settings; if no model is installed, the bubble says the React app's exact message.
- **The shortcut:** Ctrl+E, global, push to talk or toggle as the React app does.
  - The bubble window shows live words while you speak.
  - On stop, the text goes into the focused composer: the main window's or the companion's.
  - If neither is focused, it goes to the clipboard, matching the React behaviour. Check `dictation.ts`.
- **Cloud dictation is out of scope** for this slice. It relies on an owner-approved exception that reads the Codex sign-in; it needs the owner's go-ahead for the native app first. Leave a clear `TODO(owner)` and don't read any credential file.

## 4. Tests and gates

**Unit tests:**
- the companion state machine transitions;
- position clamping across two work areas, plus a monitor removed;
- the size presets and limits;
- peek timing;
- the drop decision;
- sound envelope parameters;
- the dictation copy messages;
- hotkey registration failure → the React app's message ("Ctrl+Alt+B is already in use.").

**Gates:** `cargo build --release`, `cargo test` and `cargo clippy --all-targets -- -D warnings` in `apps/native` (with your target dir), and `cargo check -p bloblex-daemon`.

**Live check** against a temporary database. Report through logs:
1. the companion opens;
2. Ctrl+Alt+B hides and shows it;
3. it remembers its position across restarts;
4. a turn finishing triggers peek;
5. dictation starts and either transcribes (if a model is installed) or shows the no-model message;
6. a clean close leaves no process behind.

Rebuild the zip with `package.ps1`. Point its target dir at yours.

## Report

- What you built, per section, with `path:line`.
- Every file changed, with the exact lines you touched in shared files.
- The gate tails.
- Where the zip is.
- Decisions this brief didn't make.
- What is unverified (you can't see the windows).
- Proposed `native-gpui.md` additions (second windows, always-on-top and transparency, global hotkeys, audio, shared state across windows), applied to the file.
