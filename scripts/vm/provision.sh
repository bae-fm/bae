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
$SSH 'winget install --id Git.Git -e --accept-source-agreements --accept-package-agreements --silent & winget install --id Microsoft.DotNet.SDK.8 -e --accept-package-agreements --silent & winget install --id Rustlang.Rustup -e --accept-package-agreements --silent & winget install --id LLVM.LLVM -e --accept-package-agreements --silent & winget install --id Mozilla.sccache -e --accept-package-agreements --silent & winget install --id Microsoft.WinDbg -e --accept-package-agreements --silent & winget install --id Microsoft.WindowsAppRuntime.1.6 -e --accept-package-agreements --silent & winget install --id Microsoft.DotNet.DesktopRuntime.8 --architecture x64 -e --accept-package-agreements --silent'

echo "== VS Build Tools (the long one) =="
$SSH 'winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --silent --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --add Microsoft.VisualStudio.ComponentGroup.UWP.BuildTools"'

echo "== rust toolchain =="
$SSH '"%USERPROFILE%\.cargo\bin\rustup" toolchain install 1.95.0 --profile minimal && "%USERPROFILE%\.cargo\bin\rustup" target add x86_64-pc-windows-msvc --toolchain 1.95.0 && "%USERPROFILE%\.cargo\bin\rustup" default 1.95.0'

echo "== repo clone (branch: ${BRANCH}) =="
$SSH "\"%ProgramFiles%\\Git\\cmd\\git.exe\" clone --branch ${BRANCH} https://github.com/bae-fm/bae.git C:\\bae 2>nul || (cd /d C:\\bae && \"%ProgramFiles%\\Git\\cmd\\git.exe\" fetch origin ${BRANCH} && \"%ProgramFiles%\\Git\\cmd\\git.exe\" reset --hard FETCH_HEAD)"
$SSH 'cd /d C:\bae && "%ProgramFiles%\Git\cmd\git.exe" submodule update --init --recursive'

echo "== ffmpeg dist (${FFMPEG_VERSION}, self-contained) =="
$SSH "powershell -Command \"\$dist = 'C:\\bae\\bae-ffmpeg\\dist'; New-Item -ItemType Directory -Force \$dist | Out-Null; curl.exe -L https://github.com/bae-fm/bae-ffmpeg/releases/download/${FFMPEG_VERSION}/ffmpeg-windows-x86_64.zip -o \$env:TEMP\\ffmpeg.zip; Expand-Archive -Path \$env:TEMP\\ffmpeg.zip -DestinationPath \$dist -Force; New-Item -ItemType Directory -Force \$dist\\lib | Out-Null; Copy-Item \$dist\\bin\\*.lib \$dist\\lib\\ -Force\""

echo "== uniffi-bindgen-cs =="
$SSH '"%USERPROFILE%\.cargo\bin\cargo" install --git https://github.com/NordSecurity/uniffi-bindgen-cs.git --tag "v0.11.0+v0.31.0" uniffi-bindgen-cs --locked'

echo "== helper scripts + task harness =="
for f in hidden.vbs vmshot.ps1 bridge-build.cmd build-normal.cmd run-app.cmd; do
  scp -q "scripts/vm/guest/${f}" "tom@${VM}:C:/Users/tom/${f}"
done
$SSH 'schtasks /create /f /tn baeRun /tr "wscript.exe C:\Users\tom\hidden.vbs \"cmd /c C:\Users\tom\run-app.cmd > C:\Users\tom\run-app.log 2>&1\"" /sc once /st 23:59 /it & schtasks /create /f /tn vmShot /tr "wscript.exe C:\Users\tom\hidden.vbs \"powershell -ExecutionPolicy Bypass -File C:\Users\tom\vmshot.ps1\"" /sc once /st 23:59 /it'

echo "== done. next: bridge-build.cmd, build-normal.cmd, schtasks /run /tn baeRun =="
