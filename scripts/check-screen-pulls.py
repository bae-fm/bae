#!/usr/bin/env python3
"""Keep every screen value on the one way an app gets it: pull.

A screen's values come from a coven live query or a `KeyedValueStream`, and
the app pulls them (`await next()`). Three things would open another way, and
each is refused:

- A callback interface carrying screen data. The bridge declares callback
  interfaces only for platform services core calls; each is listed in
  `ALLOWED_CALLBACKS` with what it does. Any other is refused, and a listed
  one the bridge no longer declares is a stale list.
- A pull object built outside the bridge's `screens` module (`screens.rs` and
  `screens/`). An object the app pulls is a type with a `next` method; one
  defined elsewhere is built elsewhere, since its fields are its module's. A
  `KeyedValueStream` is defined beside its writer in `keyed_values.rs`, so
  for it the refusal is any construction outside `screens`.
- An app reading a subscription other than through its helper. Each app pulls
  through one helper file (`APPS`); any other `.next(` member call in the
  app's own code is refused, apart from the receivers `OTHER_NEXT_CALLS` lists
  with what they are, none of which is a subscription. In Swift a
  `for await … in <subscription>` pulls through the AsyncSequence's iterator,
  so any `for await` outside the helper is refused the same way, apart from
  the sequences `OTHER_FOR_AWAITS` lists.

Code is read as tokens: a name in a comment or a string is not a use.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

from source_tokens import Token, bracket_pairs, tokenize

BRIDGE_ROOT = "bae-bridge/src"
SCREENS_FILE = "bae-bridge/src/screens.rs"
SCREENS_DIR = "bae-bridge/src/screens/"
STREAM_FILE = "bae-bridge/src/keyed_values.rs"

# The callback interfaces the bridge may declare: platform services core
# calls, none of which carries a value a screen shows.
ALLOWED_CALLBACKS = {
    "ArtworkAnalyzerCallback": (
        "identification asks the platform's image analyzer for the barcodes "
        "and text on a cover, and uses the answer itself"
    ),
    "BridgeAudioFocus": (
        "a phone's playback asks the platform for the speaker before it "
        "starts and gives it back when it stops"
    ),
    "OAuthPresenterCallback": (
        "a phone shows a storage provider's sign-in page and hands back "
        "where it redirected"
    ),
    "BridgeBugReporter": (
        "core hands each bug it finds to the platform's crash reporter, from "
        "startup on"
    ),
}

# The constructions of a stream's values that only `screens` may make.
STREAM_CONSTRUCTORS = ("KeyedValueStream", "StreamValues")


@dataclass(frozen=True)
class AppSources:
    language: str
    suffix: str
    roots: tuple[str, ...]
    # The one file that pulls; `None` for an app with no screens to pull for.
    helper: str | None
    # Paths under the roots that are not the app's own code.
    skipped: tuple[str, ...] = ()


APPS = (
    AppSources(
        "swift",
        ".swift",
        ("BaeKit/Sources", "bae-macos/bae/bae", "bae-ios/bae/bae"),
        "BaeKit/Sources/BaeKit/Services/ScreenSubscription.swift",
    ),
    AppSources(
        "kotlin",
        ".kt",
        (
            "bae-android/app/src/main",
            "bae-android/fastvalues/src/main",
            "bae-android/screensubscription/src/main",
        ),
        "bae-android/screensubscription/src/main/java/fm/bae/screensubscription/ScreenSubscription.kt",
    ),
    # The Avalonia skeleton shows no screen yet: it pulls nothing.
    AppSources(
        "csharp",
        ".cs",
        ("bae-avalonia",),
        None,
        ("bae-avalonia/bae-avalonia.Tests", "bae-avalonia/obj", "bae-avalonia/bin"),
    ),
)

# `.next(` calls in app code whose receiver is not a subscription, by file
# and receiver, with what the receiver is.
OTHER_NEXT_CALLS: dict[tuple[str, str], str] = {
    ("BaeKit/Sources/BaeKit/Services/MediaControlService.swift", "actions"): (
        "the media keys' next-track command"
    ),
}

# `for await` loops in Swift app code whose sequence is not a subscription, by
# file and the sequence's first name, with what the sequence is.
OTHER_FOR_AWAITS: dict[tuple[str, str], str] = {
    ("BaeKit/Sources/BaeKit/Services/DeviceStorageStore.swift", "cancelled"): (
        "a stream that never yields, which holds the task until it is cancelled"
    ),
    ("BaeKit/Sources/BaeKit/Services/MembershipStore.swift", "cancelled"): (
        "a stream that never yields, which holds the task until it is cancelled"
    ),
    ("bae-macos/bae/bae/Services/LibraryNameStore.swift", "cancelled"): (
        "a stream that never yields, which holds the task until it is cancelled"
    ),
    (
        "bae-macos/bae/bae/Views/Settings/Library/SyncStatusBlock.swift",
        "NotificationCenter",
    ): "the system's notifications, awaited for one",
}


def is_generated(relative: str) -> bool:
    """The bindings uniffi generates into an app's tree."""
    return Path(relative).name.startswith("bae_bridge")


