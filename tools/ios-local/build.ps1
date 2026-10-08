$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot 'build.sh'
$linuxPath = & wsl.exe --exec wslpath -a $scriptPath
if ($LASTEXITCODE -ne 0) { throw 'WSL must be running to build the iOS sample locally.' }
& wsl.exe --exec bash $linuxPath.Trim()
if ($LASTEXITCODE -ne 0) { throw "Local iOS build failed ($LASTEXITCODE). See build/ios-local/build.log." }
