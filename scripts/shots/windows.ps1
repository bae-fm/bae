#!/usr/bin/env pwsh
#
# Capture the Windows preview scenes as flat PNGs for the cross-platform
# screenshot gallery. Part of the shared per-platform contract: invoked from the
# repo root with one argument — the output directory — it builds the app, renders
# every scene to <dir>\<scene>@windows.png, and exits non-zero if the build, the
# capture run, or any expected PNG is missing.
#
# The capture itself lives in the app (DEBUG-only `--capture-shots <dir>` mode,
# ShotCapture.cs): it renders each scene through RenderTargetBitmap and exits
# 0/1. This script builds that app and drives it.
#
# Runner prerequisites (provisioned by the calling workflow, the same way
# .github/workflows/windows.yml provisions the Windows build job): the pinned
# Rust toolchain with the x86_64-pc-windows-msvc target, uniffi-bindgen-cs,
# LIBCLANG_PATH, the bae-ffmpeg dist on FFMPEG_DIR and its bin on PATH, libdiscid
# on LIB and PATH, the msys2 mingw runtime on PATH, the .NET 8 SDK, MSBuild, and
# makepri on PATH. bae-windows cannot build on any other host.

param(
    [Parameter(Mandatory = $true)]
    [string]$OutputDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Repo root is two levels up from scripts/shots/.
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $repoRoot

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$OutputDir = (Resolve-Path $OutputDir).Path

# 1. Build the native Windows bridge (baeium / desktop edition). Produces the
#    bae_bridge.dll / uniffi_bae_bridge.dll the app loads and the generated C#
#    bindings it compiles against — the same command windows.yml runs.
$env:BAE_BRIDGE_FEATURES = 'desktop'
$env:BAE_BRIDGE_CSHARP_BINDINGS_DIR = 'bae-bridge/csharp-bindings-baeium'
bash ./bae-bridge/build-windows.sh
if ($LASTEXITCODE -ne 0) { throw "bridge build failed ($LASTEXITCODE)" }

# 2. Build the WinUI app. Debug so the DEBUG-only --capture-shots mode compiles
#    in; the functional flags (Platform, EnforceCodeStyleInBuild, bindings dir)
#    match windows.yml. The self-contained flags are the addition that lets the
#    unpackaged app *run* on a runner with no machine-wide Windows App Runtime:
#    the .NET and Windows App SDK runtimes are emitted next to the exe.
msbuild bae-windows\bae-windows.csproj /restore `
    /p:Configuration=Debug /p:Platform=x64 `
    /p:RuntimeIdentifier=win-x64 /p:SelfContained=true /p:WindowsAppSDKSelfContained=true `
    /p:EnforceCodeStyleInBuild=true `
    /p:BridgeBindingsDir=..\bae-bridge\csharp-bindings-baeium
if ($LASTEXITCODE -ne 0) { throw "app build failed ($LASTEXITCODE)" }

# 3. Locate the built exe (the exact output path depends on the RID subfolder).
$exe = Get-ChildItem -Path (Join-Path $repoRoot 'bae-windows\bin') -Recurse -Filter 'bae-windows.exe' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $exe) { throw 'bae-windows.exe not found after build' }
$exeDir = Split-Path $exe

# 4. Place the native bridge DLLs next to the exe. They are resolved from the app
#    directory (the loader does not search target-windows), so the run needs them
#    copied in; the bridge's ffmpeg / libdiscid / mingw dependencies resolve from
#    PATH, provisioned as above.
$bridgeDir = Join-Path $repoRoot 'target-windows\x86_64-pc-windows-msvc\debug'
foreach ($dll in @('bae_bridge.dll', 'uniffi_bae_bridge.dll')) {
    $src = Join-Path $bridgeDir $dll
    if (-not (Test-Path $src)) { throw "bridge DLL not found: $src" }
    Copy-Item $src $exeDir -Force
}

# 5. Run the capture. It renders every scene and exits 0 (all) or 1 (any failed).
#    Bound the run so a render hang fails the job rather than blocking forever.
$proc = Start-Process -FilePath $exe -ArgumentList @('--capture-shots', $OutputDir) -PassThru
if (-not $proc.WaitForExit(180000)) {
    $proc.Kill()
    throw 'capture run timed out'
}
if ($proc.ExitCode -ne 0) { throw "capture run exited $($proc.ExitCode)" }

# 6. Verify the expected PNGs exist — the loud check that a scene did not silently
#    vanish. Keep this list in sync with ShotCapture.Scenes.
$expected = @('welcome', 'album-detail')
$missing = @()
foreach ($scene in $expected) {
    $png = Join-Path $OutputDir "$scene@windows.png"
    if (-not (Test-Path $png)) { $missing += $png }
}
if ($missing.Count -gt 0) { throw "missing captures: $($missing -join ', ')" }

Write-Host "captured $($expected.Count) scenes to $OutputDir"
