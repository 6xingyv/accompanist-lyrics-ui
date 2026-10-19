param(
    [Parameter(Mandatory = $true)][string]$RepositoryRoot,
    [Parameter(Mandatory = $true)][string]$JavaHome,
    [Parameter(Mandatory = $true)][string]$AppImage,
    [Parameter(Mandatory = $true)][string]$InstallerDirectory,
    [Parameter(Mandatory = $true)][string]$ScratchDirectory,
    [Parameter(Mandatory = $true)][string]$WixDirectory,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$IconFile,
    [Parameter(Mandatory = $true)][string]$UpgradeUuid
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$jpackage = Join-Path $JavaHome 'bin\jpackage.exe'
$jmod = Join-Path $JavaHome 'jmods\jdk.jpackage.jmod'
$jmodTool = Join-Path $JavaHome 'bin\jmod.exe'
$candle = Join-Path $WixDirectory 'candle.exe'
$light = Join-Path $WixDirectory 'light.exe'
$repositoryPath = [IO.Path]::GetFullPath($RepositoryRoot).TrimEnd('\') + '\'
$scratchPath = [IO.Path]::GetFullPath($ScratchDirectory)
if (-not $scratchPath.StartsWith($repositoryPath, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Installer scratch directory must stay inside the repository: $scratchPath"
}

foreach ($path in @($jpackage, $jmod, $jmodTool, $candle, $light, $AppImage, $IconFile)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Windows installer input not found: $path"
    }
}

$runId = [Guid]::NewGuid().ToString('N')
$runDirectory = Join-Path $ScratchDirectory $runId
$extractedJmod = Join-Path $runDirectory 'jdk-jpackage'
$resourceDirectory = Join-Path $runDirectory 'resources'
$stagedInstallerDirectory = Join-Path $runDirectory 'installer'
New-Item -ItemType Directory -Path $extractedJmod, $resourceDirectory, $stagedInstallerDirectory -Force | Out-Null

& $jmodTool extract --dir $extractedJmod $jmod
if ($LASTEXITCODE -ne 0) {
    throw "Could not extract jpackage's WiX template from $jmod"
}

$defaultWxs = Join-Path $extractedJmod 'classes\jdk\jpackage\internal\resources\main.wxs'
$wxs = [IO.File]::ReadAllText($defaultWxs)

function Replace-ExactlyOnce([string]$text, [string]$pattern, [string]$replacement) {
    $count = [regex]::Matches($text, $pattern).Count
    if ($count -ne 1) {
        throw "Expected one match for '$pattern' in jpackage's main.wxs, found $count."
    }
    return [regex]::Replace($text, $pattern, $replacement, 1)
}

# Treat an already installed package with the same ProductVersion as an upgrade.
# Strictly newer installations remain protected by the second UpgradeVersion row.
$productCode = [Guid]::NewGuid().ToString('B').ToUpperInvariant()
$wxs = Replace-ExactlyOnce $wxs 'Id="\$\(var\.JpProductCode\)"' "Id=`"$productCode`""
$wxs = Replace-ExactlyOnce $wxs 'IncludeMaximum="\$\(var\.JpUpgradeVersionOnlyDetectUpgrade\)"' 'IncludeMaximum="yes"'
$wxs = Replace-ExactlyOnce $wxs 'IncludeMinimum="\$\(var\.JpUpgradeVersionOnlyDetectDowngrade\)"' 'IncludeMinimum="no"'
[IO.File]::WriteAllText((Join-Path $resourceDirectory 'main.wxs'), $wxs, [Text.UTF8Encoding]::new($false))
[IO.File]::WriteAllText(
    (Join-Path $resourceDirectory 'overrides.wxi'),
    "<?xml version=`"1.0`" encoding=`"utf-8`"?>`n<Include>`n  <?undef JpAllowDowngrades ?>`n</Include>`n",
    [Text.UTF8Encoding]::new($false)
)

$env:PATH = "$WixDirectory;$env:PATH"
$arguments = @(
    '--type', 'exe',
    '--name', 'Accompanist',
    '--app-image', $AppImage,
    '--dest', $stagedInstallerDirectory,
    '--resource-dir', $resourceDirectory,
    '--app-version', $Version,
    '--vendor', 'MochaRealm',
    '--description', 'Lyrics following your music player',
    '--icon', $IconFile,
    '--win-per-user-install',
    '--win-menu',
    '--win-menu-group', 'Accompanist',
    '--win-shortcut',
    '--win-upgrade-uuid', $UpgradeUuid,
    '--verbose'
)

& $jpackage @arguments
if ($LASTEXITCODE -ne 0) {
    throw "jpackage failed while creating the Windows upgrade installer (exit code $LASTEXITCODE)."
}

$stagedInstaller = Join-Path $stagedInstallerDirectory "Accompanist-$Version.exe"
$targetInstaller = Join-Path $InstallerDirectory "Accompanist-$Version.exe"
if (-not (Test-Path -LiteralPath $stagedInstaller)) {
    throw "jpackage did not produce the expected installer: $stagedInstaller"
}
New-Item -ItemType Directory -Path $InstallerDirectory -Force | Out-Null
Copy-Item -LiteralPath $stagedInstaller -Destination $targetInstaller -Force
$runPath = [IO.Path]::GetFullPath($runDirectory)
$scratchRoot = [IO.Path]::GetFullPath($ScratchDirectory).TrimEnd('\') + '\'
if (-not $runPath.StartsWith($scratchRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Installer work directory escaped its designated scratch directory: $runPath"
}
try {
    Remove-Item -LiteralPath $runPath -Recurse -Force -ErrorAction Stop
} catch {
    Write-Warning "Could not remove temporary installer work files at ${runPath}: $_"
}
Write-Host "Replaced $targetInstaller with an installer that supports same-version upgrades."
Write-Host "ProductCode for this package: $productCode"
