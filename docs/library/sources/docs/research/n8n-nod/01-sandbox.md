# nod's sandbox on the user's computer, against Retinue's worktrees, approvals and linked computers

> **What this is:** a research report for the owner, one of five written at the same time about `n8n-io/nod`. This one covers the sandbox only: what runs in it, what isolates it, how it is installed and started on Windows, macOS and Linux, what it can reach, how consent works, and how it is cleaned up.
> **Status:** research only. No Retinue code was changed, no commit was made.
> **Source:** `https://github.com/n8n-io/nod`, cloned shallowly at commit `47cfee3fe77279e1d543a367131d3ff2c1202e18` (16 November 2026, `main`). The clone is at `C:\Users\Sico Vibes\Desktop\DevCN\_research\nod-sandbox` and was read, never run.
> **Owner:** Vex. **Author:** Grey (research lane). **Date:** 16 November 2026.

## Summary

1. nod runs the agent's shell, its files and its browser inside **Docker containers on a private Linux VM** (Lima with Apple Virtualization on macOS, QEMU/KVM on Linux, a WSL 2 distribution on Windows), never on the user's own OS.
2. The isolation is layered: VM, then three containers (`workspace`, `browser`, `postgres`), then an unprivileged Linux user per container, then a **separate Linux user for each connected CLI's token**, plus host-level `iptables`, a custom AppArmor profile, `cap-drop ALL`, `no-new-privileges` and per-container memory/CPU/process limits.
3. Retinue has **no sandbox of this kind at all**: bots are the owner's own coding-agent CLIs, running as the owner's Windows user in the project folder, and the only boundaries are each CLI's own permission flags, Retinue's approval cards, and git worktrees for file separation.
4. nod is far stronger at **containment** (a bad command cannot reach the owner's files, home folder or network); Retinue is stronger at **review, undo and per-action consent** (worktrees, diffs, merge/discard cards, `file.revert`, "Always allow" rules, and two computer features nod has no equivalent of).
5. Most of nod's value here is **design, not code**: the uncertainty protocol around killed commands, sealing a CLI token to its own Linux user, re-checking the environment at every start, and one honest sentence in the UI about what the sandbox can reach. Code may be copied under Apache-2.0, but this brief forbids copying, and I recommend adopting the designs only.

## Licence

- Root `LICENSE` is the **Apache License 2.0** ([LICENSE](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/LICENSE#L1-L4)), and the landing page advertises "Free · Apache 2.0". This repository does **not** use n8n's Sustainable Use License; I checked the root licence and searched every `LICENSE`/`COPYING`/`NOTICE` file in the tree.
- `NOTICE` carries the attribution, and it flags one file under **GPL-3.0-or-later**: `src/providers/local-speech/phonemizer-worker.ts`. It is not in my area, but a reader should not assume the whole tree is permissive.
- Two files under `public/logos/` carry their own third-party notices (`LICENSE-LOBEHUB.txt`, `LICENSE-OPENCODE.txt`).
- I did **not** verify the licences of tools fetched at build time: `rtk` (`src/providers/docker/install-rtk.sh:1-23`, its own `LICENSE` is copied into the image), `agent-browser` (Vercel Labs), Chromium, and the Debian packages named in the two Dockerfiles. Treat those as unknown, not as Apache-2.0.
- **Verdict per idea below:** Apache-2.0 allows copying with attribution and a change notice, but the brief says do not copy, so every idea is marked **design only**. No file from the clone was copied into Retinue.

## How nod does it

### The shape, in one line

The Nod server (Electron app process, or a self-hosted Node server) holds every key and decides everything; the model's shell and the browser live in containers on a private VM that can reach the public internet and nothing else.
Their own summary is [docs/src/content/docs/how-it-works/sandbox-and-browser.md:8-11](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/sandbox-and-browser.md#L8-L11).

### Isolation stack

| Layer | What it is | Evidence |
| --- | --- | --- |
| Hypervisor | Lima VM: Apple `vz` on macOS, `qemu` on Linux, 24 GiB disk, half the host's RAM (3–6 GiB), 2–4 CPUs | `desktop/runtime/guest-config.mjs:48-74`, `desktop/runtime/index.mjs:398-407` |
| Windows | A private **WSL 2** distribution named `nod-<hash>`, imported from a tarball, its own Docker inside | `desktop/runtime/windows.mjs:24-25`, `:295-360` |
| Container runtime | Docker **inside the VM only**; the app talks to it through a forwarded `docker.sock` or a bundled `docker.exe` | `guest-config.mjs:71-75`, `index.mjs:340-352`, `windows.mjs:417-429` |
| Containers | `nod-workspace-<id>`, `nod-browser-<id>`, `nod-postgres-<id>`, on three separate bridge networks with inter-container traffic off | `index.mjs:604-634`, `:731-751` |
| Container hardening | `--cap-drop ALL`, `no-new-privileges:true`, `--init`, `--pids-limit`, `--memory`, `--cpus`; the browser also `--read-only` with `tmpfs` | `index.mjs:947-999` |
| Users | Commands run as unprivileged `node`; each connected CLI gets its own Linux user `20 000 + hash(id)` in group `nod-connected` | `Dockerfile.workspace:16-18`, `src/providers/docker/command.ts:69-70`, `:320` |
| Kernel | A repaired `docker-default` AppArmor profile, verified present at every start | `guest-config.mjs:1-46` |
| Network | `iptables` in the VM: the workspace and browser bridges may not reach private or reserved IPv4 ranges or the host, and `INPUT` from those bridges is dropped | `guest-config.mjs:131-145` |
| Server side | The server is **not** sandboxed. It runs as the user, holds provider keys, the Vault and the access key | same doc, trust-boundary table at `:33-42` |

Nothing here is a Windows Sandbox, a job object, a restricted token, WASM, or a separate Windows user. On Windows the whole isolation is "one WSL distribution that is not the default one".

### What runs inside

- **The agent's shell.** The Pi agent's native tools (`read`, `bash`, `edit`, `write`, `find`, `ls`) are re-implemented to call `WorkspaceSession.exec`, which is `docker exec` into the workspace ([`src/providers/pi/sandbox.ts:210`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/pi/sandbox.ts#L210), [`src/providers/docker/index.ts:281-327`](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/src/providers/docker/index.ts#L281-L327)). Each thread works in `/workspace/<thread id>`; `/home/node` is a separate volume so installed tools persist ([sandbox-and-browser.md:44-52](https://github.com/n8n-io/nod/blob/47cfee3fe77279e1d543a367131d3ff2c1202e18/docs/src/content/docs/how-it-works/sandbox-and-browser.md#L44-L52)).
- **A toolchain.** Node, Python with PDF/Office libraries, git, curl, ripgrep, jq, ffmpeg, yt-dlp, poppler, qpdf, Ghostscript, Tesseract, LibreOffice (`Dockerfile.workspace:7-13`, and the instructions string the model sees at `src/providers/docker/index.ts:105-106`).
- **Connected CLIs.** A CLI the owner linked (say a mail tool) is installed at first run into a sealed folder, run as its own Linux user, with an environment built from scratch: fixed `PATH`, private `HOME`, `XDG_CONFIG_HOME`, and only that connection's credential variables (`command.ts:72-106`, `:183-191`; docs at `sandbox-and-browser.md:82-86`). The supervisor refuses an install folder another user can write, and refuses to run if another CLI's install is owned by the same user.
- **The browser, in its own container.** A Node worker inside `nod-browser` owns Chromium on a private virtual display, driven by Vercel's `agent-browser`, with `sandbox: false` because the container is the boundary (`src/providers/container-browser/config.ts:1-24`, `Dockerfile.browser:11-33`). Four live sessions at most, one profile per session under `/data`, viewport 1440×900, headed on Xvfb.
- **Not the owner's files.** The owner's own folders are a separate, later capability (`local-files`), which runs in the **server** process, not the sandbox, behind its own switch, path rules and keep-out list (`src/server/local-files.ts:210-240`, `:127`; `docs/trust/security-model.md`, "Isolation").

### Data flow of one command

1. Pi calls `bash`; the runtime sends a JSON request line, not a shell string in argv, to `docker exec -i --user node … /usr/bin/python3 -I -c <supervisor>` (`command.ts:313-331`).
2. The supervisor sets `prctl(PR_SET_CHILD_SUBREAPER)` so orphaned grandchildren re-parent to it (`command.ts:74`), spawns `bash -c` with `start_new_session=True` (`:192`), and streams `stdout`/`stderr` back as base64 JSON frames with a byte limit (`:128-129`, `:219-222`).
3. Cancel, timeout or output overflow sends a signal: `SIGTERM` to the process group, then to every member and descendant found in `/proc`, then `SIGKILL` after 2 s (`:162-174`, `:204-207`). Default `bash` timeout is 59 minutes.
4. The supervisor's final frame says *exit code and why it stopped* (`:240`). The server treats a missing final frame as **uncertain**, never as success, then runs a `/proc` sweep from `/var/tmp` that kills every process the workspace user still owns (`:243-278`); only when that sweep proves nothing is left does the workspace reopen, otherwise it is fenced until Nod restarts (`index.ts:152-192`, and `sandbox-and-browser.md:65-72`).
5. Output is capped at 1 MiB for Pi; the command view keeps the exit code and the redacted last 16 000 characters (`sandbox-and-browser.md:74-79`).

### What it can reach

- Public internet: yes. Anything private: no — private and reserved IPv4, loopback, link-local (cloud metadata), and all external IPv6 are rejected in the VM's `iptables` (`guest-config.mjs:131-145`). Source installs do the same with a **firewall sidecar container** that shares the workspace's network namespace (`compose.yaml:48-88`, `src/providers/docker/workspace-network.sh:1-19`).
- The host: no. The workspace never mounts the data root or the owner's home, and the loopback of a container can reach host listeners, so nod explicitly refuses to treat loopback as identity (`AGENTS.md:47`).
- Provider keys, the owner access key, Vault values and browser profiles: no (`sandbox-and-browser.md:35-42`).
- The owner's apps: only through connected apps and the browser, both server-side and both behind the trust check.

### Consent

This is the part worth reading twice. **Inside the sandbox, nothing asks.** `bash`, file writes and page reads run freely; the model is told to keep work in the workspace (`index.ts:105-106`). The trust check (`src/server/trust.ts:24`, one checker) decides only actions that leave the sandbox: a connected-app action, a browser step that commits (click, key press, option, tick, drag, upload, dialog, side-effecting script), a sign-in or saved session, a payment, a change to Nod's own schedules/rules/memory, and local-file actions (`AGENTS.md:36-43`). The containment is what makes that acceptable: the worst a wrong `bash` can touch is a container the owner can see and a disk they can throw away.

### Install and start

- **macOS 14+:** the DMG ships the app only; local setup downloads a runtime pack (`guest.img`, `docker-guest.tar.gz`, three images) with per-file SHA-256 checks, starts Lima with Apple's Virtualization framework, boots the VM, applies container protection, loads images by ID and compares (`docs/get-started/install-desktop.mdx:34`, `docs/deploy/desktop-app.mdx:81-84`, `desktop/runtime/index.mjs:495-519`).
- **Windows 11 x64:** needs WSL 2, hardware virtualization and ~20 GB free (`install-desktop.mdx:72-74`). If WSL is missing, nod offers `wsl --install --no-distribution` behind a **UAC prompt** (`windows.mjs:73-85`). It then imports the guest tarball as a distro with `wsl --import … --version 2`, starts it with a Python `start` step, and keeps it alive with a `sleep infinity` holder process owned by the app (`windows.mjs:295-416`). Docker runs inside the distro; the app drives it with a bundled `docker.exe` (`:417-429`). The profile folder gets a restrictive ACL first (`protect-profile.ps1:1-9`).
- **Linux:** Flatpak preview, needs `/dev/kvm` and refuses to start without it rather than emulate (`index.mjs:218-226`, `install-desktop.mdx:108`); same Lima VM with QEMU.
- **Connected to a server instead:** no local runtime at all; the “private computer” is the server's.

### Cleanup

- Quitting stops containers with `docker stop --time 20`, then stops Lima and waits until the VM reports `Stopped`, or terminates the WSL distro; failure to confirm becomes an error, not a silent quit (`index.mjs:1077-1177`, `windows.mjs:462-502`).
- **Remove all data** deletes the profile, `wsl --unregister`es the distro, or `limactl delete --force` plus `rm` of `~/.nod-desktop/<id>` (`index.mjs:1082-1110`, `windows.mjs:504-510`, `install-desktop.mdx:190-198`).
- Images from older runtimes are removed and the disk is trimmed (`index.mjs:581-602`, `:1019`); on macOS, runtime packs nothing mounts are deleted after a day.
- Anything left behind by a failed first setup is detected and replaced (`index.mjs:360-394`).
- An orphaned VM (folders without config, or config without folders) is refused or repaired rather than reused (`index.mjs:296-311`, `:412-451`).

### Main files

`desktop/runtime/index.mjs` (prepare, networks, containers, stop, remove), `desktop/runtime/windows.mjs`, `desktop/runtime/guest-config.mjs`, `desktop/runtime/protect-profile.ps1`, `Dockerfile.workspace`, `Dockerfile.browser`, `compose.yaml`, `src/providers/docker/index.ts`, `src/providers/docker/command.ts`, `src/providers/docker/workspace-network.sh`, `src/providers/container-browser/*`, `src/providers/pi/sandbox.ts`, `src/server/trust.ts`, `src/server/local-files.ts`, `docs/src/content/docs/how-it-works/sandbox-and-browser.md`.

## How Retinue does it now

Retinue has three separate mechanisms, and none of them is an OS sandbox.

**1. The bot's own machine access: none contained.** A bot is a coding-agent CLI (`claude`, `codex`, `opencode`) launched by the daemon as a child process with `current_dir` set to the project folder (`crates/retinue-adapter-claude/src/lib.rs:833`, `crates/retinue-adapter-codex/src/lib.rs:757-758`). It runs as the owner's Windows user, with the owner's environment (`env_clear` exists in exactly one place in the whole codebase: `crates/retinue-daemon/src/owner_computer.rs:515`), the owner's network, and the owner's disk. What bounds it:

- **The CLI's own flags.** Claude gets `--permission-mode default`, or `bypassPermissions` for Full access (`crates/retinue-adapter-claude/src/lib.rs:248`). Codex gets `approvalPolicy: on-request` and `sandbox: workspace-write`, or `never`/`danger-full-access` for Full access (`crates/retinue-adapter-codex/src/lib.rs:757`). That is Codex's own sandbox, not ours, and it is per-command, not per-machine.
- **Retinue's approval card.** The Claude adapter intercepts `can_use_tool` control requests and turns each into a card (`crates/retinue-adapter-claude/src/lib.rs:434-455`). Four modes: `Ask`, `Auto`, `AcceptEdits`, `Bypass` (`crates/retinue-agent-core/src/lib.rs:10`).
- **A classifier, not a jail.** `classify_permission` (`crates/retinue-agent-core/src/lib.rs:217-342`) sorts a tool call into `READ`/`EDIT` (with a path containment check against the project and its `.git` root), `SAFE_COMMANDS` (a tiny allowlist: `git`, `ls`, `pwd`, `echo`, `rg`/`grep`, `cat`, `find`, `node --version`, and **only** when the command has no shell operator), `COMPUTER_INPUT` and `BROWSER` (always ask, never rememberable). Anything else is denied in `Auto`/`AcceptEdits` and shows a card in `Ask`.
- **Remembered rules.** "Always allow" writes a `permission_rules` row; only `rememberable` cards offer it.

**2. Git worktrees: file isolation between parallel tasks, not security.** A second task on the same repository runs in `<repo>-retinue/<slug>` on `retinue/<slug>`, taken from `HEAD`, created with a plain `git worktree add` argument list, never a shell (`crates/retinue-daemon/src/worktree.rs:1-9`, `:126-147`). Merging back is refused while the main checkout is dirty (`:226-263`); a merge, a Discard, or `worktree.removeAbandoned` removes the copy, and nothing runs on a timer (`crates/retinue-daemon/src/worktree_cleanup.rs:1-8`). The copy can be restored if its files vanish (`worktree.rs:298-307`). `session.workspace` tells the owner plainly whether a conversation is `shared` or `isolated` (`crates/retinue-daemon/src/workspace.rs:1-5`).

**3. The two computers.** The **linked cloud computer** is one persistent Harbor Linux desktop container the owner links; `computer_*` tools and the `vps-browser` tools reach it over HTTP with a bearer token held in Windows Credential Manager (`crates/retinue-daemon/src/computer.rs:1-19`). The **owner's own computer** is off until `experimental.ownerComputer` is on, the conversation is attended, the app is not on the deny-list, and the owner answers a consent card; grants last 5 minutes, 60 s idle, 15 minutes per turn, and every method is `never` in `tiers.json` (`crates/retinue-daemon/src/owner_computer.rs:1-47`, `apps/desktop/src/remote/tiers.json:355-421`).

| Question | nod | Retinue |
| --- | --- | --- |
| What isolates the agent's shell | VM + container + user + iptables + AppArmor | The CLI's own flags and our approval card; no OS boundary |
| Where it runs | Linux container, `/workspace/<thread>` | Windows, the project folder |
| Keys in that place | No provider keys, no access key, sealed CLI tokens | The CLI's own auth files are readable by the CLI; the daemon's env is inherited |
| Network | Public only; private/loopback/metadata blocked | Whatever the owner's machine can reach |
| Per-task file isolation | None (one workspace per thread; task folders "do not create separate computers or file access boundaries", `docs/use/workspace.md:120`) | Yes: worktrees, diffs, merge/discard, `file.revert` |
| Asking | Only outside the sandbox | Every tool call in `Ask`; category rules in `Auto`/`AcceptEdits` |
| Browser | Its own container, its own profile per session | Only on the linked computer, per-conversation profile (`crates/retinue-daemon/src/browser.rs:1-40`) |
| Setup cost | ~1.8–2.1 GB, virtualization, WSL on Windows | None beyond the CLIs the owner already has |
| Cleanup | Stop the VM at quit, unregister the distro on "remove all data" | Worktrees removed on merge/discard, abandoned ones by explicit call |

## What they do better

1. **Containment of a wrong or hostile command.** In nod, `rm -rf ~` reaches a container's home, not the owner's. In Retinue it reaches the owner's, and only the approval card stood in the way — and `Bypass` removes that card. This is the single biggest gap the report found.
2. **The model is untrusted even after approval.** nod's design assumes a fooled model can run any command it likes *inside* the sandbox, and puts the secrets outside it. Retinue's design assumes the approval card is the only line, then lets the approved command inherit the owner's environment.
3. **Honest outcomes after a kill.** A lost acknowledgement becomes "uncertain, do not repeat", and the workspace stays fenced until a process sweep proves it is clean (`command.ts:227-236`, `index.ts:152-192`). Retinue relies on the CLI's own cancellation.
4. **Sealing a credential to one consumer.** Each connected CLI's token is bound to a per-CLI Linux user and a sealed install folder, checked again before every run (`command.ts:69-70`, `:187-191`). This is a real pattern for any future "run this CLI with my token" feature.
5. **Environment constructed, not inherited.** `serviceEnvironment()` and a from-scratch `PATH`/`HOME` (`command.ts:104-106`, `:330`); nothing a program plants in the sandbox is inherited by the next step.
6. **The environment is re-verified at every start.** Network settings must still be a non-internal bridge with inter-container traffic off, or it refuses (`index.mjs:604-634`); an unexpected mount means "storage needs recovery" (`:664-674`); the guest must still have the AppArmor profile (`guest-config.mjs:45`); a container that is no longer the one we made is replaced (`:659-683`).
7. **File transfer without a symlink or time-of-check race.** Reads open with `O_NOFOLLOW`, walk directories by descriptor, and return a `dev:ino:size:mtime:ctime` identity that the next chunk must match (`index.ts:23-54`); writes stage to a dot-file, `fsync`, then `os.replace` (`:56-96`).
8. **Files sent into the sandbox, not into the project.** Attachments are uploaded into the workspace with a bounded chunked helper; Retinue copies sent images into `<data>/message-images/` and leaves the project untouched, which is fine, but nod's upload path is the better template for any future "give the bot this file" flow.
9. **Output discipline.** Head-and-tail of up to 1 MiB, full log named in the workspace, and a separate redacted command record for the owner (`sandbox-and-browser.md:74-79`).

## What we do better

1. **Review, diff and undo.** Worktrees plus the merge card, `file.revert`, turn checkpoints (`crates/retinue-daemon/src/checkpoints.rs:1-9`) and a read-only repository view. nod has none of this: one shared workspace per thread, no branches, no per-task diff, and its own docs say task folders are not access boundaries.
2. **Granular consent with memory.** Every tool call can ask, categories can be excluded, `Always allow` is scoped per project, and `COMPUTER_INPUT`/`BROWSER` can never be remembered (`crates/retinue-agent-core/src/lib.rs:232-241`, `:349`). nod deliberately asks only outside its sandbox.
3. **The owner's own computer is treated as the one capability that must keep asking.** Deny-list on app names and window titles, attended-only, no grant longer than 5 minutes, no screenshots at all in this cut, a content-free log line per call, `never` in `tiers.json` (`owner_computer.rs:21-47`). nod has nothing comparable; its nearest thing is the local-files keep-out list.
4. **Remote and machine links inherit nothing.** `computer.*`, `browser.*` and `owner_computer.*` are all `never`, and `owner_computer.grant`/`computer.changed` are `drop` (`tiers.json:355-421`, `:1776-1778`). nod's sandbox is reachable by design from a self-hosted server and any client of it.
5. **No setup tax, and no virtualization dependency.** Retinue works on the machine the owner already uses, with the CLIs they already paid for and signed into. nod needs 2 GB, WSL 2 or `/dev/kvm`, and 20 GB.
6. **The owner's sign-ins stay the owner's.** Retinue drives the owner's own installed CLIs, so no credential is ever copied; nod must keep keys server-side and give the sandbox none.
7. **A stated isolation status per conversation.** `session.workspace` reports `shared`/`isolated` and says when isolation was wanted but did not happen (`workspace.rs:1-5`); nod's workspace is always the same and the UI never claims otherwise, but it also never says "this is not per-task".

## Ideas we could adopt

Each idea says what it is, the effort (small / medium / large), the risk, the `AGENTS.md` rules it would touch, and the licence verdict.

### 1. Say plainly what a bot can reach today (small, low risk)

A short paragraph in Settings › Computer and in the onboarding text, plus one row in the threat model: a bot runs as you, in this folder, with your network and your installed tools; its limits are the approval card and the CLI's own sandbox. nod's equivalent honesty is the trust-boundary table (`sandbox-and-browser.md:33-42`).
**Rules touched:** the 14 November amendment requires threat-model rows for new reach, and this is documentation of existing reach; design language (plain sentence-case copy).
**Licence:** design only (we invent our own words).

### 2. Construct the CLI environment instead of inheriting it (small, medium risk)

Pass each CLI an allowlist: `SystemRoot`, `SystemDrive`, `PATH`, `USERPROFILE`, `LOCALAPPDATA`, `APPDATA`, `TEMP`/`TMP`, `ComSpec`, `PATHEXT`, locale, plus explicitly what that CLI needs. Today the only `env_clear` in the tree is the owner-computer driver (`owner_computer.rs:515`), so a bot's child inherits `AWS_PROFILE`, `GITHUB_TOKEN`, proxy variables and anything else the app was launched with. nod's model is a strict regex on allowed names (`command.ts:90`) and a from-scratch base (`:104-106`).
**Risk:** breaks setups that rely on proxy or certificate variables, and some CLIs read auth from the environment; needs a per-adapter list, a `doctor.run` check, and an honest error when a variable is missing. **Medium, not small, because of that testing surface.**
**Rules touched:** "no copied provider authentication" (strengthens it — environment tokens stop leaking to the agent), prompts through stdin (unchanged), the 14 November amendment (not affected).
**Licence:** design only.

### 3. Adopt the uncertainty protocol for the tools that may have acted (medium, low-medium risk)

`computer_exec`, the six `browser_*` screen tools, `git` actions and `computer.routine.run` can lose their answer. nod's rule is: no final acknowledgement means "uncertain, do not repeat, fence until proven clean" (`command.ts:484-491`, `index.ts:152-192`). Retinue already has this instinct in the queued-send path (`client_message_id`, ambiguous-send gate in `queue.rs`), but not for computer tools.
**Rules touched:** golden rule 2 ("the UI never fabricates success"); the 14 November amendment ("what is recorded is content-free" — the new state message must stay content-free).
**Licence:** design only.

### 4. Extend the existing ACL hardening to the data folder (small, low risk)

Retinue already runs `icacls /inheritance:r /grant:r <user>:F` on `%LOCALAPPDATA%\Retinue\private-tmp` (`crates/retinue-adapter-claude/src/lib.rs:49-67`) and on Native TLS files (`crates/retinue-daemon/src/remote/native.rs:386`). nod does the same to its whole profile on Windows (`protect-profile.ps1:1-9`). Doing it once for `<data>` — which holds the capability file, endpoint files, message images, task images and the SQLite database — removes a well-known local-privilege path on shared machines.
**Rules touched:** the 14 November amendment (endpoint files and grants must stay owner-only); "never record capability secrets in docs or logs" (unchanged, supported).
**Licence:** design only (we already have the code).

### 5. Cap output with head and tail, and keep the full text (small, low-medium risk)

`computer_exec` returns at most 64 KiB (`crates/retinue-daemon/src/computer.rs:62-64`). nod keeps the first and last 512 KiB and names the complete log in the workspace (`sandbox-and-browser.md:74-79`). A tail-only cut hides the error that came first; a head-and-tail cut with a saved file fixes that.
**Rules touched:** design language (the card must say where the rest is); storage rules for the saved file (it must live in the computer, not in the transcript).
**Licence:** design only.

### 6. Use the grant clock for any future local-execution tool (medium, medium risk)

If Retinue ever gives a bot a "run one command on my machine" tool (the obvious companion to `owner_*`), reuse `owner_computer`'s three clocks, attended-only rule and "never rememberable" card rather than inventing a fifth consent model. nod's answer is different in kind — put the command in a container — and that is not available to us without a VM.
**Rules touched:** the 14 November amendment (a live grant is visible and stoppable, tier `never`); the Remote tiers parity test.
**Licence:** design only.

### 7. A per-conversation scratch folder for disposable work (small, low risk)

Bots currently write scratch files wherever they like in the project, which shows up as untracked noise in diffs. nod gives each thread `/workspace/<thread>` with a persistent tools volume; a Retinue equivalent is `<data>/scratch/<conversation>/` offered through the agent's instructions and ignored by git. This is the cheap part of nod's workspace idea, without the container.
**Rules touched:** design language (nothing visible), codebase-map; must not be presented as isolation.
**Licence:** design only.

### 8. Do not copy their sandbox architecture (large effort, high risk)

A VM per install on Windows means WSL 2, 2 GB of runtime pack, virtualization enabled in firmware, and — decisively — **keys and sign-ins outside the sandbox**. Retinue's whole premise is that the bot uses the owner's own signed-in CLIs, so a nod-style sandbox would either need credentials inside it (against the 12 and 30 October amendments) or break the product's main advantage. **Recommendation: no.**
**Rules touched:** "no copied provider authentication"; the 12 October lanes; the 14 November amendment.
**Licence:** n/a.

### Ranked shortlist

| Rank | Idea | Effort | Why now |
| --- | --- | --- | --- |
| 1 | 1. State what a bot can reach | small | Cheap, honest, and it is the gap a security reviewer will name first |
| 2 | 4. ACL the data folder | small | Reuses code we already wrote twice |
| 3 | 3. Uncertainty protocol for computer tools | medium | Directly supports "never fabricate success" |
| 4 | 2. Construct the CLI environment | medium | Real risk reduction, real breakage risk; needs a doctor check |
| 5 | 5. Head-and-tail output | small | Small quality win |

## What I verified, and what I assumed

**Read in nod's source at `47cfee3`:** the whole of `desktop/runtime/index.mjs`, `windows.mjs`, `guest-config.mjs`, `protect-profile.ps1`, `src/providers/docker/index.ts`, `src/providers/docker/command.ts`, `src/providers/docker/workspace-network.sh`, `compose.yaml`, both Dockerfiles, `src/providers/container-browser/{index,config,bridge}.ts`, `src/providers/pi/sandbox.ts` (head), `src/server/local-files.ts` (head), `src/server/trust.ts` (head), the sandbox, security-model, install, desktop-app and workspace doc pages, `AGENTS.md`, `LICENSE`, `NOTICE`.

**Read in Retinue:** `crates/retinue-daemon/src/{owner_computer,computer,browser,worktree,worktree_cleanup,workspace,checkpoints}.rs` (headers and cited regions), `crates/retinue-agent-core/src/lib.rs` (`ApprovalMode`, `classify_permission`), `crates/retinue-adapter-claude/src/lib.rs` (cited regions), `crates/retinue-adapter-codex/src/lib.rs:757-758`, `apps/desktop/src/remote/tiers.json` (cited rows), `docs/library/content/reference/{runtimes-and-adapters,architecture}.md`, the retinue-development skill and codebase map.

**Assumed, not proven:**
- That `Auto` mode approves exactly what `classify_permission` marks `allowed`. I read the classifier, not the daemon's decision arm in `main.rs`.
- That Codex's `workspace-write` sandbox on Windows actually confines writes as documented on this machine. That is the CLI's claim, and it is worth one manual test before quoting it to anyone.
- That nod's Windows path works as its docs say. I read the code; I did not run WSL, and the clone was treated as untrusted.
- That `retinue-wsl` is unused: I found no caller in any crate, but a build-time or test-only use could exist outside `.rs` files.

## Open questions for the owner

1. **Do we ever want an OS boundary for bots, knowing the cost?** On Windows that realistically means WSL 2 or a VM, 2 GB of download, and sign-ins either copied in or lost. If the answer is no, say so in the threat model so it stops being re-litigated.
2. **Should `Auto` mode's `SAFE_COMMANDS` list be visible in the UI?** Today the owner can set `Auto` without knowing which commands it silently approves (`git`, `ls`, `rg`, `cat`, `find`, `node --version`, no shell operators). nod has no equivalent list because it has no per-call approval at all.
3. **Is the owner's environment fair game for a bot?** Scrubbing it is safer but will break someone's proxy, corporate certificate or `AWS_PROFILE`. Do we scrub by default with an opt-out list, or document inheritance and leave it?
4. **Do we want a `sandbox.*` RPC family, or is a settings sentence enough for now?** The first implies a feature; the second is honest disclosure.
5. **For the linked computer, should each task get its own folder?** We already isolate conversations by profile for the browser; the workspace itself is `/config`, shared by every conversation on that machine. nod's per-task folders are organisational only, and it says so.
