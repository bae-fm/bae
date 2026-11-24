"""Lease one of the two capture areas on the macOS shot display."""

import fcntl
import os
from pathlib import Path
import subprocess
import sys


def lock(file, mode):
    fcntl.flock(file.fileno(), mode)


def lease_slot(root):
    # Wait outside the startup lock so a capture can release its lease.
    while True:
        for index in range(2):
            slot = (root / f"slot-{index}").open("a")
            try:
                lock(slot, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                slot.close()
            else:
                return index, slot
        with (root / "slot-0").open("a") as waiting:
            lock(waiting, fcntl.LOCK_EX)


def display_id(root, keeper, startup):
    identity = root / "display-id"
    with (root / "owner").open("a") as owner:
        try:
            lock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return int(identity.read_text())

        (root / "result").unlink(missing_ok=True)
        # The keeper inherits the locks before it adds the display. The startup
        # lock stays held until it publishes the ID, even if this client dies.
        process = subprocess.Popen(
            [keeper, str(root), str(startup.fileno()), str(owner.fileno())],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            pass_fds=(startup.fileno(), owner.fileno()),
            start_new_session=True,
        )
        with process.stdout:
            ready = process.stdout.readline().decode().strip()
        if not ready.isdecimal():
            process.wait()
            raise RuntimeError(ready or "The shot display keeper exited before it was ready.")
        return int(ready)


def release_display(root, active):
    # A new lease cannot join while the last lease waits for the display to go
    # away. Other active leases also release under this lock.
    with (root / "startup").open("a") as startup:
        lock(startup, fcntl.LOCK_EX)
        active.close()
        with (root / "active").open("a") as probe:
            try:
                lock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                try:
                    lock(probe, fcntl.LOCK_SH | fcntl.LOCK_NB)
                except BlockingIOError:
                    pass  # The keeper is removing the display.
                else:
                    with (root / "owner").open("a") as owner:
                        try:
                            lock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
                        except BlockingIOError:
                            return  # Another capture still holds the display.
        with (root / "owner").open("a") as owner:
            lock(owner, fcntl.LOCK_EX)
        failure = (root / "result").read_text().strip()
        if failure:
            raise RuntimeError(failure)


def hold_display(keeper):
    # Darwin gives GUI and SSH processes of the same user the same directory.
    temporary = subprocess.check_output(
        ["/usr/bin/getconf", "DARWIN_USER_TEMP_DIR"], text=True
    ).strip()
    root = Path(temporary) / "bae-shot-display"
    root.mkdir(mode=0o700, exist_ok=True)
    index, slot = lease_slot(root)
    with slot, (root / "active").open("a") as active:
        with (root / "startup").open("a") as startup:
            lock(startup, fcntl.LOCK_EX)
            lock(active, fcntl.LOCK_SH)
            display = display_id(root, keeper, startup)
        # The capture process owns the pipe's writing end. The kernel closes
        # it on exit, including when Xcode stops the capture.
        print(f"{display} {index}", flush=True)
        os.close(sys.stdout.fileno())
        try:
            sys.stdin.buffer.read()
        finally:
            release_display(root, active)


if __name__ == "__main__":
    try:
        hold_display(sys.argv[1])
    except (OSError, RuntimeError, ValueError) as error:
        sys.exit(f"The shot capture could not lease a display. {error}")
