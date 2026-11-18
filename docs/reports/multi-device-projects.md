# Piece 6: starting a project on another machine (report and contract)

Lane: Ada (DeepSeek V4.1 Flash, one of several worktrees). Date 17 November 2026. Section: piece 6 of
`docs/research/multi-device/03-design-and-order.md`, under the owner's amendment of 17 November 2026
("Projects on another machine, from any of the owner's devices", `AGENTS.md`).

**This file is the contract.** The Director builds the screens from it while the lane works. It opens with
every method, event and reply shape; the running log at the end says what is done, what changed, and what
was verified by running it.

---

## 1. What the owner asked for, and what this lane builds

> **Projects on another machine, from any of the owner's devices.** A paired device may list folders under
> a base folder the host's owner sets (never outside it, symlinks included, names only), register a folder
> there as a project, and clone any URL there. Off until the host's owner turns it on; every action is in
> that host's activity list. This amends "A remote device must not create projects" and the clone and
> folder-listing rows for this path only. — `AGENTS.md`, 17 November 2026

So: **any paired device**, not only a paired machine. The design (`03`, A2) proposed `machine` only; the
amendment is newer and says "any of the owner's devices", so a phone at `operator` may browse and create
too. If the Director wants machines only, it is one gateway kind check (see `remote/web.rs`), and it should
be said in the amendment first.

## 2. Settings (desktop only)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `projects.devices.enabled` | boolean | absent, which means off | Whether a paired device may list folders, register one, or clone on this computer. Only the desktop may write it (`settings.set` is `never` for every device). |
| `projects.devices.folder` | string | absent | The base folder. Must be empty (not set) or an absolute folder path, at most 1024 characters. Only `project.browse` children of it are ever listed or accepted. |

Both are refused by `backup.export` (`backup.rs` `exportable_setting`): a folder path and this machine's
switch belong to this machine, like `projects.folder`. They are not in `SETTING_ALLOWLIST`.

The switch off refuses **all three** far-side methods, with `forbidden_remote` and one sentence:

> Projects from other machines are off on this computer. Turn them on in Settings › Projects.

Nothing in this lane reads either setting at boot; there is no background work to start or stop.

## 3. Far side: the methods a paired device calls through the host's gateway

These are ordinary daemon methods with a `tiers.json` row, so the gateway enforces the tier and the kind
before the handler runs. New class `workspace` (see section 3.4).

### 3.1 `project.browse` — `{ path?: string }`

Lists the immediate children of a folder that must be inside the base folder. Names and kinds only: no
file contents, no sizes, no timestamps.

```json
{
  "base":  "C:\\Users\\me\\Projects",
  "path":  "C:\\Users\\me\\Projects\\retinue",
  "parent": "C:\\Users\\me\\Projects",
  "entries": [
    { "name": "src",  "kind": "folder" },
    { "name": "Cargo.toml", "kind": "file" }
  ],
  "truncated": false
}
```

- `path` omitted or `""` browses the base folder itself.
- `parent` is `null` at the base folder, so the dialog knows it cannot go up.
- `entries` is sorted by name, case-insensitive, at most 500 entries; `truncated` says one was dropped.
- `kind` is `"folder"` or `"file"`. A symbolic link or junction is listed only when it resolves inside the
  base folder, and then by what it resolves to; one that resolves outside is **not listed at all**, so a
  screen cannot offer it.
- Refusals (all `forbidden_remote` or `invalid_argument`, one sentence each):
  - the switch is off (section 2);
  - no base folder is set: "Choose the folder other machines may use for new projects in Settings › Projects first.";
  - the base folder is gone: "The folder other machines may use for new projects is missing on this computer.";
  - the path is not absolute, holds a `..` segment, or does not resolve inside the base folder:
    "That folder is outside the folder this computer shares with other machines.";
  - the path is not an existing folder: "That folder is not on this computer."

### 3.2 `project.createFromFolder` — `{ path: string, name?: string }`

Registers an existing folder under the base folder as a project. `name` defaults to the folder's own name.
Uses the desktop's own `project.create` underneath, so the row, its repository identity and its
`project.changed` event are exactly what the desktop makes.

```json
{ "project": { "id": "9f2…", "name": "retinue", "path": "C:\\Users\\me\\Projects\\retinue" } }
```

The project is reduced to those three fields on purpose: the full row carries `overrides` (a setup script)
and `sharedSettings`, which must not travel. Refusals as in 3.1. A name that is empty after trimming falls
back to the folder's name; a name over 80 characters is refused by storage as `invalid_argument`. Neither
`projects.name` nor `projects.path` is unique in the database, so a second registration of the same folder
under another name succeeds, exactly as `project.create` lets the desktop do.

