# Distribution: winget and the Microsoft Store

> **What this is:** The two free channels beside the direct download: the winget package manager and the Microsoft Store. It covers what is in the repository, what the owner does once, and what to do for each release.
> **Status:** current. The tooling is ready; nothing has been submitted yet.
> **Last updated:** 2026-11-09

Code signing of the direct download is covered in [code-signing.md](code-signing.md).

| Channel | SmartScreen warning | Who signs | Updates |
| --- | --- | --- | --- |
| Direct download (GitHub release) | Until the signed installer builds reputation | SignPath Foundation | Retinue's own updater |
| winget (`winget install SicoVibes.Retinue`) | Reported not to appear: winget downloads the installer itself, so it has no "from the internet" mark | Same installer as the direct download | `winget upgrade`, or Retinue's own updater |
| Microsoft Store | None | Microsoft signs the package | The Store |

Windows 11's Smart App Control is stricter than SmartScreen. It blocks unsigned programs outright, whichever way they were downloaded. Only signing (SignPath, or the Store's own signature) gets past it.

## Why Windows flags an unsigned release

Three different Windows mechanisms, all reported as "SmartScreen". They need different answers, and only signing fixes them for users.

| What the user sees | Mechanism | Cause | What removes it |
| --- | --- | --- | --- |
| "Windows protected your PC", "Unknown publisher", **More info → Run anyway** | SmartScreen app reputation | The installer has no Authenticode signature, and every release is a new file hash with no download history, so it starts at zero reputation each time | A valid signature, then reputation that builds with downloads of that certificate |
| A program is blocked outright (`os error 4551`), or a freshly built `.exe` will not start | Smart App Control (Windows 11, enforced) | The file is unsigned and unknown to Microsoft; SAC does not offer "Run anyway" | Signing, or turning SAC off on that PC. Off cannot be undone without reinstalling Windows, so treat it as the owner's decision |
| Defender quarantines or names the installer | Microsoft Defender antivirus | A machine-learning heuristic, not a hash of known malware. The `!ml` suffix says so; `beta.51`'s installer was named `Program:Win32/Contebrew.A!ml` on 10 November 2026 | Submit the file as a false positive, fix anything the report names, and sign the release |

- **False positive:** submit the installer at <https://www.microsoft.com/en-us/wdsi/filesubmission> (choose **Software developer**), with the product name, version, the GitHub release URL and the published SHA-256. A developer account at <https://developer.microsoft.com/en-us/windows/> gives a tracked submission.
- **Metadata matters to the heuristics.** A file with no publisher, product or version resource is treated as unknown. `bundle.publisher`, `bundle.copyright` and `bundle.homepage` now fill those in for the app and the installer. `retinued.exe` and `retinue-hook.exe` still have none: it is the open item in [code-signing.md](code-signing.md).

## winget

**In the repository:** `packaging/winget/make-manifests.mjs` writes the three manifests for a release. It reads the installer's address and SHA-256 from the GitHub release, so they always match the published file. The package is `SicoVibes.Retinue`, publisher "Bassem Hesham", MIT, with the privacy page linked. Its output passes `winget validate`.

**First submission**, after the first release whose installer is named `Retinue_...`:

1. Make the manifests. They are written to `output/winget/manifests/s/SicoVibes/Retinue/<version>/`:

   ```powershell
   node packaging/winget/make-manifests.mjs --tag v<version>
   ```

2. Check them:

   ```powershell
   winget validate --manifest output/winget/manifests/s/SicoVibes/Retinue/<version>
   ```

   To try the install locally, run `winget settings --enable LocalManifestFiles` once, then `winget install --manifest <that folder>`.
3. Fork `microsoft/winget-pkgs` on GitHub. Copy the folder to `manifests/s/SicoVibes/Retinue/<version>/` and open a pull request. Microsoft's bots install the package in a sandbox and scan it with Defender. A moderator may ask about the unsigned installer until SignPath signing is in place.

**Each later release:** repeat with the new tag. You can also use `wingetcreate update SicoVibes.Retinue --version <version> --urls <installer url> --submit`.

