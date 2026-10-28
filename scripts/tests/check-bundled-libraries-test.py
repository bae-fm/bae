#!/usr/bin/env python3
"""Tests for scripts/check-bundled-libraries.py on synthesized binaries."""

import os
import struct
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

CHECK = Path(__file__).resolve().parent.parent / "check-bundled-libraries.py"

MH_EXECUTE, MH_DYLIB = 2, 6
LC_LOAD_DYLIB, LC_RPATH = 0xC, 0x8000001C


def padded(text, align):
    raw = text.encode() + b"\0"
    return raw + b"\0" * (-len(raw) % align)


def macho(filetype, loads=(), rpaths=()):
    commands = b""
    for name in loads:
        body = padded(name, 8)
        commands += struct.pack("<6I", LC_LOAD_DYLIB, 24 + len(body), 24, 0, 0, 0) + body
    for path in rpaths:
        body = padded(path, 8)
        commands += struct.pack("<3I", LC_RPATH, 12 + len(body), 12) + body
    count = len(loads) + len(rpaths)
    header = struct.pack("<8I", 0xFEEDFACF, 0x0100000C, 0, filetype, count,
                         len(commands), 0, 0)
    return header + commands


def elf(needed=(), runpath=None):
    strings = b"\0"
    offsets = []
    for name in list(needed) + ([runpath] if runpath else []):
        offsets.append(len(strings))
        strings += name.encode() + b"\0"
    dynamic_offset = 64 + 2 * 56
    entries = [(1, offset) for offset in offsets[:len(needed)]]
    if runpath:
        entries.append((29, offsets[-1]))
    strtab_offset = dynamic_offset + (len(entries) + 2) * 16
    entries.append((5, strtab_offset))
    entries.append((0, 0))
    dynamic = b"".join(struct.pack("<qQ", tag, value) for tag, value in entries)
    size = strtab_offset + len(strings)
    header = (b"\x7fELF\x02\x01\x01" + b"\0" * 9
              + struct.pack("<2HI3QI6H", 3, 183, 1, 0, 64, 0, 0, 64, 56, 2, 64, 0, 0))
    load = struct.pack("<2I6Q", 1, 4, 0, 0, 0, size, size, 0x1000)
    dyn = struct.pack("<2I6Q", 2, 4, dynamic_offset, dynamic_offset, dynamic_offset,
                      len(dynamic), len(dynamic), 8)
    return header + load + dyn + dynamic + strings


def pe(imports):
    section_rva, section_raw = 0x1000, 0x200
    descriptors_size = 20 * (len(imports) + 1)
    names, name_rvas = b"", []
    for name in imports:
        name_rvas.append(section_rva + descriptors_size + len(names))
        names += name.encode() + b"\0"
    descriptors = b"".join(struct.pack("<5I", 0, 0, 0, rva, 0) for rva in name_rvas)
    section = descriptors + b"\0" * 20 + names
    dos = b"MZ" + b"\0" * 58 + struct.pack("<I", 0x40)
    coff = b"PE\0\0" + struct.pack("<2H3I2H", 0x8664, 1, 0, 0, 0, 240, 0x2022)
    optional = bytearray(240)
    struct.pack_into("<H", optional, 0, 0x20B)
    struct.pack_into("<I", optional, 108, 16)
    struct.pack_into("<2I", optional, 112 + 8, section_rva, len(section))
    # Name, VirtualSize, VirtualAddress, SizeOfRawData, PointerToRawData, then
    # relocation/line-number fields and characteristics, unused here.
    table = (b".idata\0\0"
             + struct.pack("<4I", len(section), section_rva, len(section), section_raw)
             + b"\0" * 16)
    head = dos + coff + bytes(optional) + table
    return head + b"\0" * (section_raw - len(head)) + section


def write(root, relative, data):
    path = Path(root) / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def check(*args):
    result = subprocess.run([sys.executable, str(CHECK), *map(str, args)],
                            capture_output=True, text=True)
    return result.returncode, result.stderr


