set PATH=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin;C:\Program Files\Git\bin;C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\arm64;%USERPROFILE%\.cargo\bin;%PATH%
rem The csproj's loc-gen target runs cargo; pin the MSVC linker and provide its
rem LIB/INCLUDE env, or a bare "link.exe" resolves to Git's GNU coreutils link
rem (see bridge-build.cmd).
for /f "delims=" %%d in ('dir /b /ad "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC"') do set MSVC_VER=%%d
set CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\%MSVC_VER%\bin\Hostarm64\arm64\link.exe
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" arm64 || exit /b 1
cd /d C:\bae
msbuild bae-windows\bae-windows.csproj /restore /p:Configuration=Debug /p:Platform=ARM64 /p:BridgeBindingsDir=..\bae-bridge\csharp-bindings-baeium /v:m || exit /b 1
copy /y C:\bae\target-windows\aarch64-pc-windows-msvc\debug\uniffi_bae_bridge.dll C:\bae\bae-windows\bin\ARM64\Debug\net8.0-windows10.0.19041.0\ >nul || exit /b 1
