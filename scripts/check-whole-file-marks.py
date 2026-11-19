#!/usr/bin/env python3
"""Refuse a lint mark that covers a whole file or module.

A lint mark (`#[expect(lint, reason = "...")]`) sits on the one function or
item that is the home of what the lint forbids, and its reason says why that
code may do it. A mark that covers a module switches the lint off for every
item in it, including code added there later, so this check refuses one:

- an inner `#![allow(..)]` or `#![expect(..)]` at the top of a file or of an
  inline `mod name { .. }` block;
- an outer `#[allow(..)]` or `#[expect(..)]` on a `mod` item;
- either one written inside `#[cfg_attr(..)]`.

An inner mark at the top of a function body covers that one function and
passes. Marks that turn a lint on (`deny`, `warn`, `forbid`) pass.

Code is read as tokens: a mark in a comment or a string is not a mark. With no
arguments the check reads every Rust file git lists; with arguments, the named
files.
"""

from __future__ import annotations

import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

from source_tokens import Token, bracket_pairs, tokenize

MARK_LEVELS = {"allow", "expect"}

@dataclass(frozen=True)
class Mark:
    line: int
    lints: tuple[str, ...]


def mark_lints(tokens: list[Token], pairs: dict[int, int], opening: int) -> list[str]:
    """The lints an attribute between `[` at `opening` and its `]` switches
    off, when it is `allow(..)` or `expect(..)`, directly or inside
    `cfg_attr(..)`."""
    closing = pairs[opening]
    first = opening + 1
    if first >= closing or tokens[first].kind != "id":
        return []
    if tokens[first].text in MARK_LEVELS:
        return listed_lints(tokens, pairs, first + 1)
    if tokens[first].text != "cfg_attr" or tokens[first + 1].text != "(":
        return []
    lints = []
    index = first + 2
    end = pairs[first + 1]
    while index < end:
        token = tokens[index]
        if token.kind == "id" and token.text in MARK_LEVELS and tokens[index + 1].text == "(":
            lints.extend(listed_lints(tokens, pairs, index + 1))
            index = pairs[index + 1] + 1
            continue
        if token.text in "([{" and token.kind == "punct":
            index = pairs[index] + 1
            continue
        index += 1
    return lints


def listed_lints(tokens: list[Token], pairs: dict[int, int], opening: int) -> list[str]:
    """The lint paths in the parentheses at `opening`, without the reason."""
    if tokens[opening].text != "(":
        return []
    lints = []
    current: list[str] = []
    for token in tokens[opening + 1 : pairs[opening]] + [Token("punct", ",", 0, False)]:
        if token.kind == "punct" and token.text == ",":
            if current and current[0] != "reason":
                lints.append("".join(current))
            current = []
        else:
            current.append(token.text)
    return lints


def item_after_attributes(tokens: list[Token], pairs: dict[int, int], index: int) -> str:
    """The keyword of the item that the attributes from `index` on belong to,
    past its other attributes and its visibility."""
    while index < len(tokens):
        token = tokens[index]
        if token.text == "#" and index + 1 < len(tokens) and tokens[index + 1].text == "[":
            index = pairs[index + 1] + 1
        elif token.kind == "id" and token.text == "pub":
            index += 1
            if index < len(tokens) and tokens[index].text == "(":
                index = pairs[index] + 1
        else:
            return token.text
    return ""


def whole_module_marks(source: str) -> list[Mark]:
    """Each mark in a Rust source that covers a whole file or module."""
    tokens = tokenize(source, "rust")
    pairs = bracket_pairs(tokens)
    # For each open `{`, whether it opens an inline module's body.
    scopes: list[bool] = []
    marks = []
    for index, token in enumerate(tokens):
        if token.kind != "punct":
            continue
        if token.text == "{":
            scopes.append(
                index >= 2
                and tokens[index - 2].kind == "id"
                and tokens[index - 2].text == "mod"
                and tokens[index - 1].kind == "id"
            )
        elif token.text == "}":
            scopes.pop()
        elif token.text == "#" and index + 2 < len(tokens):
            if tokens[index + 1].text == "!" and tokens[index + 2].text == "[":
                lints = mark_lints(tokens, pairs, index + 2)
                if lints and (not scopes or scopes[-1]):
                    marks.append(Mark(token.line, tuple(lints)))
            elif tokens[index + 1].text == "[":
                lints = mark_lints(tokens, pairs, index + 1)
                if lints and item_after_attributes(tokens, pairs, pairs[index + 1] + 1) == "mod":
                    marks.append(Mark(token.line, tuple(lints)))
    return marks


def file_violations(name: str, source: str) -> list[str]:
    """What is wrong with one file's whole-module marks."""
    return [
        f"{name}:{mark.line}: this mark switches off {', '.join(mark.lints)} for a "
        "whole module; put #[expect(<lint>, reason = \"...\")] on the one "
        "function or item that does what the lint forbids"
        for mark in whole_module_marks(source)
    ]


def listed_rust_files(root: Path) -> list[str]:
    listed = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "*.rs"],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout.decode()
    return sorted(name for name in listed.split("\0") if name)


def violations(root: Path, names: list[str] | None = None) -> list[str]:
    """The violations in the named files, or in every Rust file git lists
    when none are named."""
    if names is None:
        names = listed_rust_files(root)
    found = []
    for name in names:
        path = root / name
        if not name.endswith(".rs") or not path.is_file():
            continue
        found.extend(file_violations(Path(name).as_posix(), path.read_text()))
    return found


def main() -> int:
    found = violations(Path.cwd(), sys.argv[1:] or None)
    if found:
        print("Whole-module lint marks:", file=sys.stderr)
        for violation in found:
            print(f"  {violation}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
