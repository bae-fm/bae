#!/usr/bin/env python3
"""Copy the FFmpeg dylibs the app loads into its Frameworks folder.

Runs as the bae target's last build phase. The app links FFmpeg from the
bae-ffmpeg dist on LIBRARY_SEARCH_PATHS and loads it as @rpath/<name>, which
the target's runpaths resolve only inside the bundle. For each such name that a
binary in Contents/MacOS, or a library copied here, loads, the first search
folder holding it supplies the file. The dist names each library by its full
version and links the shorter names to it; the copy keeps those links, so every
name resolves to one file and dyld loads each library once. What it copies is
signed when the build signs.
"""

import os
import shlex
import shutil
import subprocess
import sys
from pathlib import Path

LOAD_COMMANDS = {
    "LC_LOAD_DYLIB", "LC_LOAD_WEAK_DYLIB", "LC_REEXPORT_DYLIB",
    "LC_LAZY_LOAD_DYLIB", "LC_LOAD_UPWARD_DYLIB",
}


def rpath_dylibs(binary):
    """The @rpath/<name>.dylib names a Mach-O file loads, from its load commands.

    A framework (@rpath/Sparkle.framework/Versions/B/Sparkle) is Xcode's to
    embed, so only a bare dylib name counts.
    """
    lines = subprocess.run(["otool", "-l", str(binary)], check=True,
                           capture_output=True, text=True).stdout.splitlines()
    names = []
    for index, line in enumerate(lines):
        if line.strip().startswith("cmd ") and line.split()[1] in LOAD_COMMANDS:
            load = lines[index + 2].split()[1]
            name = load[len("@rpath/"):]
            if (load.startswith("@rpath/") and "/" not in name
                    and name.endswith(".dylib") and name not in names):
                names.append(name)
    return names


def is_macho(path):
    with open(path, "rb") as stream:
        return stream.read(4) in (b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe")


def main():
    env = os.environ
    products = Path(env["TARGET_BUILD_DIR"])
    macos = products / env["EXECUTABLE_FOLDER_PATH"]
    frameworks = products / env["FRAMEWORKS_FOLDER_PATH"]
    search = [Path(d) for d in shlex.split(env.get("LIBRARY_SEARCH_PATHS", ""))]

    pending = [p for p in macos.iterdir()
               if p.is_file() and not p.is_symlink() and is_macho(p)]
    handled, copied = set(), []
    while pending:
        for name in rpath_dylibs(pending.pop()):
            if name in handled:
                continue
            handled.add(name)
            source = next((d / name for d in search if (d / name).exists()), None)
            if source is None:
                # Not from a library search path: Xcode copies its own
                # (libswiftCompatibilitySpan.dylib).
                continue
            real = source.resolve()
            if real.parent != source.parent.resolve():
                sys.exit(f"{source} links outside its folder, to {real}")
            frameworks.mkdir(parents=True, exist_ok=True)
            target = frameworks / real.name
            if target not in copied:
                if target.exists() or target.is_symlink():
                    target.unlink()
                shutil.copy2(real, target)
                os.chmod(target, 0o755)
                copied.append(target)
                pending.append(target)
            if name != real.name:
                alias = frameworks / name
                if alias.exists() or alias.is_symlink():
                    alias.unlink()
                alias.symlink_to(real.name)
                copied.append(alias)

    if env.get("CODE_SIGNING_ALLOWED") == "YES":
        identity = env.get("EXPANDED_CODE_SIGN_IDENTITY") or "-"
        flags = shlex.split(env.get("OTHER_CODE_SIGN_FLAGS", ""))
        for target in copied:
            if not target.is_symlink():
                subprocess.run(["codesign", "--force", "--sign", identity,
                                "--timestamp=none", *flags, str(target)], check=True)
    for target in copied:
        print(f"Embedded {target.name}")


if __name__ == "__main__":
    main()
