rem Build the Avalonia desktop app: full-edition bridge bindings, then the C#
rem app. bridge-build leaves vcvars + the pinned MSVC linker in this process,
rem which the csproj's host loc-gen cargo step needs (vcvars alone does not put
rem link.exe on PATH on this VM).
call C:\Users\tom\bridge-build.cmd full || exit /b 1
cd /d C:\bae\bae-avalonia
dotnet build bae-avalonia.csproj -c Debug
exit /b %ERRORLEVEL%
