@echo off
rem Stop the ETW capture vmlog-start.cmd began and convert it to CSV at
rem C:\Users\tom\bae-trace.csv (tracerpt's dumpfile format).
logman stop baeTrace -ets || exit /b 1
tracerpt C:\Users\tom\bae-trace.etl -o C:\Users\tom\bae-trace.csv -of CSV -y || exit /b 1
echo trace: C:\Users\tom\bae-trace.csv
