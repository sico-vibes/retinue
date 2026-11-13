// The prompt an owner hands to a coding agent on their own server. It describes the computer and the
// small service Retinue's daemon talks to, exactly as the daemon expects it. Keep it in step with
// `crates/retinue-daemon/src/computer.rs` and `docs/ipc-contract.md`.
export const COMPUTER_SETUP_PROMPT = `Set up a "bot computer" on this server that my Retinue desktop app can link to. Retinue's employees (coding agents) will use it for browser work, long jobs and routines, and I will watch its screen from Retinue.

Build it in this order, check each part works before the next, and tell me what you ran.

1. The computer
- One Docker container running a Linux desktop that is reachable in a browser (the linuxserver "webtop" XFCE image is a good base). Add Node, Python 3, Git, Chromium, Playwright, xdotool and ImageMagick.
- Persistent home volume mounted at /config. Limits: 2 CPUs, 4 GB memory, 1024 processes, restart "unless-stopped".
- Its own Docker network. No Docker socket, no host network, no host folders other than its own volume. Publish its desktop port on 127.0.0.1 only.

2. The service ("Harbor")
A small HTTPS service on this server, behind my existing reverse proxy and TLS certificate, never plain HTTP on a public address. Every route below is under <address>/harbor and needs the header "Authorization: Bearer <token>". Answer 401 with {"error":{"code":"unauthorized","message":"..."}} when the token is missing, wrong or revoked.

- GET  /api/computer  ->  {"computer":{"id","name","status","running":true|false,"since":ISO time or null,"restarts":0,"stats":{"cpu":"3.1%","memory":"1.2GiB / 4GiB","memoryPercent":"30%","pids":"180"} or null,"disk_bytes":0},"limits":{"cpu":"2","memory":"4g","pids":"1024"},"desktop_url":"<address>/harbor/computer/view/"}
  "status" is Docker's own word ("running", "exited"), or "missing" when the container does not exist.
- POST /api/computer/action/wake | /action/stop | /action/restart  ->  {"computer":{...}}. Wake creates the container if it is missing. Stop keeps the volume.
- GET  /api/computer/logs?tail=200  ->  {"logs":"..."}  (tail 20 to 1000)
- POST /api/computer/exec  {"command":"...","timeout_ms":120000}  ->  {"ok":true|false,"code":0,"output":"..."}
  Run it with: docker exec --user <desktop user> --workdir /config <container> sh -lc "<command>". Inside the container only, never on the host. Refuse commands over 8,000 characters, cap the timeout at 300,000 ms and the output at 8 MB.
- GET  /api/computer/routines  ->  {"routines":[{"id","name","cron","command","enabled","createdAt","lastRunAt","lastResult"}]}
- POST /api/computer/routines  {"name","cron","command"}  (five-field cron), and POST /api/computer/routines/<id>/run. Routines run the command the same way as exec, and only while the computer is awake.
- /computer/view/  ->  the live desktop, proxied from the container, HTTP and WebSocket. Accept the same bearer token here as well as my own sign-in, so Retinue can show the screen.

3. Tokens and safety
- Tokens are random (at least 32 bytes), start with "hrb_", and are stored hashed. I must be able to list, name and revoke them. A token reaches this one computer and nothing else on the server.
- Rate-limit failed sign-ins and bad tokens. Keep an audit line for every action, exec and routine run (who, what, when, and the first 500 characters of output). Never log a token.
- Only ever act on the one container you created and labelled. Refuse anything else, even with a valid token.

4. Hand back
When it all works, give me exactly two things and nothing secret in between:
- the address, for example https://dashboard.example.com
- one new token, shown once
I will paste them into Retinue under Settings, Integrations, Computer. Also tell me how to revoke the token later.`
