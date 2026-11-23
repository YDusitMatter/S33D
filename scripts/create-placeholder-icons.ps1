#Requires -Version 5.1
# Historical helper for the first Alpha placeholders; never overwrites assets.
# Current supplied artwork is maintained with update-brand-icons.ps1.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if ($PSVersionTable.PSVersion.Major -lt 6) {
    Add-Type -AssemblyName System.Drawing
} else {
    # Modern PowerShell already includes System.Drawing.Common. Loading the
    # legacy forwarding assembly by name can stall assembly discovery.
    $null = [Drawing.Bitmap]
}
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$directory = Join-Path $root 'assets/icons'
$names = @('s33d-seed', 's33d-sprouted')
foreach ($name in $names) {
    foreach ($extension in @('png', 'ico')) {
        if (Test-Path -LiteralPath (Join-Path $directory "$name.$extension")) {
            throw 'Icon assets already exist. This placeholder helper never overwrites artwork.'
        }
    }
}
$null = New-Item -ItemType Directory -Path $directory -Force
$bitmap = New-Object Drawing.Bitmap(1024, 1024)
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$outline = New-Object Drawing.Drawing2D.GraphicsPath
$background = New-Object Drawing.SolidBrush([Drawing.Color]::FromArgb(255, 32, 48, 60))
$accent = New-Object Drawing.SolidBrush([Drawing.Color]::FromArgb(255, 113, 224, 201))
$foreground = New-Object Drawing.SolidBrush([Drawing.Color]::White)
$smallFont = New-Object Drawing.Font('Segoe UI', 175, [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
$largeFont = New-Object Drawing.Font('Segoe UI', 440, [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
$format = New-Object Drawing.StringFormat
try {
    $graphics.Clear([Drawing.Color]::Transparent)
    $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.TextRenderingHint = [Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $format.Alignment = [Drawing.StringAlignment]::Center
    $format.LineAlignment = [Drawing.StringAlignment]::Center
    $outline.AddArc(40, 40, 224, 224, 180, 90)
    $outline.AddArc(760, 40, 224, 224, 270, 90)
    $outline.AddArc(760, 760, 224, 224, 0, 90)
    $outline.AddArc(40, 760, 224, 224, 90, 90)
    $outline.CloseFigure()
    $graphics.FillPath($background, $outline)
    $graphics.DrawString('S3', $smallFont, $accent, [Drawing.RectangleF]::new(80, 105, 864, 240), $format)
    $graphics.DrawString('3D', $largeFont, $foreground, [Drawing.RectangleF]::new(45, 280, 934, 610), $format)
    foreach ($name in $names) {
        $bitmap.Save((Join-Path $directory "$name.png"), [Drawing.Imaging.ImageFormat]::Png)
        $frames = @()
        foreach ($size in @(16, 24, 32, 48, 64, 128, 256)) {
            $frame = New-Object Drawing.Bitmap($size, $size)
            $painter = [Drawing.Graphics]::FromImage($frame)
            $stream = New-Object IO.MemoryStream
            try {
                $painter.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
                $painter.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $painter.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $painter.DrawImage($bitmap, 0, 0, $size, $size)
                $frame.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
                $frames += [pscustomobject]@{Size=$size; Bytes=$stream.ToArray()}
            } finally { $stream.Dispose(); $painter.Dispose(); $frame.Dispose() }
        }
        $output = [IO.File]::Create((Join-Path $directory "$name.ico"))
        $writer = New-Object IO.BinaryWriter($output)
        try {
            $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$frames.Count)
            $offset = 6 + 16 * $frames.Count
            foreach ($frame in $frames) {
                $dimension = if ($frame.Size -eq 256) { 0 } else { $frame.Size }
                $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
                $writer.Write([byte]0); $writer.Write([byte]0)
                $writer.Write([uint16]1); $writer.Write([uint16]32)
                $writer.Write([uint32]$frame.Bytes.Length); $writer.Write([uint32]$offset)
                $offset += $frame.Bytes.Length
            }
            foreach ($frame in $frames) { $writer.Write([byte[]]$frame.Bytes) }
        } finally { $writer.Dispose(); $output.Dispose() }
    }
} finally {
    $format.Dispose(); $largeFont.Dispose(); $smallFont.Dispose()
    $foreground.Dispose(); $accent.Dispose(); $background.Dispose()
    $outline.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
}
Get-ChildItem -LiteralPath $directory -File | Select-Object Name, Length
