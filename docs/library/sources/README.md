# Retinue

> **What this is:** The front page of the repository: what Retinue is, how to build and test it, and where the documentation starts.
> **Status:** current. The release figures in this file are set at v0.1.0-beta.36; `main` is ahead of that release with unreleased work.
> **Last updated:** 2026-10-22

Local Windows desktop control plane for Claude Code, Codex, and OpenCode. A React/Tauri shell and a companion window share state from the `retinued` daemon. Each provider CLI keeps its own login.

**Status: `v0.1.0-beta.36`** (the version in `Cargo.toml` and `package.json`), published as an NSIS installer that is not Authenticode-signed but is signed for the updater (new key) and offered on the beta update channel; beta.32 and later update themselves; older versions install by hand once. Full native acceptance and a clean-machine install remain open ([docs/operations/release-checklist.md](docs/operations/release-checklist.md)). Release notes: [docs/releases/0.1.0-beta.36.md](docs/releases/0.1.0-beta.36.md). History and dated records: [docs/operations/session-handoff.md](docs/operations/session-handoff.md) and [docs/operations/implementation-status.md](docs/operations/implementation-status.md). The hub for every document is [docs/README.md](docs/README.md).

## Documentation

Open [docs/DOCUMENTATION.html](docs/DOCUMENTATION.html) in a browser for the full documentation (guides, how it works, and every project document). Design rules: [docs/design/design-system.md](docs/design/design-system.md). Third-party licences: [docs/legal/third-party-notices.md](docs/legal/third-party-notices.md).

## Continue this project

Start with [docs/operations/session-handoff.md](docs/operations/session-handoff.md) and [docs/operations/implementation-status.md](docs/operations/implementation-status.md). The plan remains [docs/plans/e2e-plan-v2.md](docs/plans/e2e-plan-v2.md).

## Local development

Requirements: Windows with the MSVC C++ toolchain, Rust/Cargo, Node/npm, and WebView2. Install dependencies in this checkout (`npm ci`). Do not reuse another checkout's `node_modules`.

Desktop checks, from this folder or from `apps/desktop`:

```powershell
npm run typecheck
npm test
npm run build
```

Those commands do not launch the app or the daemon.

Native development, when you intend to open the window, uses an isolated database. Never point it at `%LOCALAPPDATA%\Retinue`.

```powershell
npm run desktop:dev
```

Details: [docs/guides/windows-development.md](docs/guides/windows-development.md).

Renderer-only Vite (`npm run dev`) does not establish native IPC or a provider session.

Backend tests, in a private Cargo target and a fresh database path:

```powershell
cargo test --workspace --exclude retinue-desktop
```

An opt-in provider smoke may use provider quota. A default test pass does not mean a live provider path ran.

```powershell
npm run desktop:bundle
```

This is the local NSIS path (`bundle.targets` is `nsis` only in `apps/desktop/src-tauri/tauri.conf.json`). A trusted signed installer and a clean-machine install are still open (Phase 7). The signed updater feed is at `v0.1.0-beta.6` (superseded: the feed now offers the latest beta; see the [release checklist](docs/operations/release-checklist.md)) ([docs/operations/updater.md](docs/operations/updater.md)).

## Source map

| Path | Responsibility |
| --- | --- |
| `apps/desktop/src/` | Main and companion UI, analytics, approval and execution contracts |
| `apps/desktop/src-tauri/` | Native windows, tray, authenticated daemon proxy, file actions |
| `crates/retinue-daemon/` | Sessions, permissions, usage analytics, quota RPCs and the durable task board (`src/tasks.rs`). Budgets are retired: `budget.*` returns `unsupported` (`crates/retinue-daemon/src/main.rs:1270`) |
| `crates/retinue-adapter-*` | OpenCode ACP, Codex app-server, and Claude adapters |
| `crates/retinue-storage/` | SQLite, migrations, usage analytics |
| `crates/retinue-runtime/` | CLI discovery and launch specifications |
| `crates/retinue-usage/`, `retinue-budget/`, `retinue-wsl/` | Usage helpers and the official price table (`crates/retinue-usage/data/official-prices.json`); `retinue-budget` is a workspace member that no crate depends on (budgets are retired); WSL helpers |
| `tools/retinue-hook/` | Bounded Claude hook helper |
| `scripts/` | Windows dev, sidecar staging, and installer entry points |

## License

Retinue is open source under the [MIT License](LICENSE). The coding agents' logos in the app and on the website are trademarks of their owners and are used only to name those agents. They are not covered by the MIT License.

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- Committers and reviewers: [the repository's collaborators](https://github.com/sico-vibes/bloblex/graphs/contributors). Every change from outside the team is reviewed before it is merged.
- Approvers: [Bassem Hesham](https://github.com/sico-vibes), the owner. Every signing request is approved by hand.
- Only Retinue's own executables and its installer are signed. They are built from this repository by the [Release build](.github/workflows/release.yml) workflow on GitHub-hosted runners. Upstream libraries are never signed under this certificate.
- Every team member uses multi-factor authentication for GitHub and SignPath.

Privacy: [PRIVACY.md](PRIVACY.md). Retinue sends nothing to its author and goes online only for the purposes listed there, such as update checks, which you can turn off. How releases are signed: [docs/operations/code-signing.md](docs/operations/code-signing.md).

## References

- [IPC contract](docs/ipc-contract.md)
- [Phase 5 analytics contract](docs/reference/phase-5-analytics-contract.md)
- [Approval modes](docs/reference/approval-modes.md)
- [Product](docs/design/product.md) and [design](docs/design/design-system.md)