class MachOTests(unittest.TestCase):
    def app(self, root, executable_rpaths=("@executable_path/../Frameworks",),
            library_loads=("@rpath/libbar.1.dylib",)):
        write(root, "bae.app/Contents/MacOS/bae", macho(
            MH_EXECUTE, ["@rpath/libfoo.1.dylib", "/usr/lib/libSystem.B.dylib"],
            executable_rpaths))
        write(root, "bae.app/Contents/Frameworks/libfoo.1.dylib",
              macho(MH_DYLIB, library_loads))
        write(root, "bae.app/Contents/Frameworks/libbar.1.dylib", macho(MH_DYLIB))
        return Path(root) / "bae.app"

    def test_bundled_and_system_loads_pass(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertEqual(check(self.app(root))[0], 0)

    def test_rpath_outside_the_bundle_fails(self):
        with tempfile.TemporaryDirectory() as root:
            app = self.app(root, ("@executable_path/../Frameworks", "/opt/bae-ffmpeg/lib"))
            status, output = check(app)
            self.assertEqual(status, 1)
            self.assertIn("LC_RPATH outside the bundle: /opt/bae-ffmpeg/lib", output)

    def test_allowed_rpath_is_not_searched(self):
        with tempfile.TemporaryDirectory() as root:
            products = "/build/Products/Debug/PackageFrameworks"
            app = self.app(root, ("@executable_path/../Frameworks", products))
            self.assertEqual(check("--allow-rpath", products, app)[0], 0)
            (app / "Contents/Frameworks/libbar.1.dylib").unlink()
            status, output = check("--allow-rpath", products, app)
            self.assertEqual(status, 1)
            self.assertIn("@rpath/libbar.1.dylib", output)

    def test_homebrew_load_fails(self):
        with tempfile.TemporaryDirectory() as root:
            app = self.app(root, library_loads=(
                "@rpath/libbar.1.dylib", "/opt/homebrew/opt/libx11/lib/libX11.6.dylib"))
            status, output = check(app)
            self.assertEqual(status, 1)
            self.assertIn("libX11.6.dylib", output)

    def test_unbundled_rpath_load_fails(self):
        with tempfile.TemporaryDirectory() as root:
            app = self.app(root, library_loads=("@rpath/libmissing.dylib",))
            status, output = check(app)
            self.assertEqual(status, 1)
            self.assertIn("@rpath/libmissing.dylib", output)

    def test_symlink_out_of_the_bundle_fails(self):
        with tempfile.TemporaryDirectory() as root:
            app = self.app(root)
            os.symlink("/usr/lib", app / "Contents/Frameworks/outside")
            status, output = check(app)
            self.assertEqual(status, 1)
            self.assertIn("symlink points outside the bundle", output)


class ElfTests(unittest.TestCase):
    def test_runpath_sibling_and_system_pass(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "lib/libavcodec.so.62", elf(["libavutil.so.60", "libc.so.6"], "$ORIGIN"))
            write(root, "lib/libavutil.so.60", elf(["libc.so.6"], "$ORIGIN"))
            self.assertEqual(check("--platform", "linux", root)[0], 0)

    def test_sibling_without_runpath_fails_on_linux(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "lib/libavcodec.so.62", elf(["libavutil.so.60"]))
            write(root, "lib/libavutil.so.60", elf())
            status, output = check("--platform", "linux", root)
            self.assertEqual(status, 1)
            self.assertIn("needs libavutil.so.60", output)

    def test_distribution_package_library_fails_on_linux(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "lib/libavcodec.so.62", elf(["libmp3lame.so.0"], "$ORIGIN"))
            status, output = check("--platform", "linux", root)
            self.assertEqual(status, 1)
            self.assertIn("needs libmp3lame.so.0", output)

    def test_android_sibling_resolves_without_runpath(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "lib/arm64-v8a/libbae_bridge.so", elf(["libavcodec.so", "liblog.so"]))
            write(root, "lib/arm64-v8a/libavcodec.so", elf(["libc.so"]))
            self.assertEqual(check("--platform", "android", root)[0], 0)

    def test_absolute_runpath_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "lib/libavcodec.so.62", elf(["libc.so.6"], "/opt/bae-ffmpeg/lib"))
            status, output = check("--platform", "linux", root)
            self.assertEqual(status, 1)
            self.assertIn("runpath outside the bundle", output)

    def test_apk_abi_missing_a_library_fails(self):
        with tempfile.TemporaryDirectory() as root:
            apk = Path(root) / "app.apk"
            with zipfile.ZipFile(apk, "w") as zipped:
                zipped.writestr("lib/arm64-v8a/libbae_bridge.so", elf(["libc.so"]))
                zipped.writestr("lib/arm64-v8a/libsentry.so", elf(["libc.so"]))
                zipped.writestr("lib/armeabi-v7a/libsentry.so", elf(["libc.so"]))
            status, output = check("--platform", "android", apk)
            self.assertEqual(status, 1)
            self.assertIn("lib/armeabi-v7a/ lacks libbae_bridge.so", output)


class PeTests(unittest.TestCase):
    def test_sibling_and_system_imports_pass(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "uniffi_bae_bridge.dll", pe(
                ["avcodec-62.dll", "KERNEL32.dll", "api-ms-win-crt-runtime-l1-1-0.dll"]))
            write(root, "avcodec-62.dll", pe(["KERNEL32.dll"]))
            self.assertEqual(check(root)[0], 0)

    def test_visual_cpp_runtime_import_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "uniffi_bae_bridge.dll", pe(["VCRUNTIME140.dll", "KERNEL32.dll"]))
            status, output = check(root)
            self.assertEqual(status, 1)
            self.assertIn("imports VCRUNTIME140.dll", output)

    def test_bundled_visual_cpp_runtime_passes(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "uniffi_bae_bridge.dll", pe(["VCRUNTIME140.dll"]))
            write(root, "vcruntime140.dll", pe(["KERNEL32.dll"]))
            self.assertEqual(check(root)[0], 0)


class InputTests(unittest.TestCase):
    def test_bundle_without_binaries_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "README", b"text")
            status, output = check(root)
            self.assertEqual(status, 1)
            self.assertIn("no binaries found", output)


if __name__ == "__main__":
    unittest.main()
