#!/usr/bin/env python3
"""Wipe this Mac's bae libraries and recreate the dev library from a profile.

Confirms, quits the running debug app, empties the profile's S3 bucket, then
launches the app with `--dev-reset <profile>`.

Every library has its bucket to itself, so the dev library takes the whole
bucket the profile names, and setup refuses a bucket holding anything else.
The script empties the bucket itself, with boto3: it aborts every unfinished
multipart upload in it and deletes every object, whatever wrote them, 1,000
keys per request. The first failure stops the script before
the app launches; running it again finishes. A versioned bucket keeps the
deleted objects' earlier versions.

The app does the rest, before any library opens: it removes every library from
this Mac without opening it or reading its store (its directory goes and its
store keys leave the keychain), deletes those libraries' saved restore codes,
deletes ~/.bae, then creates the profile's library and sets its Discogs token,
metadata preferences, and S3 storage. Only a Debug build carries
`--dev-reset`.

The profile (default ~/.config/bae-dev/profile.toml) holds credentials, so it
must be mode 600; this script never prints them. Its fields:

    library_name = "dev"
    identify_automatically = false
    import_when_identified = false

    [s3]
    endpoint = "..."
    region = "..."
    bucket = "..."              # the dev library's own: the script empties it
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
# The app `scripts/test-app.sh` copies out, and its bundle id.
DEFAULT_APP = pathlib.Path.home() / "Applications" / "bae-test" / "bae.app"
BUNDLE_ID = "fm.bae.desktop"


def fail(message: str) -> None:
    print(f"reset-bae: {message}", file=sys.stderr)
    sys.exit(1)


class EmptyingFailed(Exception):
    """A storage request failed or was refused while emptying the bucket."""


def empty_bucket(client, bucket: str) -> tuple[int, int]:
    """Abort every unfinished multipart upload and delete every object in the
    bucket; return how many objects and uploads went. The first failure
    raises, leaving what went before it gone."""
    uploads = 0
    for page in client.get_paginator("list_multipart_uploads").paginate(
        Bucket=bucket
    ):
        for upload in page.get("Uploads", []):
            client.abort_multipart_upload(
                Bucket=bucket, Key=upload["Key"], UploadId=upload["UploadId"]
            )
            uploads += 1

    objects = 0
    batch: list[str] = []

    def delete(keys: list[str]) -> None:
        response = client.delete_objects(
            Bucket=bucket,
            Delete={"Objects": [{"Key": key} for key in keys], "Quiet": True},
        )
        errors = response.get("Errors", [])
        if errors:
            error = errors[0]
            raise EmptyingFailed(
                f"deleting {error.get('Key')!r} failed: {error.get('Code')}: "
                f"{error.get('Message')} ({len(errors)} of {len(keys)} keys in "
                "that request failed)"
            )

    for page in client.get_paginator("list_objects_v2").paginate(Bucket=bucket):
        for entry in page.get("Contents", []):
            batch.append(entry["Key"])
            if len(batch) == 1000:
                delete(batch)
                objects += len(batch)
                batch = []
    if batch:
        delete(batch)
        objects += len(batch)
    return objects, uploads


S3_FIELDS = (
    "endpoint", "region", "bucket", "access_key_id", "secret_access_key",
)


def s3_client(s3: dict):
    """An S3 client for the profile's endpoint, region and keys, configured as
    coven's is: path-style addressing, and checksums only where a request
    requires one. boto3 is imported here, so only a run that empties storage
    needs it."""
    try:
        import boto3
        import botocore.config
        import botocore.exceptions
    except ImportError:
        fail("emptying the storage bucket needs boto3: pip install boto3")
    try:
        return boto3.client(
            "s3",
            endpoint_url=s3["endpoint"],
            region_name=s3["region"],
            aws_access_key_id=s3["access_key_id"],
            aws_secret_access_key=s3["secret_access_key"],
            config=botocore.config.Config(
                s3={"addressing_style": "path"},
                request_checksum_calculation="when_required",
                response_checksum_validation="when_required",
            ),
        )
    except (ValueError, botocore.exceptions.BotoCoreError) as error:
        # botocore names a bad endpoint or region, never the keys.
        fail(f"no S3 client for the profile's storage: {error}")


def empty_profile_bucket(client, s3: dict) -> None:
    """Empty the profile's storage bucket, printing what went; any failure
    fails the script."""
    import botocore.exceptions

    bucket = s3["bucket"]
    print(f"Emptying bucket {bucket!r}...")
    try:
        objects, uploads = empty_bucket(client, bucket)
    except (
        EmptyingFailed,
        botocore.exceptions.BotoCoreError,
        botocore.exceptions.ClientError,
    ) as error:
        fail(f"emptying bucket {bucket!r} stopped: {error}")
    print(f"Deleted {objects} objects and aborted {uploads} unfinished uploads.")


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
        help="the Debug bae.app to launch (default: the one scripts/test-app.sh copies out)",
    )
    args = parser.parse_args()

    profile = read_profile(args.profile)
    if not args.app.is_dir():
        fail(f"no app at {args.app}; build it with scripts/test-app.sh --no-open")
    s3 = profile.get("s3", {})
    for field in S3_FIELDS:
        if not isinstance(s3.get(field), str) or not s3[field].strip():
            fail(f"{args.profile}: s3.{field} is missing or empty")
    if "key_prefix" in s3:
        fail(
            f"{args.profile}: a dev library takes the whole bucket: remove "
            "s3.key_prefix and name a bucket of the library's own"
        )
    client = s3_client(s3)

    print("This will:")
    print(f"  - quit bae ({BUNDLE_ID}) if it is running")
    print(f"  - delete everything in bucket {s3['bucket']!r}, and abort its")
    print("    unfinished uploads")
    print(f"  - launch {args.app}, which will:")
    print("    - remove every bae library on this Mac, with its keychain entries")
    print("      and saved restore codes")
    print(f"    - delete {pathlib.Path.home() / '.bae'}")
    print(f"    - create the library {profile.get('library_name')!r} and connect it")
    print(f"      to bucket {s3['bucket']!r}")
    if input("Continue? [y/N] ").strip().lower() != "y":
        print("Nothing changed.")
        return

    quit_bae()
    empty_profile_bucket(client, s3)
    subprocess.run(
        ["open", str(args.app), "--args", "--dev-reset", str(args.profile)],
        check=True,
    )
    print("bae is resetting; its log is in Console under fm.bae.desktop, DevReset.")


if __name__ == "__main__":
    main()
