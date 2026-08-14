# Generates assets/dsh-desktop.ico (multi-resolution) from scratch.
# Renders a rounded-square gradient tile with a white "D", matching the app's init screen logo.
Add-Type -AssemblyName System.Drawing

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$outIco = Join-Path $root "assets/dsh-desktop.ico"
$tmpDir = Join-Path $root "target/icon-tmp"
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$pngPaths = @()
foreach ($s in $sizes) {
  $bmp = [System.Drawing.Bitmap]::new($s, $s)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit

  $r = [float]$s * 0.22
  $d = $r * 2
  $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
  $path.AddArc(0, 0, $d, $d, 180, 90)
  $path.AddArc($s - $d, 0, $d, $d, 270, 90)
  $path.AddArc($s - $d, $s - $d, $d, $d, 0, 90)
  $path.AddArc(0, $s - $d, $d, $d, 90, 90)
  $path.CloseFigure()

  $rect = [System.Drawing.RectangleF]::new(0, 0, [float]$s, [float]$s)
  $c1 = [System.Drawing.Color]::FromArgb(255, 0x4d, 0x9f, 0xff)
  $c2 = [System.Drawing.Color]::FromArgb(255, 0x7c, 0x5c, 0xff)
  $brush = [System.Drawing.Drawing2D.LinearGradientBrush]::new($rect, $c1, $c2, 45)
  $g.FillPath($brush, $path)

  $font = [System.Drawing.Font]::new("Segoe UI", [float]$s * 0.56, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
  $sf = [System.Drawing.StringFormat]::new()
  $sf.Alignment = [System.Drawing.StringAlignment]::Center
  $sf.LineAlignment = [System.Drawing.StringAlignment]::Center
  $g.DrawString("D", $font, [System.Drawing.Brushes]::White, $rect, $sf)

  $tmp = Join-Path $tmpDir "icon-$s.png"
  $bmp.Save($tmp, [System.Drawing.Imaging.ImageFormat]::Png)
  $pngPaths += ,@($s, $tmp)

  $font.Dispose(); $sf.Dispose(); $brush.Dispose(); $path.Dispose(); $g.Dispose(); $bmp.Dispose()
}

# Pack the PNGs into a single .ico (PNG-compressed entries, Vista+).
$pngBytes = @()
foreach ($e in $pngPaths) { $pngBytes += ,[System.IO.File]::ReadAllBytes($e[1]) }
$count = $pngPaths.Count
$ms = [System.IO.MemoryStream]::new()
$bw = [System.IO.BinaryWriter]::new($ms)
$bw.Write([UInt16]0); $bw.Write([UInt16]1); $bw.Write([UInt16]$count)
$offset = 6 + 16 * $count
for ($i = 0; $i -lt $count; $i++) {
  $s = $pngPaths[$i][0]; $data = $pngBytes[$i]
  $wh = if ($s -ge 256) { 0 } else { $s }
  $bw.Write([Byte]$wh); $bw.Write([Byte]$wh); $bw.Write([Byte]0); $bw.Write([Byte]0)
  $bw.Write([UInt16]1); $bw.Write([UInt16]32)
  $bw.Write([UInt32]$data.Length); $bw.Write([UInt32]$offset)
  $offset += $data.Length
}
foreach ($d in $pngBytes) { $bw.Write($d) }
$bw.Flush()
[System.IO.File]::WriteAllBytes($outIco, $ms.ToArray())
$bw.Dispose(); $ms.Dispose()
Write-Output "wrote $outIco ($((Get-Item $outIco).Length) bytes)"
