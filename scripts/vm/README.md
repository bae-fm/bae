# Windows dev VM (UTM on Apple Silicon)

A Windows 11 ARM VM that builds and runs the full bae Windows stack — the
compile-run-debug loop CI can't provide (CI only compiles bae-windows; it
never launches the app, which is how a launch-blocking bug once shipped).

## One-time manual steps (~15 min of clicks)

1. **Installer ISO**: CrystalFetch (Mac App Store / brew cask) → Windows 11,
   latest, Apple Silicon, Download (~5 GB).
2. **UTM** (brew cask `utm`): ＋ → Virtualize → Windows → pick the ISO →
   RAM 8192 MB → disk **96 GB** (sparse; 64 fills up — toolchain + repo +
   build trees need the headroom) → Save → ▶︎.
3. **Windows setup**: "I don't have a product key" → Windows 11 Pro → custom
   install. Create local user `tom`.
4. **In the VM, admin PowerShell** (Start → type `powershell` → right-click →
   Run as administrator), paste `guest-bootstrap.ps1` (SSH server + firewall +
   the host's public key). Note the IPv4 from its output.
5. **Auto-logon** (so reboots come back to a live desktop — interactive tasks
   need one): the bootstrap script unhides the checkbox; run `netplwiz`,
   untick "Users must enter a user name and password", enter the password.

## Everything else is scripted

From the repo root on the Mac:

```sh
scripts/vm/provision.sh <vm-ip>     # ~30-40 min, unattended
```

Installs the toolchain (VS Build Tools + Windows SDK, .NET 8, Rust with the
pinned toolchain + x64 target, LLVM, git, sccache, WinDbg), clones the repo to
C:\bae, fetches the self-contained FFmpeg dist, installs uniffi-bindgen-cs,
and stages the hidden-launcher scheduled-task harness (`baeRun` to launch the
app on the desktop session, `vmShot` to screenshot it — console apps started
by scheduled tasks otherwise pile terminal windows onto the desktop).

Build + run after provisioning:

```sh
ssh tom@<ip> 'C:\Users\tom\bridge-build.cmd'   # Rust bridge + C# bindings
ssh tom@<ip> 'C:\Users\tom\build-normal.cmd'   # WinUI app (framework-dependent)
ssh tom@<ip> 'schtasks /run /tn baeRun'        # launch on the VM desktop
ssh tom@<ip> 'schtasks /run /tn vmShot'        # screenshot → C:\Users\tom\vmshot.png
```

That's the iterate loop. Separately, `build-release.cmd` runs the release
lane locally — release bridge, self-contained publish, `vpk pack` — and drops
a Setup.exe under `C:\bae\velopack-local` for exercising the installer path
(channel `local`, so an installed test build never reads the public update
feeds). `tidy.ps1` (staged to `C:\Users\tom`) sweeps consumer preinstalls and
OS caches, keeping every build cache, and ReTrims so the host qcow2 shrinks.

## Hard-won facts baked into these scripts

- **winget needs `--source winget` on every install**: the `msstore` source
  fails certificate validation (0x8a15005e) on a fresh VM, and an unpinned
  `winget install` aborts when any source errors — even after the community
  source already found the package.
- **x64, not ARM, for now**: prebuilt FFmpeg ships x64 (ARM artifact exists as
  of `windows-arm64` work — switch when bae grows its ARM lane); Windows runs
  x64 under emulation. `cargo test` must pass `--target x86_64-pc-windows-msvc`
  or link failures follow (host is aarch64).
- **Scheduled tasks with `/it` need a logged-in desktop** — hence auto-logon.
  `/rl highest` (elevation) breaks WinUI file pickers and creates
  Administrators-owned state files a normal run then can't touch. Don't elevate
  the app task.
- **SSH sessions can't show windows**; anything windowed goes through the
  scheduled-task harness. SSH-launched app processes may also die silently —
  treat SSH as build/file transport only.
- **WinDbg reads x64 dumps poorly from arm64**; the app's own crash log
  (`%LOCALAPPDATA%\bae\crash.log`, written by CrashCapture) is the primary
  crash record.
- **Growing the disk later**: `qemu-img resize` (brew qemu) on the .qcow2 in
  the UTM bundle, then delete the trailing recovery partition and
  `Resize-Partition` — the recovery partition blocks extension otherwise.
