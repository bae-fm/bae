#!/usr/bin/env python3
"""Fail when a built binary depends on a library outside its bundle or the OS.

Each argument is a bundle: a directory (a macOS or iOS .app, an FFmpeg
distribution, a Windows staging directory) or an .apk, .ipa or .zip, which is
read in place. Every Mach-O, ELF and PE file inside is read for the libraries
it loads and the runtime search paths it carries, and every reference must
resolve to a file inside the bundle or to a library the platform itself
provides:

- Mach-O: a load is a system path (/usr/lib, /System) or an @rpath,
  @loader_path or @executable_path name that resolves inside the bundle. An
  LC_RPATH is a system path or an @-relative path that stays inside the
  bundle; an absolute path elsewhere (a build machine's checkout, /opt, a
  Homebrew prefix) is a reference outside the bundle even when nothing loads
  through it yet.
- ELF: a DT_NEEDED name is the platform's own library (--platform) or a file
  the image's DT_RUNPATH/DT_RPATH finds inside the bundle. Android's loader
  also searches the APK's own library directory, so on Android a sibling
  resolves without a runpath. A runpath must be $ORIGIN-relative and stay
  inside the bundle.
- PE: an imported DLL is a Windows system DLL or a file in the importer's own
  directory, where the Windows loader looks first.

In an APK, every lib/<abi>/ directory must carry the same libraries: an ABI
directory missing one would install on a device of that ABI and fail to load.
A symlink inside a bundle must not point outside it.

--platform names whose system libraries an ELF bundle may use: android or
linux. Mach-O and PE bundles need no platform; their system locations are
fixed. --allow-rpath accepts one absolute LC_RPATH, for the build products'
PackageFrameworks folder Xcode adds to a Debug build; nothing resolves through
it, so every load must still resolve inside the bundle.
"""

import argparse
import os
import posixpath
import struct
import sys
import tempfile
import zipfile
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath

# --- system libraries ------------------------------------------------------

MACHO_SYSTEM_PREFIXES = ("/usr/lib/", "/System/")

# The NDK's stable system libraries, which every Android device provides.
ANDROID_SYSTEM = {
    "libaaudio.so", "libamidi.so", "libandroid.so", "libbinder_ndk.so",
    "libc.so", "libcamera2ndk.so", "libdl.so", "libEGL.so", "libGLESv1_CM.so",
    "libGLESv2.so", "libGLESv3.so", "libicu.so", "libjnigraphics.so",
    "liblog.so", "libm.so", "libmediandk.so", "libnativewindow.so",
    "libneuralnetworks.so", "libOpenMAXAL.so", "libOpenSLES.so", "libstdc++.so",
    "libsync.so", "libvulkan.so", "libz.so",
}

# glibc's own libraries, plus zlib, which every glibc distribution's base
# system installs.
LINUX_SYSTEM = {
    "ld-linux-aarch64.so.1", "ld-linux-x86-64.so.2", "libc.so.6", "libdl.so.2",
    "libm.so.6", "libpthread.so.0", "libresolv.so.2", "librt.so.1", "libz.so.1",
}

# DLLs Windows 10 and later install in System32. The Universal CRT
# (ucrtbase, api-ms-win-crt-*) is part of the OS; the Visual C++ runtime
# (vcruntime140, msvcp140) is not, so it is absent here.
WINDOWS_SYSTEM = {
    "advapi32.dll", "avrt.dll", "bcrypt.dll", "bcryptprimitives.dll",
    "cfgmgr32.dll", "combase.dll", "comctl32.dll", "comdlg32.dll",
    "crypt32.dll", "d2d1.dll", "d3d11.dll", "d3d12.dll", "d3dcompiler_47.dll",
    "dbghelp.dll", "dnsapi.dll", "dwmapi.dll", "dwrite.dll", "dxgi.dll",
    "gdi32.dll", "hid.dll", "imm32.dll", "iphlpapi.dll", "kernel32.dll",
    "kernelbase.dll", "mscoree.dll", "msimg32.dll", "mswsock.dll",
    "ncrypt.dll", "netapi32.dll", "normaliz.dll", "ntdll.dll", "ole32.dll",
    "oleaut32.dll", "opengl32.dll", "powrprof.dll", "propsys.dll",
    "psapi.dll", "rpcrt4.dll", "secur32.dll", "setupapi.dll", "shcore.dll",
    "shell32.dll", "shlwapi.dll", "sspicli.dll", "ucrtbase.dll", "urlmon.dll",
    "user32.dll", "userenv.dll", "usp10.dll", "uxtheme.dll", "version.dll",
    "windowscodecs.dll", "winhttp.dll", "wininet.dll", "winmm.dll",
    "winspool.drv", "wintrust.dll", "ws2_32.dll", "wtsapi32.dll",
}
WINDOWS_API_SET_PREFIXES = ("api-ms-win-", "ext-ms-win-")

