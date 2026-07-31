rem Build the Avalonia desktop app: bridge bindings, then the C# app.
rem bridge-build leaves vcvars + the pinned MSVC linker in this process, which
rem the csproj's host loc-gen cargo step needs (vcvars alone does not put
rem link.exe on PATH on this VM).
call C:\Users\tom\bridge-build.cmd || exit /b 1
cd /d C:\bae\bae-avalonia
dotnet build bae-avalonia.csproj -c Debug
exit /b %ERRORLEVEL%
