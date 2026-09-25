rem Build the Avalonia skeleton: bridge bindings, then the C# app.
call C:\Users\tom\bridge-build.cmd || exit /b 1
cd /d C:\bae\bae-avalonia
dotnet build bae-avalonia.csproj -c Debug
exit /b %ERRORLEVEL%
