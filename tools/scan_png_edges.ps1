param(
    [string]$Directory = (Join-Path $PSScriptRoot '..\build\indic-render-android16\player-viewport'),
    [int]$BorderPixels = 4,
    [int]$Threshold = 250,
    [int]$ClipInsetLeft = 33,
    [int]$ClipInsetRight = 33
)

Add-Type -AssemblyName System.Drawing

$files = @(Get-ChildItem -LiteralPath $Directory -Filter 'l*-row-*-prepared.png' -File)
$report = foreach ($file in $files) {
    if ($file.BaseName -notmatch '^l\d+-row-\d+-prepared$') { continue }

    $bitmap = [System.Drawing.Bitmap]::new($file.FullName)
    try {
        $counts = [ordered]@{ Left = 0; Right = 0; Top = 0; Bottom = 0 }
        $rightColumns = [System.Collections.Generic.HashSet[int]]::new()

        for ($y = 0; $y -lt $bitmap.Height; $y++) {
            for ($offset = 0; $offset -lt $BorderPixels; $offset++) {
                $leftX = $ClipInsetLeft + $offset
                $rightX = $bitmap.Width - 1 - $ClipInsetRight - $offset
                foreach ($x in @($leftX, $rightX)) {
                    $pixel = $bitmap.GetPixel($x, $y)
                    if ($pixel.A -gt 0 -and ($pixel.R -lt $Threshold -or $pixel.G -lt $Threshold -or $pixel.B -lt $Threshold)) {
                        if ($x -ge $ClipInsetLeft -and $x -lt $ClipInsetLeft + $BorderPixels) {
                            $counts.Left++
                        }
                        if ($x -le $bitmap.Width - 1 - $ClipInsetRight -and $x -gt $bitmap.Width - 1 - $ClipInsetRight - $BorderPixels) {
                            $counts.Right++
                            [void]$rightColumns.Add($x)
                        }
                    }
                }
            }
        }

        for ($x = 0; $x -lt $bitmap.Width; $x++) {
            for ($offset = 0; $offset -lt $BorderPixels; $offset++) {
                $bottomY = $bitmap.Height - 1 - $offset
                foreach ($y in @($offset, $bottomY)) {
                    $pixel = $bitmap.GetPixel($x, $y)
                    if ($pixel.A -gt 0 -and ($pixel.R -lt $Threshold -or $pixel.G -lt $Threshold -or $pixel.B -lt $Threshold)) {
                        if ($y -lt $BorderPixels) { $counts.Top++ }
                        if ($y -ge $bitmap.Height - $BorderPixels) { $counts.Bottom++ }
                    }
                }
            }
        }

        [pscustomobject]@{
            Image = $file.Name
            ClipLeft = $ClipInsetLeft
            ClipRight = $bitmap.Width - 1 - $ClipInsetRight
            LeftPixels = $counts.Left
            RightPixels = $counts.Right
            RightColumns = (($rightColumns | Sort-Object) -join ',')
            TopPixels = $counts.Top
            BottomPixels = $counts.Bottom
        }
    }
    finally {
        $bitmap.Dispose()
    }
}

$report | Export-Csv -LiteralPath (Join-Path $Directory 'edge-pixel-scan.csv') -NoTypeInformation -Encoding UTF8
$report | Where-Object { $_.LeftPixels -gt 0 -or $_.RightPixels -gt 0 -or $_.TopPixels -gt 0 -or $_.BottomPixels -gt 0 } |
    Format-Table -AutoSize

Write-Output "Full edge scan: $(Join-Path $Directory 'edge-pixel-scan.csv')"
Write-Output "Prepared viewport rows scanned: $($report.Count)"
