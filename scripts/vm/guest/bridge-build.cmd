rem Build the Rust bridge and generate C# bindings for one Windows edition:
rem   bridge-build.cmd            -> baeium (bae-windows: features "desktop")
rem   bridge-build.cmd full       -> full (bae-avalonia: "oauth-providers,desktop")
rem Leaves the MSVC/vcvars environment set in the calling cmd process, so a
rem wrapper that `call`s this can run further cargo/dotnet steps in it.
set FFMPEG_DIR=C:\bae\bae-ffmpeg\dist
set BINDGEN_EXTRA_CLANG_ARGS=-IC:\bae\bae-ffmpeg\dist\include
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
set PATH=C:\bae\bae-ffmpeg\dist\bin;C:\Program Files\LLVM\bin;%USERPROFILE%\.cargo\bin;%PATH%
set BAE_BRIDGE_TARGET=aarch64-pc-windows-msvc
if "%~1"=="full" (
    set BAE_BRIDGE_FEATURES=oauth-providers,desktop
    set BAE_BRIDGE_CSHARP_BINDINGS_DIR=bae-bridge/csharp-bindings-full
) else (
    set BAE_BRIDGE_FEATURES=desktop
    set BAE_BRIDGE_CSHARP_BINDINGS_DIR=bae-bridge/csharp-bindings-baeium
)
rem Pin the MSVC linker explicitly: the build runs under Git bash, whose
rem /usr/bin precedes the inherited PATH, so a bare "link.exe" can resolve to
rem GNU coreutils' link and fail host proc-macro links.
for /f "delims=" %%d in ('dir /b /ad "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC"') do set MSVC_VER=%%d
set CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\%MSVC_VER%\bin\Hostarm64\arm64\link.exe
rem vcvars provides LIB/INCLUDE for the pinned linker (rustc skips its own
rem MSVC env injection when the linker is overridden).
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" arm64 || exit /b 1
cd /d C:\bae
"%ProgramFiles%\Git\bin\bash.exe" ./bae-bridge/build-windows.sh || exit /b 1
