# Shrink the VM: remove non-dev preinstalls and flush OS caches, keeping
# everything a build touches (cargo registry, sccache, NuGet, target trees,
# toolchains). Idempotent; run over SSH after provisioning, or any time.
# Ends with a ReTrim so the freed blocks propagate to the host's sparse qcow2.

$ErrorActionPreference = 'Continue'

# Preinstalled consumer apps. Windows Terminal, Store, and Calculator stay.
$bloat = @(
  'Clipchamp.Clipchamp', 'Microsoft.BingNews', 'Microsoft.BingWeather',
  'Microsoft.BingSearch', 'Microsoft.GamingApp', 'Microsoft.Xbox*',
  'Microsoft.ZuneMusic', 'Microsoft.ZuneVideo',
  'Microsoft.MicrosoftSolitaireCollection', 'Microsoft.People',
  'Microsoft.Todos', 'Microsoft.OutlookForWindows', 'MSTeams',
  'Microsoft.MicrosoftOfficeHub', 'Microsoft.GetHelp', 'Microsoft.Getstarted',
  'Microsoft.WindowsFeedbackHub', 'Microsoft.549981C3F5F10', # Cortana
  'Microsoft.Copilot', 'MicrosoftCorporationII.QuickAssist',
  'Microsoft.YourPhone', 'Microsoft.WindowsSoundRecorder',
  'Microsoft.MicrosoftStickyNotes', 'Microsoft.PowerAutomateDesktop',
  'Microsoft.DevHome', 'Microsoft.OneDriveSync', 'Microsoft.WindowsMaps',
  'Microsoft.WindowsAlarms', 'MicrosoftCorporationII.MicrosoftFamily'
)
foreach ($name in $bloat) {
  Get-AppxPackage -AllUsers -Name $name | Remove-AppxPackage -AllUsers -ErrorAction SilentlyContinue
  Get-AppxProvisionedPackage -Online | Where-Object DisplayName -Like $name |
    Remove-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue | Out-Null
}

# OneDrive's desktop installer (separate from the Appx sync stub).
$od = "$env:SystemRoot\SysWOW64\OneDriveSetup.exe", "$env:SystemRoot\System32\OneDriveSetup.exe" |
  Where-Object { Test-Path $_ } | Select-Object -First 1
if ($od) { & $od /uninstall; Start-Sleep 5 }

# hiberfil.sys — no reason to hibernate a VM.
powercfg /h off

# ~7 GB Windows Update staging reservation.
DISM /Online /Set-ReservedStorageState /State:Disabled

# System Restore snapshots — the VM's rollback story is the qcow2 on the host.
Disable-ComputerRestore -Drive 'C:\' -ErrorAction SilentlyContinue
vssadmin delete shadows /all /quiet

# Windows Update + Delivery Optimization download caches.
net stop wuauserv
Remove-Item -Recurse -Force "$env:SystemRoot\SoftwareDistribution\Download\*" -ErrorAction SilentlyContinue
net start wuauserv
Delete-DeliveryOptimizationCache -Force -ErrorAction SilentlyContinue

# Superseded component-store payloads.
DISM /Online /Cleanup-Image /StartComponentCleanup

# Temp dirs and recycle bin. Build caches live elsewhere and are untouched.
Remove-Item -Recurse -Force "$env:TEMP\*" -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force "$env:SystemRoot\Temp\*" -ErrorAction SilentlyContinue
Clear-RecycleBin -Force -ErrorAction SilentlyContinue

# Tell the virtual disk which blocks are free so the host qcow2 stays sparse.
Optimize-Volume -DriveLetter C -ReTrim -Verbose

Get-PSDrive C | Select-Object Used, Free
