[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$iconDirectory = Join-Path $repoRoot 'public\icons'
New-Item -ItemType Directory -Force -Path $iconDirectory | Out-Null

Add-Type -AssemblyName System.Drawing

foreach ($size in @(180, 192, 512)) {
    $bitmap = [System.Drawing.Bitmap]::new($size, $size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit

    try {
        $background = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#102a43'))
        $accent = [System.Drawing.Pen]::new([System.Drawing.ColorTranslator]::FromHtml('#2fba8b'), [single]($size * 0.035))
        $white = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::White)
        $dot = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#2fba8b'))
        $font = [System.Drawing.Font]::new('Segoe UI', [single]($size * 0.42), [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
        $format = [System.Drawing.StringFormat]::new()
        $format.Alignment = [System.Drawing.StringAlignment]::Center
        $format.LineAlignment = [System.Drawing.StringAlignment]::Center

        $graphics.FillRectangle($background, 0, 0, $size, $size)
        $inset = [single]($size * 0.2)
        $diameter = [single]($size - (2 * $inset))
        $graphics.DrawEllipse($accent, $inset, $inset, $diameter, $diameter)
        $graphics.DrawString('V', $font, $white, [System.Drawing.RectangleF]::new(0, [single](-$size * 0.035), $size, $size), $format)
        $dotSize = [single]($size * 0.075)
        $graphics.FillEllipse($dot, [single]($size * 0.69), [single]($size * 0.69), $dotSize, $dotSize)

        $output = Join-Path $iconDirectory "icon-$size.png"
        $bitmap.Save($output, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        foreach ($resource in @($format, $font, $dot, $white, $accent, $background, $graphics, $bitmap)) {
            if ($null -ne $resource) {
                $resource.Dispose()
            }
        }
    }
}

Write-Host "Generated VibePing icons in $iconDirectory"
