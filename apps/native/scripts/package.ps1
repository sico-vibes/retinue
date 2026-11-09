# Builds the native preview and puts it somewhere the owner can install.
#
# 1. retinued (+ retinue-hook) in release, into their own target dir on C:
# 2. retinue-native in release
# 3. stage apps/native/dist/Retinue-Native-<version>/ with install.ps1, the icon
#    and a short README, then zip it
#
# The product name is "Retinue (Rust preview)" and the app keeps its own database,
# so it never replaces the installed Retinue.
#
# cargo-packager is not used: it is not available offline. install.ps1 does the
# same job for a per-user install: copy the folder, add a Start menu shortcut, and
# remove both again with -Uninstall.

[CmdletBinding()]
param(
  [string]$DaemonTarget = $(if ($env:RETINUE_DAEMON_TARGET) { $env:RETINUE_DAEMON_TARGET } else { 'C:\bbn\daemon-target' }),
  [string]$NativeTarget = $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'C:\bbn\target' })
)

$ErrorActionPreference = 'Stop'
$native = Split-Path -Parent $PSScriptRoot
$root = Split-Path -Parent (Split-Path -Parent $native)
$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { throw 'Could not read the app version from the root Cargo.toml.' }

Write-Host "Retinue (Rust preview) $version"
Write-Host "  repo:    $root"
Write-Host "  daemon:  $DaemonTarget"
Write-Host "  native:  $NativeTarget"

Write-Host '1/4 retinued (release)'
$env:CARGO_TARGET_DIR = $DaemonTarget
cargo build -p retinue-daemon --release --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'retinued did not build.' }

Write-Host '2/4 retinue-hook (release, for the daemon''s hook bridge)'
cargo build -p retinue-hook --release --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'retinue-hook did not build.' }

Write-Host '3/4 retinue-native (release)'
$env:CARGO_TARGET_DIR = $NativeTarget
cargo build --release --manifest-path (Join-Path $native 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'retinue-native did not build.' }

$dist = Join-Path $native "dist\Retinue-Native-$version"
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force $dist | Out-Null

Copy-Item (Join-Path $DaemonTarget 'release\retinued.exe') $dist
$hook = Join-Path $DaemonTarget 'release\retinue-hook.exe'
if (Test-Path $hook) { Copy-Item $hook $dist } else { Write-Warning 'retinue-hook.exe was not found beside retinued.exe; Health check and outside sessions will be limited.' }
Copy-Item (Join-Path $NativeTarget 'release\retinue-native.exe') $dist

# The installer, which also removes the app again with -Uninstall.
Copy-Item (Join-Path $PSScriptRoot 'install.ps1') $dist

# The icon is embedded in the exe (build.rs) and also shipped here, so the folder
# looks right and a shortcut can be recreated by hand.
$icon = Join-Path $root 'assets\icon\retinue.ico'
if (Test-Path $icon) { Copy-Item $icon (Join-Path $dist 'retinue-native.ico') }

$readme = @"
Retinue (Rust preview) $version

Install:
  Right-click install.ps1, "Run with PowerShell", or:
  pwsh -ExecutionPolicy Bypass -File install.ps1
  It copies this folder to %LOCALAPPDATA%\Programs\Retinue Native and adds a
  Start menu shortcut named "Retinue (Rust preview)".

Remove:
  pwsh -ExecutionPolicy Bypass -File "`$env:LOCALAPPDATA\Programs\Retinue Native\install.ps1" -Uninstall
  That removes the shortcut and the app folder. Your data is kept.

Run:
  retinue-native.exe starts its own retinued.exe beside it and keeps its own
  database in %LOCALAPPDATA%\Retinue Native\retinue.db, so it never shares one
  with the installed Retinue.

Settings (gear, bottom left): theme and density, and copying the installed
Retinue data again.
Log: %LOCALAPPDATA%\Retinue Native\native.log
"@
Set-Content -Path (Join-Path $dist 'README.txt') -Value $readme -Encoding utf8

$zip = "$dist.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $dist '*') -DestinationPath $zip -Force
Write-Host "4/4 packaged: $zip"