ELF_SYSTEM = {"android": ANDROID_SYSTEM, "linux": LINUX_SYSTEM}


@dataclass
class MachO:
    executable: bool
    loads: list = field(default_factory=list)
    rpaths: list = field(default_factory=list)


@dataclass
class Elf:
    needed: list = field(default_factory=list)
    runpaths: list = field(default_factory=list)


@dataclass
class Pe:
    imports: list = field(default_factory=list)


# --- Mach-O ----------------------------------------------------------------

MH_EXECUTE = 2
LC_REQ_DYLD = 0x80000000
LC_LOAD_DYLIB = 0xC
LC_LOAD_WEAK_DYLIB = 0x18 | LC_REQ_DYLD
LC_REEXPORT_DYLIB = 0x1F | LC_REQ_DYLD
LC_LAZY_LOAD_DYLIB = 0x20
LC_LOAD_UPWARD_DYLIB = 0x23 | LC_REQ_DYLD
LC_RPATH = 0x1C | LC_REQ_DYLD
LOAD_COMMANDS = {
    LC_LOAD_DYLIB, LC_LOAD_WEAK_DYLIB, LC_REEXPORT_DYLIB, LC_LAZY_LOAD_DYLIB,
    LC_LOAD_UPWARD_DYLIB,
}


def c_string(data, offset):
    end = data.find(b"\0", offset)
    return data[offset:end if end >= 0 else len(data)].decode("utf-8", "replace")


def parse_macho_slice(data, base, macho):
    magic = data[base:base + 4]
    if magic in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe"):
        endian = "<"
    elif magic in (b"\xfe\xed\xfa\xcf", b"\xfe\xed\xfa\xce"):
        endian = ">"
    else:
        return
    is64 = magic in (b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf")
    _, _, _, filetype, ncmds, _, _ = struct.unpack_from(endian + "7I", data, base)
    macho.executable = macho.executable or filetype == MH_EXECUTE
    offset = base + (32 if is64 else 28)
    for _ in range(ncmds):
        cmd, cmdsize = struct.unpack_from(endian + "2I", data, offset)
        if cmd in LOAD_COMMANDS or cmd == LC_RPATH:
            (name_offset,) = struct.unpack_from(endian + "I", data, offset + 8)
            name = c_string(data, offset + name_offset)
            target = macho.rpaths if cmd == LC_RPATH else macho.loads
            if name not in target:
                target.append(name)
        offset += cmdsize


def parse_macho(data):
    magic = data[:4]
    if magic in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
        (count,) = struct.unpack_from(">I", data, 4)
        # Java class files share the fat magic; their version field reads as a
        # count far above any real universal binary's.
        if count == 0 or count > 16:
            return None
        macho = MachO(executable=False)
        entry = 32 if magic == b"\xca\xfe\xba\xbf" else 20
        for index in range(count):
            at = 8 + index * entry
            if magic == b"\xca\xfe\xba\xbf":
                (offset,) = struct.unpack_from(">Q", data, at + 8)
            else:
                (offset,) = struct.unpack_from(">I", data, at + 8)
            parse_macho_slice(data, offset, macho)
        return macho
    if magic in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe",
                 b"\xfe\xed\xfa\xcf", b"\xfe\xed\xfa\xce"):
        macho = MachO(executable=False)
        parse_macho_slice(data, 0, macho)
        return macho
    return None


# --- ELF -------------------------------------------------------------------

