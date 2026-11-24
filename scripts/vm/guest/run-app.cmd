rem Launch the Avalonia app avalonia-build.cmd produced, on the desktop session.
rem The bridge cdylib is not copied next to the exe, so its build directory goes
rem on PATH alongside the FFmpeg and libvips dists the bridge links: Windows
rem resolves the DllImport("uniffi_bae_bridge") from there.
set PATH=C:\bae\bae-ffmpeg\dist\bin;C:\bae\bae-libvips\dist\bin;C:\bae\target-windows\aarch64-pc-windows-msvc\debug;%PATH%
rem The arm64 Platform segment comes from building on the arm64 VM host.
"C:\bae\bae-avalonia\bin\arm64\Debug\net8.0\bae-avalonia.exe"
