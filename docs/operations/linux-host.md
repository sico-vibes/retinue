# Retinue on a Linux host: install, start, update, stop

Date 17 November 2026. Owner decision: `AGENTS.md`, "Amendment (17 November 2026): Retinue on many machines", the "Headless Linux daemon for the VPS" bullet. Design and reasons: `docs/research/multi-device/04-linux-headless.md`.

This page is for the owner or the Director at the VPS's own terminal. It installs **`retinued`, the daemon**, as a systemd *user* unit, on the host itself â€” not inside Harbor's container (the container sleeps and its lifecycle belongs to Harbor; `04-linux-headless.md` section 7). The container keeps its job: it is the employee's Linux desktop and browser, which a conversation running on the host drives as a tool.

What works on this host: conversations, bots, projects, worktrees, git, the Remote gateway (Tailscale or Native), machine links, the phone app over the gateway, and secrets in the system keyring when one is running and unlocked (else in owner-only files). What does not: the desktop window, dictation, the owner's-computer tools, and the browser/computer tools except through the linked Harbor container. The full list is at the end.

## 1. What you need on the host

- A 64-bit Linux with systemd (Ubuntu 22.04 or later, Debian 12, Fedora 40, or similar).
- `git`, a C toolchain (`build-essential` / `gcc`) and Rust **only to build**; the host needs neither to run the daemon.
- The coding-agent CLIs you use, installed **for the same user the service runs as**, each signed in there: `claude`, `codex`, `opencode`. Check with `sh -lc 'command -v claude codex opencode'`.
- Tailscale, if other devices will reach this host: `tailscale up` once, and `tailscaled` enabled.

## 2. Build and install

```sh
# On the host, as the user the service will run as.
git clone <the Retinue repository> ~/retinue-src
cd ~/retinue-src
cargo build --release -p retinue-daemon --bin retinued

mkdir -p ~/.local/bin ~/.local/share/retinue
install -m 0755 target/release/retinued ~/.local/bin/retinued

install -d ~/.config/systemd/user
cp docs/operations/retinued.service ~/.config/systemd/user/retinued.service
systemctl --user daemon-reload
systemctl --user enable --now retinued
```

The unit needs the folder it logs into, so either `mkdir -p ~/.local/share/retinue` by hand (as above) or leave the unit's own `ExecStartPre` to do it.

**The one line most often missed.** A user manager starts a user's units at boot only when that user is allowed to linger:

```sh
sudo loginctl enable-linger "$USER"
```

Without it, the daemon starts when you log in and stops when you log out â€” which is not "the VPS hosts sessions".

**On Arch, start the user manager once.** With linger enabled, `/run/user/<uid>` and the user manager may not exist until something starts them, and every `systemctl --user` command then answers "Failed to connect to user scope bus via local transport". This is one command, once (it comes back by itself at boot):

```sh
sudo systemctl start "user@$(id -u).service"
```

## 3. Where everything lives

| What | Where |
|---|---|
| Data folder | `$XDG_DATA_HOME/retinue`, else `~/.local/share/retinue` |
| Database | `<data folder>/retinue.db` (and its `-wal`) |
| Secrets | The system keyring when one is running and unlocked: one item per secret in the freedesktop Secret Service (GNOME keyring or KWallet), labelled `Retinue: <name>`, found by `application=retinue` and `name=<name>`. Otherwise the owner-only files `<data folder>/secrets/` â€” folder `0700`, one file per secret `0600`, written atomically. **Never** in the database, a backup, a diagnostics bundle or a log. |
| Console token | `<data folder>/console.json` â€” `0600`, written at start, removed at a clean stop. It opens **only** the console commands, never the daemon's own capability. |
| Log file | `<data folder>/retinued.log`, plus `journalctl --user -u retinued` |
| The unit | `~/.config/systemd/user/retinued.service` |

An explicit `RETINUE_DB_PATH` overrides the database path, and the data folder is that file's parent. The unit sets `UMask=0077`, so the database, its `-wal` and the log are `0600` and belong to the service's user alone.

**Where the secrets go, and what happens when that changes (18 November 2026).** At start the daemon looks for a Secret Service on the user's session bus â€” the GNOME keyring, or KWallet with its Secret Service plugin. If one answers and its default collection is unlocked, values live there; if it is locked, or nothing answers, they stay in the owner-only files and nothing prompts (a systemd unit has nobody to answer a prompt). The choice is made once, at start, and `retinued status` prints it (`secret-service` or `files`).

When a host that was keeping files gains a keyring, the next start moves every value into it: each is written, read back to prove the copy, and only then is its file deleted. A value that does not move keeps that run on the files, so nothing becomes invisible. Removing the keyring's collection later does not bring the files back: a file was deleted only after its value was verified in the keyring.

A headless VPS normally has no session bus at all. That is the expected state, not an error: the daemon uses the files without a prompt and without a delay.

## 4. Turn the gateway on

A headless host has no Settings page, so the console does it:

```sh
retinued status                       # version, folders, secrets store, Remote state
retinued remote on --method tailscale # or: --method native
retinued status                       # the URL other devices will use
```

Tailscale publishes the gateway with `tailscale serve` (never `funnel`), exactly as on Windows. The method can only be chosen while Remote is off, which is the desktop's own rule.

## 5. Pair a device (the laptop, a phone)

**From the laptop** (Settings â€º Machines â€º Add machine), with the host's URL `https://<host>.<tailnet>.ts.net`:

1. On the host: `retinued pair`. It prints the URL, the code in its `K7QM-4XPD` form, when it expires, and how to approve. **The code is never written to a log, the database or an event.**
2. On the laptop: enter the code when the dialog asks.
3. On the host: `retinued pair list` shows the waiting device (name, kind, platform, sign-in method). Approve it with the tier you want:

   ```sh
   retinued pair confirm --tier chat    # or viewer, or operator
   ```

   `retinued pair deny` refuses it. `retinued pair cancel` throws the code away.

