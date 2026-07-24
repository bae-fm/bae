set FFMPEG_DIR=C:\bae\bae-ffmpeg\dist
set BINDGEN_EXTRA_CLANG_ARGS=-IC:\bae\bae-ffmpeg\dist\include
set LIBCLANG_PATH=C:\Program Files\LLVM\bin
set PATH=C:\bae\bae-ffmpeg\dist\bin;C:\Program Files\LLVM\bin;%USERPROFILE%\.cargo\bin;%PATH%
set BAE_BRIDGE_FEATURES=desktop
set BAE_BRIDGE_CSHARP_BINDINGS_DIR=bae-bridge/csharp-bindings-baeium
cd /d C:\bae
"%ProgramFiles%\Git\bin\bash.exe" ./bae-bridge/build-windows.sh
