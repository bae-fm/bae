#!/usr/bin/env python3
"""Package the Avalonia publish output as a signed macOS development app."""

import argparse
from datetime import datetime, timezone
import fnmatch
import hashlib
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tempfile


def run(*args):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE).stdout


def signing(profile_path, bundle_id):
    if not profile_path.is_file():
        raise ValueError(
            f"Provisioning profile missing: {profile_path}\n"
            "Build the matching edition with bae-macos/run.sh first, or set "
            "BAE_AVALONIA_PROVISIONING_PROFILE."
        )
    profile = plistlib.loads(run("security", "cms", "-D", "-i", str(profile_path)))
    if profile["ExpirationDate"].replace(tzinfo=timezone.utc) <= datetime.now(timezone.utc):
        raise ValueError(f"Provisioning profile expired: {profile_path}")
    allowed = profile["Entitlements"]
    team = allowed["com.apple.developer.team-identifier"]
    application_id = f"{profile['ApplicationIdentifierPrefix'][0]}.{bundle_id}"
    if not fnmatch.fnmatchcase(application_id, allowed["com.apple.application-identifier"]):
        raise ValueError(f"Profile does not authorize {application_id}")
    if not any(fnmatch.fnmatchcase(application_id, group)
               for group in allowed["keychain-access-groups"]):
        raise ValueError(f"Profile does not authorize the keychain group {application_id}")

    identities = run("security", "find-identity", "-v", "-p", "codesigning").decode()
    available = set(re.findall(r"\b[0-9A-F]{40}\b", identities))
    certificates = [hashlib.sha1(cert).hexdigest().upper()
                    for cert in profile["DeveloperCertificates"]]
    identity = next((cert for cert in certificates if cert in available), None)
    if identity is None:
        raise ValueError("No valid signing identity with a private key matches the provisioning profile")
    return identity, {
        "com.apple.application-identifier": application_id,
        "com.apple.developer.team-identifier": team,
        "keychain-access-groups": [application_id],
        "com.apple.security.network.client": True,
        "com.apple.security.network.server": True,
    }


def is_native(path):
    if path.is_symlink() or not path.is_file():
        return False
    with path.open("rb") as stream:
        return stream.read(4) in (
            b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
            b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xce",
            b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca",
            b"\xca\xfe\xba\xbf", b"\xbf\xba\xfe\xca",
        )


def bundle(args, identity, entitlements, staging):
    app = staging / args.app.name
    contents = app / "Contents"
    native = contents / "MacOS"
    resources = contents / "Resources"
    shutil.copytree(args.publish, native, symlinks=True)
    resources.mkdir()
    shutil.copy2(args.bridge, native / "libbae_bridge.dylib")
    libraries = list((args.ffmpeg / "lib").glob("*.dylib"))
    if not libraries:
        raise ValueError(f"No FFmpeg dylibs in {args.ffmpeg / 'lib'}")
    for library in libraries:
        # Dereference distribution symlinks so no link can point outside the app.
        shutil.copy2(library, native / library.name)

    # codesign treats MacOS as native code. Managed assemblies and runtime data
    # belong in Resources, with links preserving the .NET apphost's layout.
    for path in list(native.iterdir()):
        if not is_native(path):
            path.rename(resources / path.name)
            path.symlink_to(Path("../Resources") / path.name)
    binaries = [path for path in native.rglob("*") if is_native(path)]
    for path in binaries:
        # otool -L also lists a dylib's own install name. Normalize that first:
        # packages can ship an ID whose basename differs from their filename.
        if path.suffix == ".dylib":
            run("install_name_tool", "-id", f"@loader_path/{path.name}", str(path))
        # Follow absolute dependencies from the distribution (including its
        # Homebrew libraries), then rewrite every reference into the bundle.
        # Universal binaries have one unindented header per architecture.
        dependencies = dict.fromkeys(
            line.strip().split(" (compatibility version", 1)[0]
            for line in run("otool", "-L", str(path)).decode().splitlines()
            if line.startswith("\t")
        )
        for dependency in dependencies:
            if dependency == f"@loader_path/{path.name}" or dependency.startswith(
                ("/System/Library/", "/usr/lib/")
            ):
                continue
            name = Path(dependency).name
            if not (native / name).exists() and Path(dependency).is_absolute():
                shutil.copy2(dependency, native / name)
                binaries.append(native / name)
            if (native / name).is_file():
                run("install_name_tool", "-change", dependency,
                    f"@loader_path/{name}", str(path))
            else:
                raise ValueError(f"Unbundled dependency in {path.name}: {dependency}")
        run("codesign", "--force", "--sign", identity, "--timestamp=none", str(path))

    # The apphost resolves the managed assembly into Resources before probing
    # for hostfxr and P/Invoke libraries, so that location needs native links too.
    for path in native.glob("*.dylib"):
        (resources / path.name).symlink_to(Path("../MacOS") / path.name)

    (contents / "Info.plist").write_bytes(plistlib.dumps({
        "CFBundleExecutable": "bae-avalonia",
        "CFBundleIdentifier": args.bundle_id,
        "CFBundleName": args.app.stem,
        "CFBundleDisplayName": args.app.stem,
        "CFBundlePackageType": "APPL",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleVersion": "1",
        "CFBundleShortVersionString": "1.0",
        "CFBundleDevelopmentRegion": "en",
        "NSHighResolutionCapable": True,
    }))
    (contents / "PkgInfo").write_bytes(b"APPL????")
    shutil.copy2(args.profile, contents / "embedded.provisionprofile")
    entitlement_path = staging / "entitlements.plist"
    entitlement_path.write_bytes(plistlib.dumps(entitlements))
    run("codesign", "--force", "--sign", identity, "--timestamp=none",
        "--entitlements", str(entitlement_path), str(app))
    run("codesign", "--verify", "--deep", "--strict", str(app))
    return app


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--bundle-id", required=True)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--check-signing", action="store_true")
    parser.add_argument("--publish", type=Path)
    parser.add_argument("--bridge", type=Path)
    parser.add_argument("--ffmpeg", type=Path)
    args = parser.parse_args()
    identity, entitlements = signing(args.profile, args.bundle_id)
    executable = str(args.app / "Contents/MacOS/bae-avalonia")
    running = run("ps", "-axo", "comm=").decode().splitlines()
    if executable in (line.strip() for line in running):
        raise ValueError(f"Quit {args.app.name} before rebuilding it.")
    if args.check_signing:
        print(f"Signing profile: {args.profile}")
        return
    if any(value is None for value in (args.publish, args.bridge, args.ffmpeg)):
        parser.error("bundling requires --publish, --bridge and --ffmpeg")
    args.app.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".bundle-", dir=args.app.parent) as directory:
        staging = Path(directory)
        app = bundle(args, identity, entitlements, staging)
        # Publish only a verified bundle; restore the previous app if the rename
        # fails. A failed build or signature never replaces a runnable app.
        previous = staging / "previous.app"
        if args.app.exists():
            args.app.rename(previous)
        try:
            app.rename(args.app)
        except OSError:
            if previous.exists():
                previous.rename(args.app)
            raise


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