**Native method** (a password and a pinned certificate instead of a code): the laptop shows the certificate fingerprint; compare it with the one `retinued status` prints, then sign in with the host's username and password. The request then waits exactly as above: `retinued pair list`, then `retinued pair confirm --tier chat`.

**The phone** pairs with this host in Settings â€º Remote access â€º Devices on the host's own page (`https://<host>.<tailnet>.ts.net`), and can then watch and drive the sessions that run here.

## 6. Set this host up from its own terminal

A headless host has no Settings page, so the things a window would do are console commands. Each talks to the running daemon with `console.json`'s token; none of them reads or prints a secret.

```sh
retinued agents                        # what is installed here, and whether each is signed in
retinued settings get                  # the two machine-local settings a device needs
retinued settings set projects.devices.enabled true
retinued settings set projects.devices.folder /srv/projects
retinued bot add --name Ada --agent claude --model <id> --effort high --project /srv/projects/app
retinued bot list
```

- **The two settings are the whole allowlist:** what another machine may do here (start projects in the folder below), and that folder. Any other key is refused on read and on write, so the terminal cannot change a preference or reach a secret. The folder must exist on this host, and what is stored is its canonical path.
- **`retinued agents`** prints each provider the daemon found and its sign-in state; one that is not signed in prints its own sign-in command. Run that command as the same user the unit runs as.
- **`retinued bot add`** names a provider (`claude`, `codex` or `opencode`), and the daemon picks a running copy of it, else any installed one. The bot asks before it acts: there is no window here to approve Full access from. `--project` registers the folder as a project if it has no row yet, then places the bot there.

## 7. Update

```sh
cd ~/retinue-src && git pull
cargo build --release -p retinue-daemon --bin retinued
systemctl --user stop retinued
install -m 0755 target/release/retinued ~/.local/bin/retinued
systemctl --user start retinued
```

If the unit file changed: `systemctl --user daemon-reload` before starting.

**Updating from 0.2.0-beta.5 or earlier:** that version printed the daemon capability in its first log line, so `~/.local/share/retinue/retinued.log` may hold a dead-but-real token at rest. Clear it once while the daemon is stopped:

```sh
: > ~/.local/share/retinue/retinued.log     # or: rm the file
```

This version prints no capability on a headless host, and the unit's `UMask=0077` keeps the new log and the database `0600`.

**Honest cost:** a turn that is running when you stop the daemon is lost. The daemon restores its conversations and their state at the next start, and the conversation says the turn was interrupted; nothing else is lost. Update when nothing you care about is running.

## 8. Stop, restart, remove

```sh
systemctl --user stop retinued        # stop
systemctl --user restart retinued     # restart
systemctl --user status retinued      # what systemd thinks
systemctl --user disable --now retinued
rm ~/.local/bin/retinued ~/.config/systemd/user/retinued.service ~/.local/share/retinue/retinued.log
```

Removing the data folder (`~/.local/share/retinue`) deletes the database **and the secrets folder**. Keep it if you may come back.

## 9. What is not available on a Linux host

| Feature | Why, and what you get instead |
|---|---|
| The desktop window, tray, companion, dictation island | The desktop app is Windows-only (Tauri, WebView2). The window you use is on the laptop or the home PC; the VPS is reached as a machine link, or through the phone app. |
| Dictation | `retinue-speech` runs Cactus's Windows `needle.exe`. |
| The owner's-computer tools (`owner_*`) | They drive Windows UI Automation. The tools are not offered on Linux at all, and a call says so. |
| The browser and computer tools (`browser_*`, `computer_*`) | They run inside the linked Harbor container, which this daemon reaches as a tool. Works, but only while that computer is linked and awake. |
| Preview ports ("which port is this") | The scan uses Windows' `netstat`; on Linux the preview panel shows no port list. |
| "Show in folder" | Uses `xdg-open` on Linux, so it opens the folder if a graphical session is attached; a headless host has none. |
| The Native firewall rule | The elevated Windows prompt has no Linux equivalent; the console answers `unsupported`. Add the rule yourself with `ufw`, and forward the port in your router. |
| A Secret Service keyring | Used when one is running and unlocked for this user (GNOME keyring, or KWallet's Secret Service plugin); otherwise the owner-only files in `<data folder>/secrets`. A headless host usually has none, which is expected. |

## 10. When something is wrong

| Symptom | What to do |
|---|---|
| `retinued status` says "No headless daemon is running here." | The daemon is not running, or it was started by a desktop shell (which writes no console file â€” that never happens on a VPS). `systemctl --user status retinued`, then `journalctl --user -u retinued -n 50`. |
| `Turn Remote on before pairing a device.` | `retinued remote on --method tailscale`. |
| `Tailscale is not installed on this computer.` | Install Tailscale (`curl -fsSL https://tailscale.com/install.sh | sh` is Tailscale's own installer), then `sudo systemctl start tailscaled` and `tailscale up`. |
| A turn fails with "not installed" | The unit's `PATH` does not include the folder the CLI is in. Add it to the unit's `Environment=PATH=â€¦` line and restart. |
| `retinued status` says `files` although a keyring is running | The keyring is locked, or the unit's environment has no session bus. Unlock the keyring and restart the unit; `journalctl --user -u retinued` says once at start whether a locked keyring was found. Nothing moves on its own until the next start. |
| Nothing survives a reboot | `sudo loginctl enable-linger "$USER"` (section 2). |
| The phone page shows "not installed" | The built phone app is not on the host. Set `RETINUE_REMOTE_WEB_DIR` in the unit to a folder holding `npm run build:remote`'s output, then restart. |