## Microsoft Store (MSIX)

The Store signs MSIX packages itself, so this channel needs no certificate. Tauri cannot make MSIX, so the repository has its own packaging.

**In the repository:**

- `packaging/msix/AppxManifest.xml` is the package manifest:
  - full trust, since Retinue is a normal desktop app with a daemon;
  - file-system write virtualisation off, so Retinue and the coding agents it starts share the real `%LOCALAPPDATA%\Retinue` and their own folders, exactly as the installer version does;
  - a startup task, off until the owner turns it on.
- `packaging/msix/Assets/` holds the tile images, made from `assets/icon/retinue-master.png`.
- `packaging/msix/build-msix.ps1` packs the executables with `makeappx`.
- `packaging/msix/msix-version.mjs` maps versions, because the Store needs four numbers and a higher one each time. For example, `0.1.0-beta.51` becomes `0.1.51.0`, and `0.1.0` becomes `0.1.999.0`.
- The **Release build** workflow packs it from the same executables as the installer (signed, once SignPath is set up). It uploads `retinue-<version>-msix` when the Partner Center identity is configured.
- The app knows when it runs from the Store (`apps/desktop/src-tauri/src/distribution.rs`):
  - **The built-in updater stays off.** Settings, Updates says "This copy of Retinue updates through the Microsoft Store".
  - **"Start with Windows" uses the startup task.** If the owner turned Retinue off under Windows Settings, Apps, Startup, Retinue says so, because only Windows Settings can turn it back on.
  - **The hook bridge is copied to a fixed place** (`%LOCALAPPDATA%\Retinue\runtime-processes\retinue-hook.exe`). The Store install folder changes with every version, and Claude Code's settings would otherwise point at a stale path.

**The owner's one-time setup:**

1. Register a free individual developer account at https://storedeveloper.microsoft.com and reserve the name **Retinue**. If the name is taken, pick another and change the display names in `AppxManifest.xml`.
2. Copy three values from Partner Center's **Product identity** page into the GitHub repository's Actions variables:
   - `Package/Identity/Name` → **`MSIX_IDENTITY_NAME`**;
   - `Package/Identity/Publisher` (`CN=...`) → **`MSIX_PUBLISHER`**;
   - `Package/Properties/PublisherDisplayName` → **`MSIX_PUBLISHER_DISPLAY_NAME`**.
3. Run **Release build**, download `retinue-<version>-msix` and upload the `.msix` in a new submission.
4. Partner Center asks why the package uses restricted capabilities. Suggested answers:
   - **runFullTrust:** "Retinue is a desktop app that runs a local background service and starts the user's own command-line coding tools (Claude Code, Codex, OpenCode) in their project folders."
   - **unvirtualizedResources:** "Retinue and the command-line tools it starts must read and write the same configuration and data files as when those tools run on their own, for example their sign-ins and settings in the user's profile and Retinue's own data in %LOCALAPPDATA%. With write virtualisation, those tools would see different files depending on whether Retinue started them."
5. Store listing:
   - privacy policy URL `https://github.com/sico-vibes/retinue/blob/main/PRIVACY.md`;
   - the description from the website;
   - screenshots;
   - age rating questionnaire;
   - category Developer tools.

**Trying the package before submitting.** An unsigned MSIX cannot be installed directly. With Windows **Developer Mode** on (Settings, System, For developers):

1. Rename the `.msix` to `.zip` and extract it.
2. Run `Add-AppxPackage -Register <folder>\AppxManifest.xml`.
3. Remove it afterwards with `Get-AppxPackage *Retinue* | Remove-AppxPackage`.

**Open items:**

- The Store copy is unverified on a real machine. In particular:
  - its startup task;
  - the shared data folder with an installer copy present;
  - notifications;
  - the daemon and coding agents started from inside the package.
- Microsoft may refuse `unvirtualizedResources`. Without it, the Store copy keeps its data in a private per-app folder: Retinue's AppData writes are redirected. That probably still works, but it shares nothing with an installer copy, and needs testing.
- Remote's Native method opens a firewall rule through an elevated prompt, which may behave differently inside a package.
