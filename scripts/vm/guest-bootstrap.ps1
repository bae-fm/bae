# Paste into an elevated PowerShell inside the fresh Windows VM.
# Enables SSH with the host's key, opens the firewall, unhides the
# auto-logon checkbox, and prints the VM's address.

Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service sshd -StartupType Automatic
Start-Service sshd
Set-NetConnectionProfile -NetworkCategory Private
New-NetFirewallRule -Name sshd-in -DisplayName 'OpenSSH inbound' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22

# Host public key -> administrators key file (replace if the host key rotates).
$k = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEGTHFGvCmxGXN7iATWYOOs172o4BA69mb4dNVfaAYG0 dima@mac-to-vm'
New-Item -ItemType Directory -Force C:\ProgramData\ssh | Out-Null
Add-Content -Path C:\ProgramData\ssh\administrators_authorized_keys -Value $k
icacls C:\ProgramData\ssh\administrators_authorized_keys /inheritance:r /grant 'Administrators:F' /grant 'SYSTEM:F'

# Windows 11 hides netplwiz's auto-logon checkbox by default; unhide it.
reg add "HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\PasswordLess\Device" /v DevicePasswordLessBuildVersion /t REG_DWORD /d 0 /f

ipconfig