PT_LOAD = 1
PT_DYNAMIC = 2
DT_NEEDED = 1
DT_STRTAB = 5
DT_RPATH = 15
DT_RUNPATH = 29


def parse_elf(data):
    if data[:4] != b"\x7fELF":
        return None
    is64 = data[4] == 2
    endian = "<" if data[5] == 1 else ">"
    if is64:
        phoff, = struct.unpack_from(endian + "Q", data, 0x20)
        phentsize, phnum = struct.unpack_from(endian + "2H", data, 0x36)
        header, dyn_entry = endian + "2I6Q", endian + "qQ"
    else:
        phoff, = struct.unpack_from(endian + "I", data, 0x1C)
        phentsize, phnum = struct.unpack_from(endian + "2H", data, 0x2A)
        header, dyn_entry = endian + "8I", endian + "iI"
    loads, dynamic = [], None
    for index in range(phnum):
        fields = struct.unpack_from(header, data, phoff + index * phentsize)
        if is64:
            p_type, _, p_offset, p_vaddr, _, p_filesz, _, _ = fields
        else:
            p_type, p_offset, p_vaddr, _, p_filesz, _, _, _ = fields
        if p_type == PT_LOAD:
            loads.append((p_vaddr, p_offset, p_filesz))
        elif p_type == PT_DYNAMIC:
            dynamic = (p_offset, p_filesz)
    elf = Elf()
    if dynamic is None:
        return elf
    entries, strtab = [], None
    size = struct.calcsize(dyn_entry)
    for at in range(dynamic[0], dynamic[0] + dynamic[1], size):
        tag, value = struct.unpack_from(dyn_entry, data, at)
        if tag == 0:
            break
        if tag == DT_STRTAB:
            strtab = value
        entries.append((tag, value))
    if strtab is None:
        return elf
    strtab_offset = next(
        (offset + strtab - vaddr for vaddr, offset, filesz in loads
         if vaddr <= strtab < vaddr + filesz),
        None,
    )
    if strtab_offset is None:
        return elf
    for tag, value in entries:
        if tag == DT_NEEDED:
            elf.needed.append(c_string(data, strtab_offset + value))
        elif tag in (DT_RPATH, DT_RUNPATH):
            elf.runpaths.extend(
                part for part in c_string(data, strtab_offset + value).split(":") if part
            )
    return elf


# --- PE --------------------------------------------------------------------

def parse_pe(data):
    if data[:2] != b"MZ" or len(data) < 0x40:
        return None
    (pe_offset,) = struct.unpack_from("<I", data, 0x3C)
    if data[pe_offset:pe_offset + 4] != b"PE\0\0":
        return None
    coff = pe_offset + 4
    _, sections, _, _, _, optional_size, _ = struct.unpack_from("<2H3I2H", data, coff)
    optional = coff + 20
    (magic,) = struct.unpack_from("<H", data, optional)
    directories = optional + (112 if magic == 0x20B else 96)
    (directory_count,) = struct.unpack_from("<I", data, directories - 4)
    section_table = optional + optional_size
    spans = []
    for index in range(sections):
        at = section_table + index * 40
        virtual_size, virtual_address, raw_size, raw_offset = struct.unpack_from(
            "<4I", data, at + 8)
        spans.append((virtual_address, max(virtual_size, raw_size), raw_offset))

    def offset_of(rva):
        for start, length, raw in spans:
            if start <= rva < start + length:
                return raw + rva - start
        return None

    def directory(index):
        if index >= directory_count:
            return 0, 0
        return struct.unpack_from("<2I", data, directories + index * 8)

    pe = Pe()
    import_rva, _ = directory(1)
    at = offset_of(import_rva) if import_rva else None
    while at is not None:
        descriptor = struct.unpack_from("<5I", data, at)
        if not any(descriptor):
            break
        name = offset_of(descriptor[3])
        if name is not None:
            pe.imports.append(c_string(data, name))
        at += 20
    delay_rva, _ = directory(13)
    at = offset_of(delay_rva) if delay_rva else None
    while at is not None:
        descriptor = struct.unpack_from("<8I", data, at)
        if not any(descriptor):
            break
        name = offset_of(descriptor[1])
        if name is not None:
            pe.imports.append(c_string(data, name))
        at += 32
    return pe


