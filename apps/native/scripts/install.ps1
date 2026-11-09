<#
.SYNOPSIS
  Installs (or removes) the Retinue Rust preview: the folder, a Start menu
  shortcut, and nothing else.

.DESCRIPTION
  Install copies the folder this script sits in to
  %LOCALAPPDATA%\Programs\Retinue Native and adds a Start menu shortcut named
  "Retinue (Rust preview)" that runs retinue-native.exe with the app's icon.

  -Uninstall removes the shortcut and that installed folder. It never touches the
  app's data in %LOCALAPPDATA%\Retinue Native (its database, its log and its
  settings), so uninstalling and reinstalling keeps your projects and
  conversations.

.EXAMPLE
  pwsh .\install.ps1

.EXAMPLE
  pwsh .\install.ps1 -Uninstall
#>
[CmdletBinding()]
param(
  [switch]$Uninstall,
  [string]$Destination = (Join-Path $env:LOCALAPPDATA 'Programs\Retinue Native'),
  [string]$ShortcutName = 'Retinue (Rust preview)'
)

$ErrorActionPreference = 'Stop'

$source = $PSScriptRoot
$dataFolder = Join-Path $env:LOCALAPPDATA 'Retinue Native'
$programs = [Environment]::GetFolderPath('Programs')
$shortcutPath = Join-Path $programs "$ShortcutName.lnk"
$exe = Join-Path $Destination 'retinue-native.exe'

function Remove-Shortcut {
  if (Test-Path -LiteralPath $shortcutPath) {
    Remove-Item -LiteralPath $shortcutPath -Force
    Write-Host "Removed the Start menu shortcut: $shortcutPath"
  } else {
    Write-Host "No Start menu shortcut at $shortcutPath"
  }
}

# Refuse to replace a folder that is not ours: a mistyped -Destination must never
# wipe something unrelated. Ours is empty, or already holds retinue-native.exe.
function Assert-Replaceable([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) { return }
  if ($Path -eq $dataFolder) { throw "Refusing to touch ${Path}: that is the data folder." }
  $entries = @(Get-ChildItem -LiteralPath $Path -Force -ErrorAction SilentlyContinue)
  if ($entries.Count -eq 0) { return }
  if (Test-Path -LiteralPath (Join-Path $Path 'retinue-native.exe')) { return }
  throw "Refusing to touch ${Path}: it is not empty and does not hold retinue-native.exe. Remove it yourself, or pass -Destination with another folder."
}

if ($Uninstall) {
  Remove-Shortcut
  if (Test-Path -LiteralPath $Destination) {
    Assert-Replaceable $Destination
    Remove-Item -LiteralPath $Destination -Recurse -Force
    Write-Host "Removed the app folder: $Destination"
  } else {
    Write-Host "No app folder at $Destination"
  }
  Write-Host "Your data is still in $dataFolder (database, log, settings). Remove it by hand if you want a clean slate."
  return
}

if (-not (Test-Path -LiteralPath (Join-Path $source 'retinue-native.exe'))) {
  throw "install.ps1 must sit beside retinue-native.exe; $source does not have it. Unzip the whole package first."
}

# 1. The folder.
Assert-Replaceable $Destination
if (Test-Path -LiteralPath $Destination) {
  Remove-Item -LiteralPath $Destination -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
Copy-Item -Path (Join-Path $source '*') -Destination $Destination -Recurse -Force
# The uninstaller travels with the app, so it can be run from the installed copy.
Copy-Item -LiteralPath $PSCommandPath -Destination (Join-Path $Destination 'install.ps1') -Force
Write-Host "Installed to $Destination"

# 2. The Start menu shortcut, with the app's own icon.
New-Item -ItemType Directory -Force -Path $programs | Out-Null
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $exe
$shortcut.WorkingDirectory = $Destination
$shortcut.IconLocation = "$exe,0"
$shortcut.Description = 'Retinue in Rust: the native preview'
$shortcut.Save()
Write-Host "Added the Start menu shortcut: $shortcutPath"

Write-Host ''
Write-Host 'Run "Retinue (Rust preview)" from the Start menu.'
Write-Host "Its data stays in $dataFolder, so Retinue and this preview never share a database."
