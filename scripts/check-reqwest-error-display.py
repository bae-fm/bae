#!/usr/bin/env python3
"""Reject an error variant that holds a reqwest::Error and prints it plainly.

reqwest's own Display text is often only "error sending request for url
(...)": the DNS, TLS or connection failure behind it is a cause further down,
which that text leaves out. A `#[error(...)]` on a variant or struct holding a
`reqwest::Error` therefore prints it through `CauseChain`
(bae-core/src/util/error_text.rs), which walks every cause and cuts each URL to
its scheme and host. `{0}`, `{source}`, `{0:?}` and `#[error(transparent)]`
are all refused: the first two drop the cause, the last two print the whole
URL or forward the same Display.
"""

from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent

ATTRIBUTE = re.compile(r"#\[\s*error\s*\(")
RAW_STRING = re.compile(r'(?<!\w)b?r(?P<hashes>#*)"(?P<body>.*?)"(?P=hashes)', re.DOTALL)
CHAR_LITERAL = re.compile(r"'(?:\\.|[^\\'\n])'")


def tracked_rust_files() -> list[Path]:
    listed = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard", "-z", "--", "*.rs"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout.decode()
    return [ROOT / name for name in listed.split("\0") if name]


def mask_non_code(source: str) -> str:
    """`source` with comments and the insides of string and char literals
    blanked, keeping every offset, so brackets inside them do not count."""
    masked = list(source)
    index = 0
    length = len(source)
    while index < length:
        if source.startswith("//", index):
            end = source.find("\n", index)
            end = length if end == -1 else end
            for position in range(index, end):
                masked[position] = " "
            index = end
        elif source.startswith("/*", index):
            end = source.find("*/", index + 2)
            end = length if end == -1 else end + 2
            for position in range(index, end):
                if source[position] != "\n":
                    masked[position] = " "
            index = end
        elif raw := RAW_STRING.match(source, index):
            for position in range(raw.start("body"), raw.end("body")):
                if source[position] != "\n":
                    masked[position] = " "
            index = raw.end()
        elif char := CHAR_LITERAL.match(source, index):
            for position in range(index + 1, char.end() - 1):
                masked[position] = " "
            index = char.end()
        elif source[index] == '"':
            index += 1
            while index < length and source[index] != '"':
                if source[index] == "\\":
                    masked[index] = " "
                    index += 1
                if index < length and source[index] != "\n":
                    masked[index] = " "
                index += 1
            index += 1
        else:
            index += 1
    return "".join(masked)


def matching(masked: str, opening: int) -> int:
    pairs = {"(": ")", "[": "]", "{": "}"}
    stack = [pairs[masked[opening]]]
    index = opening + 1
    while stack:
        if index >= len(masked):
            raise ValueError("unbalanced brackets")
        character = masked[index]
        if character in pairs:
            stack.append(pairs[character])
        elif character in ")]}":
            if character != stack.pop():
                raise ValueError("mismatched brackets")
        index += 1
    return index - 1


def reqwest_error_names(source: str) -> list[str]:
    names = [r"reqwest::Error\b"]
    for alias in re.findall(r"\breqwest::\{?[^;]*\bError\s+as\s+(\w+)", source):
        names.append(rf"\b{alias}\b")
    if re.search(r"\buse\s+reqwest::Error\s*;", source):
        names.append(r"(?<!::)\bError\b")
    return names


def held_fields(source: str, masked: str, after: int) -> str | None:
    """The fields of the variant or struct the attribute ending at `after`
    belongs to: the text inside its `(...)` or `{...}`, or `None` for a unit
    variant."""
    index = after
    while True:
        rest = masked[index:]
        stripped = rest.lstrip()
        index += len(rest) - len(stripped)
        if stripped.startswith("#"):
            opening = masked.index("[", index)
            index = matching(masked, opening) + 1
            continue
        break
    declaration = re.match(r"(?:pub(?:\([^)]*\))?\s+)?(?:struct\s+)?\w+(?:<[^>]*>)?\s*", masked[index:])
    if declaration is None:
        return None
    index += declaration.end()
    if index < len(masked) and masked[index] in "({":
        return source[index : matching(masked, index) + 1]
    return None


def main() -> int:
    violations: list[str] = []
    for path in tracked_rust_files():
        if not path.is_file():
            continue
        source = path.read_text()
        if "reqwest" not in source:
            continue
        masked = mask_non_code(source)
        names = [re.compile(name) for name in reqwest_error_names(source)]
        for found in ATTRIBUTE.finditer(masked):
            opening = found.end() - 1
            closing = matching(masked, opening)
            attribute = source[found.start() : closing + 1]
            attribute_end = masked.index("]", closing) + 1
            fields = held_fields(source, masked, attribute_end)
            if fields is None or not any(name.search(fields) for name in names):
                continue
            if "CauseChain(" in attribute:
                continue
            line = source.count("\n", 0, found.start()) + 1
            shown = source[found.start() : attribute_end]
            violations.append(f"{path.relative_to(ROOT)}:{line}: {' '.join(shown.split())}")
    if violations:
        print("\n".join(violations))
        print(
            "an error holding a reqwest::Error must print it through "
            "crate::util::error_text::CauseChain, which prints every cause with "
            "its URLs cut to the host",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
