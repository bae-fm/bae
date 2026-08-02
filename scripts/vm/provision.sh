#!/usr/bin/env bash
# Provision the Windows dev VM over SSH after guest-bootstrap.ps1 has run.
# Usage: scripts/vm/provision.sh <vm-ip> [<git-branch>]
# Unattended; ~30-40 min (VS Build Tools dominates). Idempotent: every step
# is install-if-missing or overwrite.
set -euo pipefail

VM="${1:?usage: provision.sh <vm-ip> [branch]}"
BRANCH="${2:-main}"
SSH="ssh tom@${VM}"
FFMPEG_VERSION="$(grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+-bae[0-9]+' .github/workflows/windows.yml | head -1)"

echo "== toolchain (winget) =="
$SSH 'winget install --id Git.Git --source winget -e --accept-source-agreements --accept-package-agreements --silent & winget install --id Microsoft.DotNet.SDK.8 --source winget -e --accept-package-agreements --silent & winget install --id Rustlang.Rustup --source winget -e --accept-package-agreements --silent & winget install --id LLVM.LLVM --source winget -e --accept-package-agreements --silent & winget install --id Mozilla.sccache --source winget -e --accept-package-agreements --silent & winget install --id Microsoft.WinDbg --source winget -e --accept-package-agreements --silent & winget install --id Microsoft.WindowsAppRuntime.1.6 --source winget -e --accept-package-agreements --silent & winget install --id Microsoft.DotNet.DesktopRuntime.8 --source winget -e --accept-package-agreements --silent'

echo "== VS Build Tools (the long one) =="
$SSH 'winget install --id Microsoft.VisualStudio.2022.BuildTools --source winget -e --accept-package-agreements --silent --override "--quiet --wait --nocache --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --add Microsoft.VisualStudio.ComponentGroup.UWP.BuildTools"'

echo "== rust toolchain =="
$SSH '"%USERPROFILE%\.cargo\bin\rustup" toolchain install 1.95.0 --profile minimal && "%USERPROFILE%\.cargo\bin\rustup" default 1.95.0'

echo "== repo clone (branch: ${BRANCH}) =="
$SSH "\"%ProgramFiles%\\Git\\cmd\\git.exe\" clone --branch ${BRANCH} https://github.com/bae-fm/bae.git C:\\bae 2>nul || (cd /d C:\\bae && \"%ProgramFiles%\\Git\\cmd\\git.exe\" fetch origin ${BRANCH} && \"%ProgramFiles%\\Git\\cmd\\git.exe\" reset --hard FETCH_HEAD)"
$SSH 'cd /d C:\bae && "%ProgramFiles%\Git\cmd\git.exe" submodule update --init --recursive'

echo "== ffmpeg dist (${FFMPEG_VERSION}, self-contained) =="
$SSH "powershell -Command \"\$dist = 'C:\\bae\\bae-ffmpeg\\dist'; New-Item -ItemType Directory -Force \$dist | Out-Null; curl.exe -fsSL --retry 5 --retry-delay 3 https://github.com/bae-fm/bae-ffmpeg/releases/download/${FFMPEG_VERSION}/ffmpeg-windows-aarch64.zip -o \$env:TEMP\\ffmpeg.zip; Expand-Archive -Path \$env:TEMP\\ffmpeg.zip -DestinationPath \$dist -Force; Remove-Item \$env:TEMP\\ffmpeg.zip; New-Item -ItemType Directory -Force \$dist\\lib | Out-Null; Copy-Item \$dist\\bin\\*.lib \$dist\\lib\\ -Force\""

echo "== uniffi-bindgen-cs =="
$SSH '"%USERPROFILE%\.cargo\bin\cargo" install --git https://github.com/bae-fm/uniffi-bindgen-cs --branch uniffi-0.32-bae uniffi-bindgen-cs --locked'

echo "== vpk (Velopack CLI, version-locked to the app's Velopack package) =="
$SSH 'dotnet tool install --global vpk --version 1.2.0 || dotnet tool update --global vpk --version 1.2.0'

echo "== helper scripts + task harness =="
for f in hidden.vbs vmshot.ps1 bridge-build.cmd avalonia-build.cmd build-release.cmd run-app.cmd tidy.ps1 vmlog-start.cmd vmlog-stop.cmd; do
  scp -q "scripts/vm/guest/${f}" "tom@${VM}:C:/Users/tom/${f}"
done
$SSH 'schtasks /create /f /tn baeRun /tr "wscript.exe C:\Users\tom\hidden.vbs \"cmd /c C:\Users\tom\run-app.cmd > C:\Users\tom\run-app.log 2>&1\"" /sc once /st 23:59 /it & schtasks /create /f /tn vmShot /tr "wscript.exe C:\Users\tom\hidden.vbs \"powershell -ExecutionPolicy Bypass -File C:\Users\tom\vmshot.ps1\"" /sc once /st 23:59 /it'

echo "== done. next: avalonia-build.cmd, schtasks /run /tn baeRun =="
