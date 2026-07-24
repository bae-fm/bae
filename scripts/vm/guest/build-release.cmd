@echo off
rem Release build + local Velopack pack (baeium edition) — the build steps of
rem release-windows.yml without publishing or signing. Output:
rem C:\bae\velopack-local\*Setup.exe; install that to exercise the installer
rem path. Channel "local" keeps an installed test build off the public GitHub
rem update feeds. Version is 0.0.<commit count> so successive local packs
rem are monotonically newer.
setlocal
set FFMPEG_DIR=C:\bae\bae-ffmpeg\dist
set BINDGEN_EXTRA_CLANG_ARGS=-IC:\bae\bae-ffmpeg\dist\include
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
set PATH=C:\bae\bae-ffmpeg\dist\bin;C:\Program Files\LLVM\bin;%USERPROFILE%\.cargo\bin;C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin;C:\Program Files\Git\bin;C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\arm64;%USERPROFILE%\.dotnet\tools;%PATH%
set BAE_BRIDGE_TARGET=aarch64-pc-windows-msvc
set BAE_BRIDGE_FEATURES=desktop
set BAE_BRIDGE_CSHARP_BINDINGS_DIR=bae-bridge/csharp-bindings-baeium
cd /d C:\bae

"%ProgramFiles%\Git\bin\bash.exe" ./bae-bridge/build-windows.sh --release || exit /b 1

for /f %%c in ('"%ProgramFiles%\Git\cmd\git.exe" rev-list --count HEAD') do set BUILD_NUM=%%c

msbuild bae-windows\bae-windows.csproj /restore /t:Publish /p:Configuration=Release /p:Platform=ARM64 /p:SelfContained=true /p:RuntimeIdentifier=win-arm64 /p:PublishDir=publish\ /p:Version=0.0.%BUILD_NUM% /p:BridgeBindingsDir=..\bae-bridge\csharp-bindings-baeium /v:m || exit /b 1

set STAGE=C:\bae\stage-local
if exist %STAGE% rmdir /s /q %STAGE%
mkdir %STAGE%
xcopy /e /y /q C:\bae\bae-windows\publish %STAGE%\ >nul || exit /b 1
copy /y C:\bae\target-windows\aarch64-pc-windows-msvc\release\bae_bridge.dll %STAGE%\ >nul || exit /b 1
copy /y C:\bae\target-windows\aarch64-pc-windows-msvc\release\uniffi_bae_bridge.dll %STAGE%\ >nul || exit /b 1
copy /y C:\bae\bae-ffmpeg\dist\bin\*.dll %STAGE%\ >nul || exit /b 1
ren %STAGE%\bae-windows.exe baeium.exe || exit /b 1

vpk pack --packId baeium --packVersion 0.0.%BUILD_NUM% --packDir %STAGE% --mainExe baeium.exe --channel local --packTitle baeium --outputDir C:\bae\velopack-local || exit /b 1
dir C:\bae\velopack-local
