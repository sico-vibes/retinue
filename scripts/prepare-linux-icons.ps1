# Resize assets/icon/retinue-master.png into the PNG set the Linux bundles use
# (deb installs icons under hicolor/<size>/apps/Retinue.png; AppImage embeds the
# same list). Same resampling as scripts/prepare-logo.ps1 so the Linux icons keep
# the look the Windows ones have.
#
# Run after the master changes:  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-linux-icons.ps1
#
# The output names are the Tauri convention: `128x128@2x.png` is recognised as a
# high-density 256 px icon by the bundler's `is_retina` (file stem ends `@2x`).
param(
    [string]$Source = '..\assets\icon\retinue-master.png'
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$repoRoot = Split-Path -Parent $PSScriptRoot
$iconDir = Join-Path $repoRoot 'assets\icon'
$Source = if ([System.IO.Path]::IsPathRooted($Source)) { [System.IO.Path]::GetFullPath($Source) } else { [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot $Source)) }
if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Source image not found: $Source" }

function New-ResizedPng([System.Drawing.Bitmap]$Bitmap, [int]$Size) {
    $output = [System.Drawing.Bitmap]::new($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($output)
    try {
        $graphics.Clear([System.Drawing.Color]::Transparent)
        $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
        $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
        $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $graphics.DrawImage($Bitmap, [System.Drawing.Rectangle]::new(0, 0, $Size, $Size))
    } finally {
        $graphics.Dispose()
    }
    return $output
}

$master = [System.Drawing.Bitmap]::new($Source)
if ($master.Width -ne $master.Height) {
    $master.Dispose()
    throw "The master icon must be square; $Source is $($master.Width)x$($master.Height)."
}

$outputs = @(
    @{ Name = '32x32.png'; Size = 32 },
    @{ Name = '128x128.png'; Size = 128 },
    @{ Name = '128x128@2x.png'; Size = 256 },
    @{ Name = '256x256.png'; Size = 256 },
    @{ Name = '512x512.png'; Size = 512 }
)

try {
    New-Item -ItemType Directory -Force -Path $iconDir | Out-Null
    foreach ($output in $outputs) {
        $path = Join-Path $iconDir $output.Name
        $frame = New-ResizedPng $master $output.Size
        try { $frame.Save($path, [System.Drawing.Imaging.ImageFormat]::Png) }
        finally { $frame.Dispose() }
        Write-Output "Wrote $path ($($output.Size)x$($output.Size))"
    }
} finally {
    $master.Dispose()
}
