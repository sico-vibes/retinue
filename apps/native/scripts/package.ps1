# Builds the native preview and puts it somewhere the owner can install.
#
# 1. bitbotsd (+ bitbots-hook) in release, into their own target dir on C:
# 2. bitbots-native in release
# 3. stage apps/native/dist/Bitbots-Native-<version>/ and zip it
# 4. an installer only if cargo-packager is already installed (it is not part of
#    this repository, and installing it needs the network)
#
# The product name is "Bitbots (Rust preview)" and the app keeps its own database,
# so it never replaces the installed Bitbots.

[CmdletBinding()]
param(
  [string]$DaemonTarget = $(if ($env:BITBOTS_DAEMON_TARGET) { $env:BITBOTS_DAEMON_TARGET } else { 'C:\bbn\daemon-target' }),
  [string]$NativeTarget = $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'C:\bbn\target' })
)

$ErrorActionPreference = 'Stop'
$native = Split-Path -Parent $PSScriptRoot
$root = Split-Path -Parent (Split-Path -Parent $native)
$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { throw 'Could not read the app version from the root Cargo.toml.' }

Write-Host "Bitbots (Rust preview) $version"
Write-Host "  repo:    $root"
Write-Host "  daemon:  $DaemonTarget"
Write-Host "  native:  $NativeTarget"

Write-Host '1/4 bitbotsd (release)'
$env:CARGO_TARGET_DIR = $DaemonTarget
cargo build -p bloblex-daemon --release --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'bitbotsd did not build.' }

Write-Host '2/4 bitbots-hook (release, for the daemon''s hook bridge)'
cargo build -p bloblex-hook --release --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'bitbots-hook did not build.' }

Write-Host '3/4 bitbots-native (release)'
$env:CARGO_TARGET_DIR = $NativeTarget
cargo build --release --manifest-path (Join-Path $native 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'bitbots-native did not build.' }

$dist = Join-Path $native "dist\Bitbots-Native-$version"
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force $dist | Out-Null

Copy-Item (Join-Path $DaemonTarget 'release\bitbotsd.exe') $dist
$hook = Join-Path $DaemonTarget 'release\bitbots-hook.exe'
if (Test-Path $hook) { Copy-Item $hook $dist } else { Write-Warning 'bitbots-hook.exe was not found beside bitbotsd.exe; Health check and outside sessions will be limited.' }
Copy-Item (Join-Path $NativeTarget 'release\bitbots-native.exe') $dist

# The icon the release assets carry (`tauri.conf.json` bundle.icon).
$icon = Join-Path $root 'assets\icon\bloblex.ico'
if (Test-Path $icon) { Copy-Item $icon (Join-Path $dist 'bitbots-native.ico') }

$readme = @"
Bitbots (Rust preview) $version

Run bitbots-native.exe. It starts its own bitbotsd.exe beside it and keeps its
own database in %LOCALAPPDATA%\Bitbots Native\bloblex.db, so it never shares one
with the installed Bitbots. It is a preview: it shows the projects tree, the
conversations column and one conversation, and sends messages.

Log: %LOCALAPPDATA%\Bitbots Native\native.log
"@
Set-Content -Path (Join-Path $dist 'READ ME.txt') -Value $readme -Encoding utf8

$zip = "$dist.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $dist '*') -DestinationPath $zip -Force
Write-Host "4/4 packaged: $zip"

$packager = Get-Command cargo-packager -ErrorAction SilentlyContinue
if ($packager) {
  Write-Host 'cargo-packager found: building an NSIS installer (per user, Start menu shortcut)'
  cargo packager --release --formats nsis --out-dir (Join-Path $native 'dist') --manifest-path (Join-Path $native 'Cargo.toml')
  if ($LASTEXITCODE -ne 0) { Write-Warning 'cargo-packager failed; the zip is still there.' }
} else {
  Write-Host 'cargo-packager is not installed, so this run produced the zip only (no installer).'
}