def in_screens(relative: str) -> bool:
    return relative == SCREENS_FILE or relative.startswith(SCREENS_DIR)


def bridge_files(root: Path) -> list[tuple[str, list[Token]]]:
    files = []
    for path in sorted((root / BRIDGE_ROOT).rglob("*.rs")):
        if path.name.endswith("_tests.rs"):
            continue
        relative = path.relative_to(root).as_posix()
        files.append((relative, tokenize(path.read_text(), "rust")))
    return files


def callback_interfaces(tokens: list[Token]) -> list[tuple[int, str]]:
    """Each trait the tokens export as a callback interface, with its line."""
    found = []
    for index in range(len(tokens) - 6):
        texts = [token.text for token in tokens[index : index + 7]]
        if texts[:5] != ["#", "[", "uniffi", "::", "export"] or texts[5] != "(":
            continue
        if texts[6] not in {"callback_interface", "with_foreign"}:
            continue
        cursor = index + 7
        while cursor < len(tokens) and tokens[cursor].text != "trait":
            cursor += 1
        if cursor + 1 < len(tokens):
            found.append((tokens[cursor + 1].line, tokens[cursor + 1].text))
    return found


def pulled_types(tokens: list[Token]) -> list[tuple[int, str]]:
    """Each type whose `impl` block in the tokens has a public `next`
    method."""
    pairs = bracket_pairs(tokens)
    found = []
    for index, token in enumerate(tokens):
        if token.text != "impl" or token.kind != "id":
            continue
        cursor = index + 1
        while cursor < len(tokens) and tokens[cursor].text != "{":
            if tokens[cursor].text == ";":
                break
            cursor += 1
        if cursor >= len(tokens) or tokens[cursor].text != "{":
            continue
        # The implemented type: the last name before the block, or before a
        # `for`'s trait.
        header = [t for t in tokens[index + 1 : cursor] if t.kind == "id"]
        if "for" in [t.text for t in header]:
            continue
        if not header:
            continue
        name = header[-1].text
        closing = pairs[cursor]
        depth = 0
        for inner in range(cursor + 1, closing):
            text = tokens[inner].text
            if text == "{":
                depth += 1
            elif text == "}":
                depth -= 1
            elif (
                depth == 0
                and text == "fn"
                and inner + 1 < closing
                and tokens[inner + 1].text == "next"
                and exported(tokens, inner)
            ):
                found.append((tokens[inner + 1].line, name))
                break
    return found


def exported(tokens: list[Token], fn_index: int) -> bool:
    """Whether the `fn` at `fn_index` is `pub`, as everything uniffi exports
    is: `pub fn` or `pub async fn`."""
    before = [token.text for token in tokens[max(0, fn_index - 2) : fn_index]]
    return before[-1:] == ["pub"] or before == ["pub", "async"]


def stream_constructions(tokens: list[Token]) -> list[tuple[int, str]]:
    found = []
    for index in range(len(tokens) - 2):
        if (
            tokens[index].text in STREAM_CONSTRUCTORS
            and tokens[index + 1].text == "::"
            and tokens[index + 2].text == "new"
        ):
            found.append((tokens[index].line, tokens[index].text))
    return found


def check_bridge(root: Path) -> list[str]:
    violations = []
    declared = set()
    for relative, tokens in bridge_files(root):
        for line, name in callback_interfaces(tokens):
            declared.add(name)
            if name not in ALLOWED_CALLBACKS:
                violations.append(
                    f"{relative}:{line}: callback interface {name} is not a "
                    "listed platform service; a screen pulls its values"
                )
        if in_screens(relative):
            continue
        for line, name in pulled_types(tokens):
            if relative == STREAM_FILE and name == "KeyedValueStream":
                continue
            violations.append(
                f"{relative}:{line}: {name} is pulled from but defined "
                "outside the screens module"
            )
        if relative == STREAM_FILE:
            continue
        for line, name in stream_constructions(tokens):
            violations.append(
                f"{relative}:{line}: builds a {name} outside the screens module"
            )
    for name in sorted(set(ALLOWED_CALLBACKS) - declared):
        violations.append(
            f"{BRIDGE_ROOT}: declares no callback interface {name}; the list "
            "names what the bridge no longer has"
        )
    return violations


