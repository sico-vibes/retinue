---
name: cloud-computer
description: The owner's cloud computer: what it is, the vps-computer tools and what each is for, where work goes on it, when to wake it and let it sleep, and how it differs from the owner's project folder. Read it before you run anything on the computer.
---

# The cloud computer

The owner runs a computer for their bots: a container on a server they control (Harbor), with a desktop of its own. It is awake or asleep independently of this machine, its `/config` disk survives a sleep or a restart, and it keeps its own tools and git checkouts. It is not this machine and it is not the owner's project folder.

"Work on the cloud" and "run it on the computer" mean the same thing: use these tools. There is no other way to reach it.

## The tools

They come from the second MCP server, `vps-computer`. All of them run through the owner's daemon, which holds the credential; you never see an address or a token.

| Tool | What it is for |
|---|---|
| `computer_status` | Its state (running, exited, unknown), CPU and memory, open port, disk use. Read it before deciding to wake anything. |
| `computer_wake` | Start it. |
| `computer_sleep` | Stop it, keeping the `/config` disk. |
| `computer_restart` | Restart it, keeping the `/config` disk. |
| `computer_logs` | The tail of its container log: the place to look when a command behaved oddly. |
| `computer_exec` | Run one shell command inside it as the desktop user, starting in `/config`. Timeout up to 5 minutes. Returns the command's output, so a quick check costs one call. |
| `computer_list_routines` | The schedules stored on the computer. |
| `computer_create_routine` | Add a cron routine (`name`, five-field `cron` in Europe/London, `command`). It runs only while the computer is awake. |
| `computer_run_routine` | Run an existing routine now, by id. |
| `computer_desktop_url` | The owner-only live desktop URL. Opening it needs the owner's Harbor login, so it is for the owner, not for you. |

## Where work goes

- Put everything under `/config/work/<project>`, one folder per project, and logs under `/config/work/logs`. `/config` is the part that survives: anything outside it is lost when the container is replaced.
- Clone from the project's remote, or copy files across with `computer_exec`; the computer cannot read this machine's disk.
- Anything longer than a few minutes (a server, a build, a watcher) belongs in the background with its output in a log file under `/config/work/logs`, then read that log rather than holding one long `computer_exec`.
- `computer_exec` runs inside the container, never on the server's host. Do not try to break out of it.

## When to wake it, and when to let it sleep

- Wake it before work that needs it: a build or test run, a browser or desktop task, a routine about to fire.
- If a call fails because the computer is asleep, wake it once and retry. Do not loop.
- Let it sleep when the work is done and nothing is scheduled soon: sleeping frees the server's resources and keeps the `/config` disk.
- Do not restart it while a routine or a long job of yours is running; a restart stops both.

## How it differs from the owner's project folder

- The project folder on this machine is where the owner's work lives. The computer is a separate place with its own copy.
- Changes made on the computer are not in the owner's project until they are brought back: commit and push there, then the owner pulls, or copy the files across. Say which of the two you did.
- Do not copy credentials, tokens or keys from this machine into the computer, or the other way. If a job there needs a secret, ask the owner for it with `request_secret` and use it for that run only.
- A routine on the computer is not a Retinue schedule (`schedule_task`). The computer's routines run in the container; a Retinue schedule runs this bot's conversation. Use whichever the owner actually asked for, and say which one you set up.

## Reporting

When you finish, report what you did, which command logs show it, where the files are on the computer, and what is still needed to bring the work into the owner's project.
