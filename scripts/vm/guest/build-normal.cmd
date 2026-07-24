set PATH=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\MSBuild\Current\Bin;C:\Program Files\Git\bin;C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\arm64;%PATH%
cd /d C:\bae
msbuild bae-windows\bae-windows.csproj /restore /p:Configuration=Debug /p:Platform=x64 /p:BridgeBindingsDir=..\bae-bridge\csharp-bindings-baeium /v:m
copy /y C:\bae\target-windows\x86_64-pc-windows-msvc\debug\bae_bridge.dll C:\bae\bae-windows\bin\x64\Debug\net8.0-windows10.0.19041.0\ >nul
copy /y C:\bae\target-windows\x86_64-pc-windows-msvc\debug\uniffi_bae_bridge.dll C:\bae\bae-windows\bin\x64\Debug\net8.0-windows10.0.19041.0\ >nul
