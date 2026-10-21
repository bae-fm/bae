#!/usr/bin/env python3
"""Wipe this Mac's bae libraries and recreate the dev library from a profile.

Quits the running debug app, confirms, then launches it with
`--dev-reset <profile>`. The app does the reset itself, before any library
opens: it removes every library the way Remove Library does (store keys
leave the keychain), deletes those libraries' saved restore codes, empties the
profile's S3 key prefix (every object under it is deleted and every unfinished
upload aborted, whatever wrote them), deletes ~/.bae, then creates the
profile's library and sets its Discogs token, metadata preferences, and S3
storage. Only a Debug build carries `--dev-reset`.

The profile (default ~/.config/bae-dev/profile.toml) holds credentials, so it
must be mode 600; this script never prints them. Its fields:

    library_name = "dev"
    identify_automatically = false
    import_when_identified = false

    [s3]
    endpoint = "..."
    region = "..."
    bucket = "..."
    key_prefix = "..."
    access_key_id = "..."
    secret_access_key = "..."

    [discogs]
    token = "..."
"""

import argparse
import pathlib
import stat
import subprocess
import sys
import time
import tomllib

REPO = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_PROFILE = pathlib.Path.home() / ".config" / "bae-dev" / "profile.toml"
# The app `bae-macos/run.sh` builds, and its bundle id.
DEFAULT_APP = (
    REPO / "bae-macos" / "bae" / ".build" / "runDerivedData" / "Build" / "Products"
    / "Debug" / "bae.app"
)
BUNDLE_ID = "fm.bae.desktop"


def fail(message: str) -> None:
    print(f"reset-bae: {message}", file=sys.stderr)
    sys.exit(1)


def read_profile(path: pathlib.Path) -> dict:
    if not path.is_file():
        fail(f"no profile at {path}")
    mode = stat.S_IMODE(path.stat().st_mode)
    if mode != 0o600:
        fail(f"{path} has mode {mode:o}; it holds credentials and must be 600")
    try:
        with path.open("rb") as file:
            return tomllib.load(file)
    except tomllib.TOMLDecodeError as error:
        # The decoder's message names a position, not the line's text.
        fail(f"{path} is not valid TOML: {error}")


def running() -> bool:
    script = f'application id "{BUNDLE_ID}" is running'
    result = subprocess.run(
        ["osascript", "-e", script], capture_output=True, text=True, check=True
    )
    return result.stdout.strip() == "true"


def quit_bae() -> None:
    if not running():
        return
    print(f"Quitting the running bae ({BUNDLE_ID})...")
    subprocess.run(
        ["osascript", "-e", f'tell application id "{BUNDLE_ID}" to quit'],
        check=True,
    )
    deadline = time.monotonic() + 30
    while running():
        if time.monotonic() > deadline:
            fail("bae is still running 30 seconds after being asked to quit")
        time.sleep(0.5)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--profile", type=pathlib.Path, default=DEFAULT_PROFILE,
        help=f"the dev profile (default {DEFAULT_PROFILE})",
    )
    parser.add_argument(
        "--app", type=pathlib.Path, default=DEFAULT_APP,
        help="the Debug bae.app to launch (default: the one bae-macos/run.sh builds)",
    )
    args = parser.parse_args()

    profile = read_profile(args.profile)
    if not args.app.is_dir():
        fail(f"no app at {args.app}; build it with bae-macos/run.sh --no-open")
    s3 = profile.get("s3", {})

    print("This will:")
    print(f"  - quit bae ({BUNDLE_ID}) if it is running")
    print("  - remove every bae library on this Mac, with its keychain entries")
    print("    and saved restore codes")
    print(f"  - delete everything in bucket {s3.get('bucket')!r} under prefix")
    print(f"    {s3.get('key_prefix')!r}, and abort its unfinished uploads")
    print(f"  - delete {pathlib.Path.home() / '.bae'}")
    print(f"  - create the library {profile.get('library_name')!r} and connect it to")
    print(f"    bucket {s3.get('bucket')!r}, prefix {s3.get('key_prefix')!r}")
    print(f"  - launch {args.app}")
    if input("Continue? [y/N] ").strip().lower() != "y":
        print("Nothing changed.")
        return

    quit_bae()
    subprocess.run(
        ["open", str(args.app), "--args", "--dev-reset", str(args.profile)],
        check=True,
    )
    print("bae is resetting; its log is in Console under fm.bae.desktop, DevReset.")


if __name__ == "__main__":
    main()
