#!/usr/bin/env python3
"""Keep the bridge's fast values inside each app's attach API.

Values core sends many times a second reach an app only in the bridge's
`KeyedValueStream`, and each app reads them only in its attach API, which
hands each value to the native view drawing its key once per display frame.
So no other app file may touch the stream: not its type, not the bridge
calls that make one, not the value enum it holds, and not
the test writer that stands in for core. Each app names its attach API's files
in `APPS`. The bridge's signatures and types say which calls open a stream,
including calls that open several together.

Code is read as tokens: a name in a comment or a string is not a use, and a
name interpolated into a string is.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

from source_tokens import Token, bracket_pairs, tokenize


@dataclass(frozen=True)
class AppSources:
    language: str
    suffix: str
    roots: tuple[str, ...]
    # The attach API files that may touch the stream.
    attach_files: tuple[str, ...] = ()
    # Paths under the roots that are not the app's own code: tests, and the
    # bindings the bridge generates.
    skipped: tuple[str, ...] = ()


APPS = (
    AppSources(
        "swift",
        ".swift",
        ("BaeKit/Sources", "bae-macos/bae/bae", "bae-ios/bae/bae"),
        (
            "BaeKit/Sources/BaeKit/Services/Store/LatestValues.swift",
            "BaeKit/Sources/BaeKit/Services/Store/DownloadStore+LatestFeed.swift",
        ),
    ),
    AppSources(
        "kotlin",
        ".kt",
        ("bae-android/app/src/main", "bae-android/fastvalues/src/main"),
        ("bae-android/fastvalues/src/main/java/fm/bae/fastvalues/LatestValues.kt",),
    ),
    # The Avalonia skeleton draws no fast value yet: nothing of it may touch
    # the stream until its attach API does.
    AppSources(
        "csharp",
        ".cs",
        ("bae-avalonia",),
        skipped=("bae-avalonia/bae-avalonia.Tests", "bae-avalonia/obj", "bae-avalonia/bin"),
    ),
)

# The bridge's shape for fast values: the stream and the value it holds. A
# bridge call that hands out a stream is found from its signature.
STREAM_NAMES = (
    "KeyedValueStream",
    "BridgeFastValue",
)
# What a test writes a stream through in place of core.
TEST_WRITER_NAMES = ("LatestTestWriter",)
TEST_WRITER_MEMBERS = ("written",)

BRIDGE_ROOT = "bae-bridge/src"


def camel_case(name: str) -> str:
    head, *rest = name.split("_")
    return head + "".join(part[:1].upper() + part[1:] for part in rest)


def is_generated(relative: str) -> bool:
    """The bindings uniffi generates into an app's tree."""
    return Path(relative).name.startswith("bae_bridge")


def bridge_stream_calls(root: Path) -> tuple[set[str], list[str]]:
    """The generated names of calls that return a stream directly or in a
    record or enum, and the stream names the bridge no longer declares."""
    signatures: list[tuple[str, set[str]]] = []
    declared: set[str] = set()
    stream_types = {"KeyedValueStream"}
    for path in sorted((root / BRIDGE_ROOT).rglob("*.rs")):
        if path.name.endswith("_tests.rs"):
            continue
        tokens = tokenize(path.read_text(), "rust")
        pairs = bracket_pairs(tokens)
        for index, token in enumerate(tokens):
            if token.kind != "id":
                continue
            following = tokens[index + 1] if index + 1 < len(tokens) else None
            if token.text in {"struct", "enum", "trait"} and following is not None:
                declared.add(following.text)
                body = index + 2
                if (
                    token.text in {"struct", "enum"}
                    and body < len(tokens)
                    and tokens[body].text == "{"
                ):
                    fields = tokens[body + 1 : pairs[body]]
                    if any(part.kind == "id" and part.text == "KeyedValueStream" for part in fields):
                        stream_types.add(following.text)
            # Only what the bridge exports: a `pub fn`, not a crate's own.
            exported = index > 0 and tokens[index - 1].text == "pub" or (
                index > 1 and tokens[index - 1].text == "async" and tokens[index - 2].text == "pub"
            )
            if token.text != "fn" or following is None or not exported:
                continue
            open_paren = index + 2
            if open_paren >= len(tokens) or tokens[open_paren].text != "(":
                continue
            close_paren = pairs[open_paren]
            signature_end = close_paren + 1
            while signature_end < len(tokens) and tokens[signature_end].text not in {"{", ";"}:
                signature_end += 1
            names = {
                tokens[i].text
                for i in range(open_paren, signature_end)
                if tokens[i].kind == "id"
            }
            signatures.append((following.text, names))
    calls = {camel_case(name) for name, names in signatures if names & stream_types}
    missing = [name for name in STREAM_NAMES if name not in declared]
    return calls, missing


def app_files(root: Path, app: AppSources) -> list[str]:
    files: list[str] = []
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


def touches(tokens: list[Token], calls: set[str]) -> list[tuple[int, str]]:
    """Each place `tokens` touch the stream, with what they touch."""
    found: list[tuple[int, str]] = []
    for index, token in enumerate(tokens):
        if token.kind != "id":
            continue
        previous = tokens[index - 1] if index > 0 else None
        after_dot = previous is not None and previous.text in {".", "?."}
        if token.text in STREAM_NAMES:
            found.append((token.line, f"the bridge's {token.text}"))
        elif token.text in TEST_WRITER_NAMES or (
            after_dot and token.text in TEST_WRITER_MEMBERS
        ):
            found.append((token.line, f"the test writer {token.text}"))
        elif after_dot and token.text in calls:
            found.append((token.line, f"the bridge call {token.text}"))
    return found


def check(root: Path) -> list[str]:
    calls, missing = bridge_stream_calls(root)
    violations = [
        f"{BRIDGE_ROOT}: declares no {name}; the boundary names what the "
        "bridge no longer has"
        for name in missing
    ]
    if not calls:
        violations.append(
            f"{BRIDGE_ROOT}: no bridge call hands out a KeyedValueStream"
        )
    for app in APPS:
        attach_touched: set[str] = set()
        attach_api = ", ".join(app.attach_files) or "an attach API"
        for relative in app_files(root, app):
            tokens = tokenize((root / relative).read_text(), app.language)
            found = touches(tokens, calls)
            if relative in app.attach_files:
                if found:
                    attach_touched.add(relative)
                continue
            violations.extend(
                f"{relative}:{line}: touches {what}, which only "
                f"{attach_api} may"
                for line, what in found
            )
        for attach_file in app.attach_files:
            if not (root / attach_file).is_file():
                violations.append(f"{attach_file}: the attach API is missing")
            elif attach_file not in attach_touched:
                violations.append(
                    f"{attach_file}: the attach API touches no stream; "
                    "name the file that does"
                )
    return violations


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
            "Fast values reach the screen only through each app's attach API.",
            file=sys.stderr,
        )
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
