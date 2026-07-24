@echo off
rem Start an ETW capture of both bae providers — the Windows equivalent of
rem `log stream` on macOS. Providers are named by GUID: logman's "*name" star
rem syntax fails for unregistered TraceLogging providers, so these are the
rem standard name-hash GUIDs for "bae-core" and "bae-app" (the same derivation
rem .NET EventSource and tracing-etw use). Explicit keyword/level masks —
rem logman's defaults match no TraceLogging events. Stop with vmlog-stop.cmd.
logman start baeTrace -p "{4dbe2db1-549f-5bfb-b9bd-8724538df9ba}" 0xffffffffffffffff 0xff -o C:\Users\tom\bae-trace.etl -ets || exit /b 1
logman update baeTrace -p "{73def730-ddb7-5e9e-e396-968edfd85140}" 0xffffffffffffffff 0xff -ets || exit /b 1
