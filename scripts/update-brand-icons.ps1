#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$ArtworkPath,
    [string]$OutputDirectory,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $ArtworkPath) { $ArtworkPath = Join-Path $repoRoot 'assets/icons/s33d-artwork.png' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repoRoot 'assets/icons' }
$ArtworkPath = [IO.Path]::GetFullPath($ArtworkPath)
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
if (-not (Test-Path -LiteralPath $ArtworkPath -PathType Leaf)) { throw 'The approved PNG artwork is required.' }
$names = @('s33d-seed.png','s33d-seed.ico','s33d-sprouted.png','s33d-sprouted.ico','brand-manifest.json')
foreach ($name in $names) {
    $destination = Join-Path $OutputDirectory $name
    if ($destination -eq $ArtworkPath) { throw 'The artwork must not be an output file.' }
    if ((Test-Path -LiteralPath $destination) -and -not $Force) { throw 'Existing derived assets require -Force; the source artwork is never overwritten.' }
}

# Format conversion only: preserve the selected master and alpha; no redraw/crop.
Add-Type -AssemblyName System.Drawing
$source = [Drawing.Bitmap]::new($ArtworkPath)
try {
    if ($source.Width -ne $source.Height -or $source.Width -lt 256 -or $source.Width -gt 4096) {
        throw 'Approved artwork must be a square PNG between 256 and 4096 pixels.'
    }
    $null = New-Item -ItemType Directory -Path $OutputDirectory -Force
    function Get-PngBytes([int]$Size) {
        $bitmap = [Drawing.Bitmap]::new($Size, $Size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $bitmap.SetResolution(96,96)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $attributes = [Drawing.Imaging.ImageAttributes]::new()
        $stream = [IO.MemoryStream]::new()
        try {
            $graphics.Clear([Drawing.Color]::Transparent)
            $graphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $attributes.SetWrapMode([Drawing.Drawing2D.WrapMode]::TileFlipXY)
            $graphics.DrawImage($source, [Drawing.Rectangle]::new(0,0,$Size,$Size), 0,0,$source.Width,$source.Height,[Drawing.GraphicsUnit]::Pixel,$attributes)
            $bitmap.Save($stream,[Drawing.Imaging.ImageFormat]::Png)
            return ,$stream.ToArray()
        } finally {
            $stream.Dispose(); $attributes.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
        }
    }

    $windowPng = Get-PngBytes 512
    $sizes = @(16,20,24,32,40,48,64,96,128,256)
    $frames = @($sizes | ForEach-Object { ,(Get-PngBytes $_) })
    $iconStream = [IO.MemoryStream]::new()
    $writer = [IO.BinaryWriter]::new($iconStream)
    try {
        $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$sizes.Count)
        $offset = 6 + 16 * $sizes.Count
        for ($i=0; $i -lt $sizes.Count; $i++) {
            $dimension = if ($sizes[$i] -eq 256) { 0 } else { $sizes[$i] }
            $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
            $writer.Write([byte]0); $writer.Write([byte]0)
            $writer.Write([uint16]1); $writer.Write([uint16]32)
            $writer.Write([uint32]$frames[$i].Length); $writer.Write([uint32]$offset)
            $offset += $frames[$i].Length
        }
        foreach ($frame in $frames) { $writer.Write([byte[]]$frame) }
        $writer.Flush()
        $ico = $iconStream.ToArray()
    } finally { $writer.Dispose(); $iconStream.Dispose() }
    foreach ($role in @('seed','sprouted')) {
        [IO.File]::WriteAllBytes((Join-Path $OutputDirectory "s33d-$role.png"), $windowPng)
        [IO.File]::WriteAllBytes((Join-Path $OutputDirectory "s33d-$role.ico"), $ico)
    }
    $manifest = [ordered]@{
        artwork = 's33d-artwork.png'
        artwork_revision = '2026-11-12-iron-walnut-approved-v1'
        artwork_origin = 'User-supplied design, iteratively edited with built-in image generation and approved on 2026-11-12'
        source_artwork_sha256 = 'f79665fd29c24795fc87d407dcea4a63558807d44c79260003928e9d43766077'
        approved_generated_artwork_sha256 = '84f546efa586a4d2a71d1446089546b915cbbb0396135e0c169dfe3632b621f5'
        artwork_sha256 = (Get-FileHash -LiteralPath $ArtworkPath -Algorithm SHA256).Hash.ToLowerInvariant()
        artwork_size = @($source.Width,$source.Height)
        window_png_size = 512
        ico_sizes = $sizes
        roles_share_identical_artwork = $true
        files = @($names | Where-Object { $_ -ne 'brand-manifest.json' } | ForEach-Object {
            $file = Join-Path $OutputDirectory $_
            [ordered]@{name=$_;bytes=(Get-Item -LiteralPath $file).Length;sha256=(Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()}
        })
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'brand-manifest.json') -Encoding UTF8
    [pscustomobject]@{WindowPng=512;IconSizes=($sizes -join ',');Roles='installer, application, shortcuts, uninstall';Manifest='brand-manifest.json'}
} finally { $source.Dispose() }
