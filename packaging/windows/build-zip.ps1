# Builds FrameCraft-<version>-windows-x64.zip into target\dist\.
# Needs: cargo (MSVC toolchain). FrameCraft uses the WebView2 runtime that
# Windows 10 and 11 have. Unzip anywhere and run FrameCraft.exe; the
# assets folder stays next to it.
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value

cargo build --release -p framecraft-app
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$name = "FrameCraft-$version-windows-x64"
$stage = Join-Path 'target\dist' $name
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item 'target\release\framecraft.exe' (Join-Path $stage 'FrameCraft.exe')
Copy-Item -Recurse 'assets' (Join-Path $stage 'assets')
Copy-Item 'LICENSE' (Join-Path $stage 'LICENSE.txt')
Copy-Item 'THIRD_PARTY.md' $stage

$zip = "$stage.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip
Write-Host "Built $zip"