### 3.3 `project.clone` — `{ url, parent, name?, confirm: true }` (existing method, now reachable)

The same method the desktop calls, with the same URL checks, the same job list in memory and the same
`project.clone.changed` event. **For a device call only, `parent` must be inside the base folder** (the
gateway stamps `deviceCall: true`; a device cannot clear it). The desktop still clones anywhere.

- It uses the host's own git sign-in. No credential is read, passed or printed by this code, on either path.
- `detail` on a failed job is git's first line, made single-line (`scaffold::one_line`), never a path list.
- A device cannot cancel or list jobs (`project.clone.cancel` and `project.clone.list` stay `never`);
  it reads one job with `project.clone.status`.

### 3.4 `project.clone.status` — `{ id: string }`

```json
{
  "job": {
    "id": "6b1…",
    "url": "https://github.com/team/repo.git",
    "target": "C:\\Users\\me\\Projects\\repo",
    "state": "running",
    "detail": null,
    "project": null
  }
}
```

- `state` is `running`, `done`, `failed` or `cancelled`; `detail` is the one plain sentence on failure.
- `project` is `null` until the clone is done, then `{ "id", "name" }`; the full row never crosses.
- `not_found` once the job has left the in-memory list (at most 20 are kept).
- Refusals: the switch off, and `invalid_argument` for an empty id. The switch gate applies to every
  caller, the desktop included (there is no local caller: a window reads its own machine's jobs with
  `project.clone.list` and a linked machine's with `machine.clone.status`).

### 3.5 The new class, and the rows

`tiers.json` gains one class, `workspace`, and four rows:

| Method | Class | Minimum tier | Why |
|---|---|---|---|
| `project.browse` | workspace | operator | It lists folders under the base folder the owner set. |
| `project.createFromFolder` | workspace | operator | It registers a folder on the host as a project. |
| `project.clone` | workspace | operator | It writes a folder on the host and may use the host's git sign-in. |
| `project.clone.status` | workspace | operator | It reads one clone's progress; the folder it names is inside the base folder. |

A `workspace` method is refused during quiet hours (it writes), refused below `operator`, and **writes one
activity line** when it succeeds (`audit_kind`): *"<device> sent project.browse from the remote app."* That
is the "one activity line per action" the amendment asks for. A refusal by the switch or by confinement
does not write a line; the device gets the sentence.

`project.clone.cancel` and `project.clone.list` stay `never` for devices.

The TypeScript policy (`apps/desktop/src/remote/remoteTiers.ts`) gains the same class, and the goldens in
`apps/desktop/src/remote/fixtures/policy-goldens.json` gain four cases (one allowed, one below tier, one in
quiet hours, one refusal sentence).

## 4. Near side: the desktop's own link, `machine.*`

These are the four methods the window calls. Each is `never` for remote devices, like `machine.projects`:
the link is the desktop's, and a phone must not drive another machine through this one.

| Method | Parameters | Far method | Reply |
|---|---|---|---|
| `machine.project.browse` | `{ machineId, path? }` | `project.browse` | the far reply, unchanged |
| `machine.project.create` | `{ machineId, path, name? }` | `project.createFromFolder` | the far reply, unchanged |
| `machine.project.clone` | `{ machineId, url, parent, name?, confirm: true }` | `project.clone` | the far reply, unchanged |
| `machine.clone.status` | `{ machineId, jobId }` | `project.clone.status` | the far reply, unchanged (`{job}`); `jobId` becomes the far `id` |

All four need tier **`operator`** on the link, checked before the request with the link's own tier, exactly
as `machine.permission.reply` does; a refusal names the machine and the tier to change. A machine that is
offline or revoked gives the existing sentence. A far-side refusal is passed through unchanged
(`machines::failure`), so the window shows the host's own words.

`machine.project.create` and `machine.project.clone` refresh the machine's snapshot afterwards, so the
new project appears in `machine.projects` without waiting for the 20 s read.

## 5. Events

**No new event type.** The far side emits what it already did:

- `project.changed` (created) when a folder is registered or a clone finishes; `drop` for devices, so the
  window's own `machine.projects` read shows it.
- `project.clone.changed` on every clone transition; `drop` for devices.
- `remote.activity` carries the activity lines; `drop` for devices (the activity list is piece 7).

The window learns about a device-side clone only through `machine.clone.status`, which it polls while a
clone is running (2 s is honest and cheap: the far answer is one in-memory job). When piece 1 (the live
link) lands, that poll can go.

## 6. Bot tools

None. No bot gets a tool that starts a project on another machine; this is the owner's own action from the
window.

## 7. Who else is affected

- **The Director's UI.** Settings › Projects needs the switch and the base folder picker; the
  "New project on <machine>" dialog needs the browse tree, the clone form and a progress row. The shapes
  above are final.
- **The phone app** (`apps/desktop/src/remote/`). `remoteTiers.ts` and `tiers.json` are read by the phone
  too, so a phone at `operator` *can* call `project.browse` and `project.createFromFolder` already. No
  phone screen is part of this lane; if the phone should not have it, the amendment needs a kind check.
  `RemoteActivityKind` gains `workspace`: the daemon writes that kind on every successful project action,
  and piece 7's activity list will show it.
- **Backups and diagnostics.** Neither setting travels.
- **The threat model.** Section 24 of `docs/remote/THREAT_MODEL.md` carries nine rows (W1–W9): a device
  browses the host's disk, a link reaches outside the base folder, a device makes the host clone a URL, a
  device claims to be the desktop, one device reads another's job, a project row carries a setup script, a
  folder path travels in a backup, quiet hours and tier, and a window driving a third machine through this
  one. `docs/reference/invariants.md` gained the one-line rule with the tests that pin it.

## 8. Running log

- **17 November 2026, start.** Report written before any code. Shapes above are the intended ones; nothing
  has been run yet.
- **17 November 2026, code.** `project_share.rs` carries the switch, the base folder, the confinement
  (`confined`/`inside`/`fold`) and the two methods; `project_clone.rs` gains `status` and the device-only
  parent check; `machines.rs` gains the four near-side pass-throughs (`call_checked` was split out of
  `call_through` so `machine.clone.status` can rename `jobId` to `id`); `remote/web.rs` stamps `deviceCall`
  on every `project.*` call and maps class `workspace` to the `workspace` activity kind; `main.rs` routes
  the two methods, validates the two settings and reaches the four near-side ones. `tiers.json` and
  `remoteTiers.ts` gain class `workspace`; `RemoteActivityKind` gains `workspace` (the daemon writes that
  kind, so the phone's type now says so too).
- **17 November 2026, what the change found in the contract.** `projects.name` and `projects.path` are not
  unique in the database, so registering the same folder twice under two names succeeds; §3.2 said
  `conflict`, which was wrong and is corrected. The gateway stamps `deviceCall` on a *link's* call too, so
  the desktop asking another machine to clone is confined exactly like a phone: that is intended, and §4
  says so.
- **17 November 2026, verified by running.**
  - `npm run typecheck` (tsc -b): clean.
  - `npx vitest run src/remote --maxWorkers=4`: **13 files, 243 tests, all pass** — including
    `policyGoldens.test.ts` (102, with the five new cases), `remoteTiers.parity.test.ts` (7, so every new
    method has a row and no row is stale), `remoteTiers.test.ts` (39, with the `workspace` class
    assertion), and `remoteTauri.parity.test.ts` (5).
  - `npm run check:architecture`: passes.
  - `npm run check:format`: fails only because `crates/retinue-daemon/build.rs` is moved aside for the
    compile check below; it passes again once that file is restored (see the next bullet).
- **17 November 2026, Rust: compiled (tests included), not executed.** On this PC Smart App Control
  refuses every freshly linked executable (see `references/verification.md`), so `cargo test -p
  retinue-daemon` cannot run the binary it just linked. What did run:
  - `cargo clippy -p retinue-daemon --all-targets -- -D warnings` against a warm shared target, with
    `RUST_MIN_STACK=33554432`: **exit 0, no warnings**. `--all-targets` compiles the test module, so
    `project_share_tests.rs`, `machines_tests.rs` and the widened `LocalRepo` are typechecked, and every
    clippy lint is an error here, so the new code is clean by that standard.
  - The check needs the daemon's own `build.rs` moved aside and `skill_library.rs`'s `OUT_DIR` include
    stubbed (the freshly built build-script exe is blocked). **Both are restored before handover** and
    `git status` shows neither; `npm run check:format` then passes (1,480 files).
  - **Not run:** the daemon's test suite, on this machine, for the reason above. The Director's CI runs it.
- **17 November 2026, not verified at all.** A real phone and a real second machine (two daemons over
  Native on loopback stand in for the second, and that is what the link tests do), the Director's Settings
  screen and dialog, and `project.clone.status` polling from a window.


