# Code signing (SignPath Foundation)

> **What this is:** How Retinue's Windows installer gets an Authenticode signature for free through SignPath Foundation, what the owner sets up once, and how a signed release is made.
> **Status:** current. The workflow and policy are in place; the SignPath application and setup are the owner's to do.
> **Last updated:** 2026-11-09

The other free channels, winget and the Microsoft Store, are in [distribution.md](distribution.md).

## Why

Without a signature, Windows SmartScreen shows "Unknown publisher" and stops the installer. SignPath Foundation signs open-source projects for free.

The certificate is issued to **SignPath Foundation**, so Windows shows that name as the publisher, not Retinue's. A signature does not remove the SmartScreen warning at once. The warning fades as the signed installers build a download reputation, and that reputation stays with the certificate across releases.

## What the repository already has

- **The MIT [LICENSE](../../LICENSE)**, since SignPath requires an OSI licence.
- **A "Code signing policy" section in the [README](../../README.md)** with SignPath's required sentence, the team roles and the privacy link.
- **The website footer** carries the same sentence and links: `apps/website/components/ui/Footer.tsx`, `apps/website/content/site.ts`.
- **[PRIVACY.md](../../PRIVACY.md)**, which lists everything the app connects to.
- **[`.github/workflows/release.yml`](../../.github/workflows/release.yml)** (run by hand: Actions, Release build, Run workflow), on a GitHub-hosted runner:
  1. Builds the daemon, the hook and the app, without bundling.
  2. Marks `Retinue.exe` as the NSIS build (`scripts/patch-bundle-type.mjs`). Tauri normally writes this marker while bundling, which would break a signature made earlier.
  3. Uploads the three executables and has SignPath sign them (artifact configuration `app-binaries`).
  4. Bundles the NSIS installer from the signed executables with `--no-binary-patching`.
  5. Uploads the installer and has SignPath sign it (artifact configuration `installer`).
  6. Checks both signatures and uploads `retinue-<version>-installer` with a `SHA256SUMS.txt`.

  Until the `SIGNPATH_API_TOKEN` secret exists, steps 3 and 5 are skipped and the installer is uploaded **unsigned**. The run summary says which it is.
- **[`.signpath/artifact-configurations/`](../../.signpath/artifact-configurations/)**: the two artifact configurations to paste into SignPath.
- **`scripts/release-windows.ps1 -SignedInstaller <path>`**: publishes a downloaded signed installer. It checks the Authenticode signature, makes the updater signature locally (bound to the version, as the bundler does), then publishes the release and the beta-channel manifest as before. **The updater key never goes to CI.**

## The owner's one-time setup

1. **Apply** at https://signpath.org/apply with the repository URL `https://github.com/sico-vibes/bloblex` (or its new name after the rename). Acceptance is at SignPath's discretion. Their conditions:
   - an OSI licence and no proprietary components;
   - the project is actively maintained and already released in the form to be signed;
   - its features are described on the download page;
   - it respects privacy, announces system changes and can be uninstalled.
2. **Turn on multi-factor authentication** for GitHub and for SignPath. Their terms require it for every team member.
3. **In SignPath, once accepted:**
   - Add the **GitHub.com trusted build system** and link it to the Retinue project.
   - Create two artifact configurations with the slugs **`app-binaries`** and **`installer`**, from the XML files in `.signpath/artifact-configurations/`. SignPath's editor validates them; adjust there if it asks for changes.
   - Use the release signing policy SignPath sets up. Keep **manual approval** on: every request waits for your approval on signpath.io.
   - Create an API token for a user with the **submitter** role.
4. **In GitHub** (repository Settings, Secrets and variables, Actions):
   - Secret **`SIGNPATH_API_TOKEN`**: the API token.
   - Variables **`SIGNPATH_ORGANIZATION_ID`**, **`SIGNPATH_PROJECT_SLUG`** and **`SIGNPATH_SIGNING_POLICY_SLUG`**: from SignPath.

## Making a signed release

1. Bump the version as usual, then commit and push.
2. Run **Actions, Release build**. Approve the two signing requests on signpath.io as they arrive (executables first, then the installer). Each waits up to an hour.
3. Download the `retinue-<version>-installer` artifact and unzip it.
4. Run `scripts/release-windows.ps1 -SignedInstaller <path-to>\Retinue_<version>_x64-setup.exe` (try `-DryRun` first).
5. Check the release's asset digest against `SHA256SUMS.txt`, then record it in the [release checklist](release-checklist.md).

## Open items

- `retinued.exe` and `retinue-hook.exe` have no Windows version resource. So the `app-binaries` configuration restricts the product name on `Retinue.exe` only. SignPath asks for product name and version restrictions on every signed file. If they require them, add a version resource to both binaries in a short-path build: the resource compiler fails on the main checkout's path.
- The NSIS uninstaller inside the installer is not signed on its own.
- The first signed run on GitHub is unverified. The unsigned path has not run on GitHub yet either.
- The coding agents' logos (`apps/desktop/src/assets/providers/`) are trademarks of their owners. They are not open-source components. If SignPath objects to them, replace them with plain initials.
