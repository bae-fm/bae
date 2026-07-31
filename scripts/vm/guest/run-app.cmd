rem Launch the Avalonia app avalonia-build.cmd produced, on the desktop session.
rem The bridge cdylib is not copied next to the exe, so its build directory goes
rem on PATH alongside the FFmpeg dist the bridge links: Windows resolves the
rem DllImport("uniffi_bae_bridge") from there.
set PATH=C:\bae\bae-ffmpeg\dist\bin;C:\bae\target-windows\aarch64-pc-windows-msvc\debug;%PATH%
"C:\bae\bae-avalonia\bin\Debug\net8.0-windows10.0.19041.0\bae-avalonia.exe"
