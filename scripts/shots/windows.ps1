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
# LIBCLANG_PATH, the bae-ffmpeg dist on FFMPEG_DIR and its bin on PATH, the msys2
# mingw runtime on PATH, the .NET 8 SDK, MSBuild, and makepri on PATH.
# bae-windows cannot build on any other host.

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
#    copied in; the bridge's ffmpeg / mingw dependencies resolve from PATH,
#    provisioned as above.
$bridgeDir = Join-Path $repoRoot 'target-windows\x86_64-pc-windows-msvc\debug'
foreach ($dll in @('bae_bridge.dll', 'uniffi_bae_bridge.dll')) {
    $src = Join-Path $bridgeDir $dll
    if (-not (Test-Path $src)) { throw "bridge DLL not found: $src" }
    Copy-Item $src $exeDir -Force
}

# 4b. Ensure the module resource index sits beside the exe. ResourceLoader
#     (Loc.cs) loads the exe-named module PRI ($(AssemblyName).pri) from the
#     executable's directory — there is no separate resources.pri; one would
#     shadow the module PRI and break WinUI theme-resource lookups. The publish
#     layout can place the PRI in a different output folder than the launched
#     exe, which fails Loc with a FileNotFoundException; if it is missing, copy
#     the one built under bin next to the exe.
Write-Host "exe: $exe"
$priName = [IO.Path]::GetFileNameWithoutExtension($exe) + '.pri'
$exePri = Join-Path $exeDir $priName
if (Test-Path $exePri) {
    Write-Host "$($priName): already present beside exe"
} else {
    $pri = Get-ChildItem -Path (Join-Path $repoRoot 'bae-windows\bin') -Recurse -Filter $priName -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
    if (-not $pri) { throw "$priName not found in build output" }
    Write-Host "$($priName): missing beside exe; copying from $pri"
    Copy-Item $pri $exeDir -Force
}

# The module PRI must carry the app's own Core resource map (Loc constructs
# ResourceLoader(pri, "Core") and dies with ResourceMap Not Found otherwise).
# Verify up front with a makepri dump so a bad PRI fails here, with the dump's
# map inventory in the log, instead of inside the capture process.
$makepri = Get-ChildItem -Path @(
        'C:\Program Files (x86)\Windows Kits\10\bin',
        "$env:USERPROFILE\.nuget\packages\microsoft.windows.sdk.buildtools"
    ) -Recurse -Filter makepri.exe -ErrorAction SilentlyContinue |
    Select-Object -First 1 -ExpandProperty FullName
if ($makepri) {
    $dump = Join-Path ([IO.Path]::GetTempPath()) 'bae-pri-dump.xml'
    & $makepri dump /if $exePri /of $dump /o | Out-Null
    $coreCount = (Select-String -Path $dump -Pattern '/Core/' -SimpleMatch).Count
    Write-Host "PRI dump: $coreCount Core-map entries in $priName"
    if ($coreCount -eq 0) {
        Write-Host '--- resource maps present in the PRI ---'
        Select-String -Path $dump -Pattern '<ResourceMap|<ResourceMapSubtree' | Select-Object -First 40 | ForEach-Object { $_.Line.Trim() }
        Write-Host '--- pri.resfiles the build fed makepri ---'
        Get-ChildItem -Path (Join-Path $repoRoot 'bae-windows\obj') -Recurse -Filter 'pri.resfiles' -ErrorAction SilentlyContinue |
            ForEach-Object { Write-Host "== $($_.FullName)"; Get-Content $_.FullName | Select-Object -First 40 }
        throw "module PRI has no Core resource map"
    }
} else {
    Write-Host 'makepri not found; skipping PRI verification'
}

# 5. Capture one scene per process. A second RenderTargetBitmap in one process
#    wedges headless (the first render always succeeds, the second always hangs),
#    so each enabled scene gets its own exe run with its own bounded timeout. This
#    list is the source of truth for both the loop and the expected-PNG check;
#    keep it in sync with the enabled scenes in ShotCapture.Scenes.
$scenes = @('story-1-first-run', 'story-3-empty-library')
$log = Join-Path $OutputDir 'capture.log'
$missing = @()
foreach ($scene in $scenes) {
    if (Test-Path $log) { Remove-Item $log -Force }
    Write-Host "=== capturing scene: $scene ==="
    $proc = Start-Process -FilePath $exe `
        -ArgumentList @('--capture-shots', $OutputDir, '--capture-scene', $scene) -PassThru
    $exited = $proc.WaitForExit(90000)

    # The app has no visible stderr, so always surface its stage log for this
    # scene, whether it rendered, failed, or wedged.
    Write-Host "----- capture.log ($scene) -----"
    if (Test-Path $log) { Get-Content $log | ForEach-Object { Write-Host $_ } }
    else { Write-Host '(capture.log not found)' }
    Write-Host "----- end capture.log ($scene) -----"

    if (-not $exited) {
        $proc.Kill()
        Write-Host "scene '$scene' timed out"
    }

    $png = Join-Path $OutputDir "$scene@windows.png"
    if (-not (Test-Path $png)) { $missing += $scene }
}

# The loud check that no scene silently vanished.
if ($missing.Count -gt 0) { throw "missing captures: $($missing -join ', ')" }

Write-Host "captured $($scenes.Count) scenes to $OutputDir"
