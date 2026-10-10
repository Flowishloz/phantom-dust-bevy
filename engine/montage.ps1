# Combine screenshots into one labelled grid (cheap to inspect, easy to send).
#   ./montage.ps1 -Out shots\sheet.png -Columns 3 -Images shots\a.png,shots\b.png -Labels A,B
# Labels default to A, B, C, ... Each cell is 640x360.
param(
    [Parameter(Mandatory)][string]$Out,
    [Parameter(Mandatory)][string[]]$Images,
    [string[]]$Labels,
    [int]$Columns = 3,
    [int]$CellWidth = 640,
    [int]$CellHeight = 360,
    [int]$FontSize = 28
)
Add-Type -AssemblyName System.Drawing
if (-not $Labels) { $Labels = 0..($Images.Count - 1) | ForEach-Object { [string][char](65 + $_) } }
$rows = [math]::Ceiling($Images.Count / $Columns)
$bmp = New-Object Drawing.Bitmap ($CellWidth * [math]::Min($Columns, $Images.Count)), ($CellHeight * $rows)
$g = [Drawing.Graphics]::FromImage($bmp)
$g.InterpolationMode = 'HighQualityBicubic'
$font = New-Object Drawing.Font 'Arial', $FontSize, ([Drawing.FontStyle]::Bold)
for ($i = 0; $i -lt $Images.Count; $i++) {
    $x = ($i % $Columns) * $CellWidth; $y = [math]::Floor($i / $Columns) * $CellHeight
    $im = [Drawing.Image]::FromFile((Resolve-Path $Images[$i]).Path)
    $g.DrawImage($im, $x + 2, $y + 2, $CellWidth - 4, $CellHeight - 4)
    $g.DrawString($Labels[$i], $font, [Drawing.Brushes]::Yellow, $x + 10, $y + 6)
    $im.Dispose()
}
$bmp.Save((Join-Path (Get-Location) $Out))
"montage -> $Out"
