# Retinue on a Linux host: install, start, update, stop

Date 17 November 2026. Owner decision: `AGENTS.md`, "Amendment (17 November 2026): Retinue on many machines", the "Headless Linux daemon for the VPS" bullet. Design and reasons: `docs/research/multi-device/04-linux-headless.md`.

This page is for the owner or the Director at the VPS's own terminal. It installs **`retinued`, the daemon**, as a systemd *user* unit, on the host itself — not inside Harbor's container (the container sleeps and its lifecycle belongs to Harbor; `04-linux-headless.md` section 7). The container keeps its job: it is the employee's Linux desktop and browser, which a conversation running on the host drives as a tool.

What works on this host: conversations, bots, projects, worktrees, git, the Remote gateway (Tailscale or Native), machine links, the phone app over the gateway, and secrets in owner-only files. What does not: the desktop window, dictation, the owner's-computer tools, and the browser/computer tools except through the linked Harbor container. The full list is at the end.

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

Without it, the daemon starts when you log in and stops when you log out — which is not "the VPS hosts sessions".

## 3. Where everything lives

| What | Where |
|---|---|
| Data folder | `$XDG_DATA_HOME/retinue`, else `~/.local/share/retinue` |
| Database | `<data folder>/retinue.db` (and its `-wal`) |
| Secrets | `<data folder>/secrets/` — folder `0700`, one file per secret `0600`, written atomically. **Never** in the database, a backup, a diagnostics bundle or a log. |
| Console token | `<data folder>/console.json` — `0600`, written at start, removed at a clean stop. It opens **only** the console commands, never the daemon's own capability. |
| Log file | `<data folder>/retinued.log`, plus `journalctl --user -u retinued` |
| The unit | `~/.config/systemd/user/retinued.service` |

An explicit `RETINUE_DB_PATH` overrides the database path, and the data folder is that file's parent.

## 4. Turn the gateway on

A headless host has no Settings page, so the console does it:

```sh
retinued status                       # version, folders, secrets store, Remote state
retinued remote on --method tailscale # or: --method native
retinued status                       # the URL other devices will use
```

Tailscale publishes the gateway with `tailscale serve` (never `funnel`), exactly as on Windows. The method can only be chosen while Remote is off, which is the desktop's own rule.

## 5. Pair a device (the laptop, a phone)

**From the laptop** (Settings › Machines › Add machine), with the host's URL `https://<host>.<tailnet>.ts.net`:

1. On the host: `retinued pair`. It prints the URL, the code in its `K7QM-4XPD` form, when it expires, and how to approve. **The code is never written to a log, the database or an event.**
2. On the laptop: enter the code when the dialog asks.
3. On the host: `retinued pair list` shows the waiting device (name, kind, platform, sign-in method). Approve it with the tier you want:

   ```sh
   retinued pair confirm --tier chat    # or viewer, or operator
   ```

   `retinued pair deny` refuses it. `retinued pair cancel` throws the code away.

**Native method** (a password and a pinned certificate instead of a code): the laptop shows the certificate fingerprint; compare it with the one `retinued status` prints, then sign in with the host's username and password. The request then waits exactly as above: `retinued pair list`, then `retinued pair confirm --tier chat`.

**The phone** pairs with this host in Settings › Remote access › Devices on the host's own page (`https://<host>.<tailnet>.ts.net`), and can then watch and drive the sessions that run here.

## 6. Update

```sh
cd ~/retinue-src && git pull
cargo build --release -p retinue-daemon --bin retinued
systemctl --user stop retinued
install -m 0755 target/release/retinued ~/.local/bin/retinued
systemctl --user start retinued
```

If the unit file changed: `systemctl --user daemon-reload` before starting.

**Honest cost:** a turn that is running when you stop the daemon is lost. The daemon restores its conversations and their state at the next start, and the conversation says the turn was interrupted; nothing else is lost. Update when nothing you care about is running.

## 7. Stop, restart, remove

```sh
systemctl --user stop retinued        # stop
systemctl --user restart retinued     # restart
systemctl --user status retinued      # what systemd thinks
systemctl --user disable --now retinued
rm ~/.local/bin/retinued ~/.config/systemd/user/retinued.service ~/.local/share/retinue/retinued.log
```

Removing the data folder (`~/.local/share/retinue`) deletes the database **and the secrets folder**. Keep it if you may come back.

## 8. What is not available on a Linux host

| Feature | Why, and what you get instead |
|---|---|
| The desktop window, tray, companion, dictation island | The desktop app is Windows-only (Tauri, WebView2). The window you use is on the laptop or the home PC; the VPS is reached as a machine link, or through the phone app. |
| Dictation | `retinue-speech` runs Cactus's Windows `needle.exe`. |
| The owner's-computer tools (`owner_*`) | They drive Windows UI Automation. The tools are not offered on Linux at all, and a call says so. |
| The browser and computer tools (`browser_*`, `computer_*`) | They run inside the linked Harbor container, which this daemon reaches as a tool. Works, but only while that computer is linked and awake. |
| Preview ports ("which port is this") | The scan uses Windows' `netstat`; on Linux the preview panel shows no port list. |
| "Show in folder" | Uses `xdg-open` on Linux, so it opens the folder if a graphical session is attached; a headless host has none. |
| The Native firewall rule | The elevated Windows prompt has no Linux equivalent; the console answers `unsupported`. Add the rule yourself with `ufw`, and forward the port in your router. |
| A Secret Service keyring | Not wired yet (see the report's open list). Headless Linux uses the owner-only files in `<data folder>/secrets`. |

## 9. When something is wrong

| Symptom | What to do |
|---|---|
| `retinued status` says "No headless daemon is running here." | The daemon is not running, or it was started by a desktop shell (which writes no console file — that never happens on a VPS). `systemctl --user status retinued`, then `journalctl --user -u retinued -n 50`. |
| `Turn Remote on before pairing a device.` | `retinued remote on --method tailscale`. |
| `Tailscale is not installed on this computer.` | Install Tailscale (`curl -fsSL https://tailscale.com/install.sh | sh` is Tailscale's own installer), then `sudo systemctl start tailscaled` and `tailscale up`. |
| A turn fails with "not installed" | The unit's `PATH` does not include the folder the CLI is in. Add it to the unit's `Environment=PATH=…` line and restart. |
| Nothing survives a reboot | `sudo loginctl enable-linger "$USER"` (section 2). |
| The phone page shows "not installed" | The built phone app is not on the host. Set `RETINUE_REMOTE_WEB_DIR` in the unit to a folder holding `npm run build:remote`'s output, then restart. |
