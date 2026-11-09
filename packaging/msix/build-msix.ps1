# Pack Retinue into an MSIX package for the Microsoft Store.
#
# The Store signs the package itself, so the result is left unsigned: upload it in Partner
# Center. The identity values come from Partner Center (the app's Product identity page).
# The release workflow runs this after the executables are built; see
# docs/operations/distribution.md.
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$AppExe,
    [Parameter(Mandatory)] [string]$DaemonExe,
    [Parameter(Mandatory)] [string]$HookExe,
    [string]$RemoteWebDir = '',
    [Parameter(Mandatory)] [string]$Version,
    [Parameter(Mandatory)] [string]$IdentityName,
    [Parameter(Mandatory)] [string]$Publisher,
    [Parameter(Mandatory)] [string]$PublisherDisplayName,
    [Parameter(Mandatory)] [string]$OutDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

function Find-MakeAppx {
    $command = Get-Command makeappx.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $found = Get-ChildItem -Path $kits -Filter makeappx.exe -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.DirectoryName -match '\\x64$' } |
        Sort-Object { [version]($_.Directory.Parent.Name -replace '[^0-9.]', '') } -Descending |
        Select-Object -First 1
    if (-not $found) { throw 'makeappx.exe was not found. Install the Windows SDK.' }
    return $found.FullName
}

function ConvertTo-XmlText([string]$Value) {
    return [System.Security.SecurityElement]::Escape($Value)
}

$here = $PSScriptRoot
$msixVersion = (& node (Join-Path $here 'msix-version.mjs') $Version).Trim()
if ($LASTEXITCODE -ne 0) { throw "Could not map $Version to an MSIX version." }
if ($Publisher -cnotmatch '^CN=') { throw 'The publisher must be the CN=... value from Partner Center.' }

$OutDir = [System.IO.Path]::GetFullPath($OutDir)
$layout = Join-Path $OutDir 'layout'
if (Test-Path -LiteralPath $layout) { Remove-Item -LiteralPath $layout -Recurse -Force }
New-Item -ItemType Directory -Force -Path $layout | Out-Null

# The same files, under the same names, as the NSIS installer puts in its install folder.
Copy-Item -LiteralPath $AppExe -Destination (Join-Path $layout 'Retinue.exe')
Copy-Item -LiteralPath $DaemonExe -Destination (Join-Path $layout 'retinued.exe')
Copy-Item -LiteralPath $HookExe -Destination (Join-Path $layout 'retinue-hook.exe')
if ($RemoteWebDir) {
    Copy-Item -LiteralPath $RemoteWebDir -Destination (Join-Path $layout 'remote-web') -Recurse
}
Copy-Item -LiteralPath (Join-Path $here 'Assets') -Destination (Join-Path $layout 'Assets') -Recurse

$manifest = Get-Content -LiteralPath (Join-Path $here 'AppxManifest.xml') -Raw
$manifest = $manifest.Replace('{{IDENTITY_NAME}}', (ConvertTo-XmlText $IdentityName)).
    Replace('{{PUBLISHER}}', (ConvertTo-XmlText $Publisher)).
    Replace('{{PUBLISHER_DISPLAY_NAME}}', (ConvertTo-XmlText $PublisherDisplayName)).
    Replace('{{VERSION}}', $msixVersion)
if ($manifest -match '\{\{') { throw 'A manifest placeholder was left unfilled.' }
[System.IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest, (New-Object System.Text.UTF8Encoding $false))

$package = Join-Path $OutDir "Retinue_${msixVersion}_x64.msix"
$makeappx = Find-MakeAppx
& $makeappx pack /d $layout /p $package /o /h SHA256
if ($LASTEXITCODE -ne 0) { throw "makeappx failed with exit code $LASTEXITCODE." }
Write-Host "MSIX package: $package (version $msixVersion, from $Version)"
