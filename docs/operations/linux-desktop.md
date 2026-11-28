# Retinue on Linux

Linux ships the same window as Windows, built by Tauri 2: a **deb**, an
**AppImage** and an **rpm** for `x86_64`. The daemon (`retinued`) is bundled
beside the app and runs headless on the same machine, as it does on Windows.
macOS and mobile are not part of this port.

## Install

- **deb** (Debian, Ubuntu 22.04 or newer): `sudo apt install ./Retinue_<version>_amd64.deb`.
  It pulls `libwebkit2gtk-4.1-0`, `libgtk-3-0` and `libayatana-appindicator3-1`.
- **AppImage** (any glibc 2.35 or newer): `chmod +x Retinue_<version>_amd64.AppImage`
  and run it. On a system without FUSE 2, run it with `--appimage-extract-and-run`.
- **rpm** (Fedora): `sudo dnf install ./Retinue-*.rpm`.

The desktop entry is `Retinue.desktop`: name Retinue, category Development,
`StartupWMClass=Retinue`, which is the window class the main binary sets. No URI
scheme is registered on either platform, so there is no MIME handler.

## Build it locally

On Ubuntu 22.04 or newer:

```bash
sudo apt-get install -y build-essential pkg-config cmake \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf libssl-dev
npm ci
cargo build -p retinue-daemon --bin retinued -p retinue-hook --bin retinue-hook --release
node scripts/stage-daemon.mjs --release      # writes binaries/retinued-<triple>, mode 0755
cd apps/desktop
npx tauri build --bundles deb,appimage,rpm \
  --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

The bundles land in `target/release/bundle/{deb,appimage,rpm}`. The `--config`
override skips updater signing, which needs the owner's key; a release build
uses it. The Linux-only settings (targets, icons, deb dependencies) are in
`apps/desktop/src-tauri/tauri.linux.conf.json`, which Tauri merges over
`tauri.conf.json`; Windows keeps its own file, so `nsis` remains the only
Windows target.

The icon set (`assets/icon/32x32.png`, `128x128.png`, `128x128@2x.png`,
`256x256.png`, `512x512.png`) is made from `retinue-master.png`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-linux-icons.ps1
```

## How a release adds Linux

1. Bump the version as usual and push. Run **Release build**
   (`.github/workflows/release.yml`); it now has two jobs after the CI check:
   `build` (Windows, unchanged) and `linux` on `ubuntu-22.04`. The Linux runner
   is 22.04 so the deb and AppImage link against glibc 2.35 and start on every
   distribution this port supports; a 24.04 build would need 2.39.
2. Download both artifacts: `retinue-<version>-installer` (already signed by
   SignPath when that is set up) and `retinue-<version>-linux` (AppImage, deb,
   rpm and their `SHA256SUMS.txt`).
3. Publish from a clean `main` checkout, where the updater key lives:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-windows.ps1 `
     -Version <version> -LinuxBundleDir <downloaded linux folder> [-SignedInstaller <exe>]
   ```

   The script packs the AppImage into `<AppImage>.tar.gz` (member mode 0755, the
   only layout the updater can install), signs it with the updater key, uploads
   the AppImage, the archive, the `.sig`, the deb and the rpm beside the Windows
   installer, and writes one `latest.json` with both `windows-x86_64` and
   `linux-x86_64`. The key is never in CI. `-SkipLinux` publishes a Windows-only
   release.
4. Upload `SHA256SUMS.txt` yourself, as for Windows; the script prints the
   Linux digests.

## Updates

Tauri's updater replaces an **AppImage** in place; it cannot upgrade a deb or an
rpm install (those are owned by the package manager). So the app must present
self-update only for an AppImage and say "update with your package manager"
otherwise; that switch is the desktop shell's (`distribution`/updater state) and
the UI reads it.

## Known caveats

- **Wayland.** The window is transparent and relies on the compositor; on
  compositors without a blur/alpha surface it can render opaque or black. If a
  window is blank, try `GDK_BACKEND=x11`. `app.enableGTKAppId` is off, as on
  Windows, so the single-instance plugin keeps handling a second launch.
- **NVIDIA proprietary drivers.** WebKitGTK's DMA-BUF renderer is unreliable
  there and shows a blank webview; `WEBKIT_DISABLE_DMABUF_RENDERER=1` is the
  workaround.
- **AppImage on a system without FUSE 2** needs `--appimage-extract-and-run`.