# --- bundle walk -----------------------------------------------------------

def read_binary(path):
    with open(path, "rb") as stream:
        head = stream.read(4)
        if head[:2] != b"MZ" and head not in (
            b"\x7fELF", b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf",
            b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
            b"\xfe\xed\xfa\xce",
        ):
            return None
        data = head + stream.read()
    for parse in (parse_macho, parse_elf, parse_pe):
        try:
            parsed = parse(data)
        except (struct.error, IndexError):
            parsed = None
        if parsed is not None:
            return parsed
    return None


def inside(root, path):
    root = os.path.realpath(root)
    path = os.path.realpath(path)
    return path == root or path.startswith(root + os.sep)


class Checker:
    def __init__(self, root, label, platform, allowed_rpaths=()):
        self.root = Path(root)
        self.label = label
        self.platform = platform
        # Compared by real path: a build folder reached through a symlink is
        # recorded in the binary by its target.
        self.allowed_rpaths = {os.path.realpath(path) for path in allowed_rpaths}
        self.problems = []
        self.binaries = {}

    def problem(self, path, message):
        relative = os.path.relpath(path, self.root)
        self.problems.append(f"{self.label}: {relative}: {message}")

    def walk(self):
        for directory, subdirectories, files in os.walk(self.root):
            for name in subdirectories + files:
                path = Path(directory) / name
                if path.is_symlink():
                    if not inside(self.root, path):
                        self.problem(path, f"symlink points outside the bundle ({os.readlink(path)})")
                    continue
                if name in files:
                    parsed = read_binary(path)
                    if parsed is not None:
                        self.binaries[path] = parsed

    def check(self):
        self.walk()
        executables = [
            path for path, parsed in self.binaries.items()
            if isinstance(parsed, MachO) and parsed.executable
        ]
        for path, parsed in sorted(self.binaries.items()):
            if isinstance(parsed, MachO):
                self.check_macho(path, parsed, executables)
            elif isinstance(parsed, Elf):
                self.check_elf(path, parsed)
            else:
                self.check_pe(path, parsed)

    # Mach-O: @executable_path is the process's main executable, @loader_path
    # the image doing the load, and @rpath every LC_RPATH of the image and of
    # the executable that loaded it.
    def macho_expand(self, path, value, executables):
        if value.startswith("@loader_path"):
            return [str(path.parent) + value[len("@loader_path"):]]
        if value.startswith("@executable_path"):
            dirs = [path.parent] if self.binaries[path].executable else [
                executable.parent for executable in executables]
            return [str(d) + value[len("@executable_path"):] for d in dirs]
        return [value]

    def check_macho(self, path, macho, executables):
        rpath_dirs = []
        for rpath in macho.rpaths:
            if rpath.startswith(MACHO_SYSTEM_PREFIXES):
                continue
            if not rpath.startswith("@") and os.path.realpath(rpath) in self.allowed_rpaths:
                continue
            if not rpath.startswith("@"):
                self.problem(path, f"LC_RPATH outside the bundle: {rpath}")
                continue
            for expanded in self.macho_expand(path, rpath, executables):
                if inside(self.root, expanded):
                    rpath_dirs.append(expanded)
                else:
                    self.problem(path, f"LC_RPATH leaves the bundle: {rpath}")
        if not macho.executable:
            for executable in executables:
                for rpath in self.binaries[executable].rpaths:
                    if rpath.startswith("@"):
                        rpath_dirs.extend(
                            d for d in self.macho_expand(executable, rpath, executables)
                            if inside(self.root, d))
        for load in macho.loads:
            if load.startswith(MACHO_SYSTEM_PREFIXES):
                continue
            if load.startswith("@rpath/"):
                name = load[len("@rpath/"):]
                # A library found through @rpath finds its siblings through the
                # same search path, so a file next to it resolves too.
                candidates = [posixpath.join(d, name) for d in rpath_dirs]
                candidates.append(str(path.parent / name))
            elif load.startswith("@"):
                candidates = self.macho_expand(path, load, executables)
            else:
                self.problem(path, f"loads a library outside the bundle: {load}")
                continue
            if not any(os.path.isfile(c) and inside(self.root, c) for c in candidates):
                self.problem(path, f"loads {load}, which resolves to no file in the bundle")

    def check_elf(self, path, elf):
        if self.platform is None:
            self.problem(path, "ELF binary in a bundle checked without --platform")
            return
        system = ELF_SYSTEM[self.platform]
        dirs = []
        for runpath in elf.runpaths:
            if not runpath.startswith("$ORIGIN"):
                self.problem(path, f"runpath outside the bundle: {runpath}")
                continue
            expanded = str(path.parent) + runpath[len("$ORIGIN"):]
            if inside(self.root, expanded):
                dirs.append(expanded)
            else:
                self.problem(path, f"runpath leaves the bundle: {runpath}")
        if self.platform == "android":
            dirs.append(str(path.parent))
        for needed in elf.needed:
            if needed in system:
                continue
            if "/" in needed:
                self.problem(path, f"needs a library by path: {needed}")
                continue
            if not any(os.path.isfile(os.path.join(d, needed)) for d in dirs):
                self.problem(path, f"needs {needed}, which is neither a {self.platform} "
                             "system library nor found in the bundle by its runpath")

    def check_pe(self, path, pe):
        present = {entry.name.lower() for entry in path.parent.iterdir()}
        for name in pe.imports:
            lowered = name.lower()
            if lowered in WINDOWS_SYSTEM or lowered.startswith(WINDOWS_API_SET_PREFIXES):
                continue
            if lowered not in present:
                self.problem(path, f"imports {name}, which is neither a Windows system "
                             "DLL nor next to it in the bundle")

    def check_apk_abis(self):
        lib = self.root / "lib"
        if not lib.is_dir():
            return
        abis = {d.name: {f.name for f in d.iterdir()} for d in lib.iterdir() if d.is_dir()}
        every = set().union(*abis.values()) if abis else set()
        for abi, names in sorted(abis.items()):
            for missing in sorted(every - names):
                self.problems.append(
                    f"{self.label}: lib/{abi}/ lacks {missing}, which other ABIs ship")