def app_files(root: Path, app: AppSources) -> list[str]:
    files = []
    for source_root in app.roots:
        for path in sorted((root / source_root).rglob(f"*{app.suffix}")):
            relative = path.relative_to(root).as_posix()
            if is_generated(relative) or any(
                relative == skipped or relative.startswith(f"{skipped}/")
                for skipped in app.skipped
            ):
                continue
            files.append(relative)
    return files


def next_calls(tokens: list[Token]) -> list[tuple[int, str]]:
    """Each `.next(` member call, with its receiver's last name."""
    found = []
    for index in range(1, len(tokens) - 1):
        token = tokens[index]
        if token.text not in {"next", "Next"} or tokens[index + 1].text != "(":
            continue
        if tokens[index - 1].text not in {".", "?."}:
            continue
        receiver = tokens[index - 2].text if index > 1 else ""
        found.append((token.line, receiver))
    return found


def for_awaits(tokens: list[Token]) -> list[tuple[int, str]]:
    """Each `for await` loop, with the first name of the sequence it reads."""
    found = []
    for index, token in enumerate(tokens):
        if token.text != "for" or token.kind != "id":
            continue
        cursor = index + 1
        if tokens[cursor].text == "try":
            cursor += 1
        if tokens[cursor].text != "await":
            continue
        while cursor < len(tokens) and not (
            tokens[cursor].kind == "id" and tokens[cursor].text == "in"
        ):
            cursor += 1
        if cursor + 1 < len(tokens):
            found.append((token.line, tokens[cursor + 1].text))
    return found


def check_apps(root: Path) -> list[str]:
    violations = []
    for app in APPS:
        helper_pulls = False
        for relative in app_files(root, app):
            tokens = tokenize((root / relative).read_text(), app.language)
            if relative == app.helper:
                helper_pulls = any(
                    token.text == "next" and following.text == "("
                    for token, following in zip(tokens, tokens[1:])
                )
                continue
            calls = next_calls(tokens)
            for line, receiver in calls:
                if (relative, receiver) in OTHER_NEXT_CALLS:
                    continue
                violations.append(
                    f"{relative}:{line}: pulls {receiver}.next() itself; "
                    f"a screen reads through {app.helper or 'its helper'}"
                )
            if app.language != "swift":
                continue
            for line, receiver in for_awaits(tokens):
                if (relative, receiver) in OTHER_FOR_AWAITS:
                    continue
                violations.append(
                    f"{relative}:{line}: iterates {receiver} with for await "
                    f"itself; a screen reads through {app.helper}"
                )
        if app.helper is not None:
            if not (root / app.helper).is_file():
                violations.append(f"{app.helper}: the helper is missing")
            elif not helper_pulls:
                violations.append(
                    f"{app.helper}: the helper pulls nothing; name the file "
                    "that does"
                )
    for (relative, receiver) in sorted(OTHER_NEXT_CALLS):
        path = root / relative
        language = next(app.language for app in APPS if relative.endswith(app.suffix))
        if not path.is_file() or receiver not in {
            called for _, called in next_calls(tokenize(path.read_text(), language))
        }:
            violations.append(
                f"{relative}: makes no {receiver}.next() call; the list names "
                "what the app no longer has"
            )
    for (relative, receiver) in sorted(OTHER_FOR_AWAITS):
        path = root / relative
        if not path.is_file() or receiver not in {
            read for _, read in for_awaits(tokenize(path.read_text(), "swift"))
        }:
            violations.append(
                f"{relative}: has no for await over {receiver}; the list "
                "names what the app no longer has"
            )
    return violations


def check(root: Path) -> list[str]:
    return check_bridge(root) + check_apps(root)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="repository root",
    )
    args = parser.parse_args()
    violations = check(args.root)
    for violation in violations:
        print(violation)
    if violations:
        print(
            "A screen's values reach the app only as pulls of objects built "
            "in the bridge's screens module, read through each app's helper.",
            file=sys.stderr,
        )
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