def extract(archive, into):
    with zipfile.ZipFile(archive) as zipped:
        for info in zipped.infolist():
            target = PurePosixPath(info.filename)
            if target.is_absolute() or ".." in target.parts:
                raise SystemExit(f"{archive}: unsafe path in archive: {info.filename}")
            mode = info.external_attr >> 16
            destination = Path(into) / target
            if info.is_dir():
                destination.mkdir(parents=True, exist_ok=True)
                continue
            destination.parent.mkdir(parents=True, exist_ok=True)
            if (mode & 0o170000) == 0o120000:
                os.symlink(zipped.read(info).decode(), destination)
            else:
                destination.write_bytes(zipped.read(info))


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("bundles", nargs="+", type=Path)
    parser.add_argument("--platform", choices=sorted(ELF_SYSTEM))
    parser.add_argument("--allow-rpath", action="append", default=[], metavar="PATH")
    args = parser.parse_args()
    problems, checked = [], 0
    for bundle in args.bundles:
        if not bundle.exists():
            problems.append(f"{bundle}: does not exist")
            continue
        if bundle.is_dir():
            checker = Checker(bundle, str(bundle), args.platform, args.allow_rpath)
            checker.check()
            problems.extend(checker.problems)
            checked += len(checker.binaries)
            continue
        with tempfile.TemporaryDirectory() as directory:
            extract(bundle, directory)
            checker = Checker(directory, str(bundle), args.platform, args.allow_rpath)
            checker.check()
            if bundle.suffix == ".apk":
                checker.check_apk_abis()
            problems.extend(checker.problems)
            checked += len(checker.binaries)
    if checked == 0 and not problems:
        problems.append("no binaries found in " + ", ".join(map(str, args.bundles)))
    for line in problems:
        print(line, file=sys.stderr)
    if problems:
        print(f"{len(problems)} reference(s) outside the bundle or the OS", file=sys.stderr)
        return 1
    print(f"{checked} binaries load only bundled or system libraries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
