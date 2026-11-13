"""Rust sources as rustc reaches them: every file a workspace target compiles,
found by following `mod` declarations and `include!` from Cargo's targets
with the compiler's file rules, and which of its code exists only in a test
build.

A file is test code when a test, bench or example target compiles it, or
when the code that declares or includes it does; within a file, code under a
`cfg` that holds only in a test build is test code (`cfg_value`).
"""

from __future__ import annotations

import json
import subprocess
from dataclasses import dataclass
from pathlib import Path

from source_tokens import Token, bracket_pairs, tokenize


def cfg_value(tokens: list[Token], start: int, end: int) -> bool | None:
    """The `cfg` predicate in `tokens[start:end]` in a build without tests:
    `test` and every `test-*` feature are off, anything else is unknown
    (`None`). False means the code exists only in a test build."""
    position = start

    def predicate() -> bool | None:
        nonlocal position
        name = tokens[position].text
        position += 1
        if position < end and tokens[position].text == "=":
            value = tokens[position + 1].text.strip('"')
            position += 2
            if name == "feature" and value.startswith("test-"):
                return False
            return None
        if position < end and tokens[position].text == "(":
            position += 1
            values = []
            while tokens[position].text != ")":
                values.append(predicate())
                if tokens[position].text == ",":
                    position += 1
            position += 1
            if name == "not":
                return None if values[0] is None else not values[0]
            if name == "all":
                if False in values:
                    return False
                return None if None in values else True
            if name == "any":
                if True in values:
                    return True
                return None if None in values else False
            raise ValueError(f"unknown cfg predicate {name}() at line {tokens[start].line}")
        return False if name == "test" else None

    value = predicate()
    if position != end:
        raise ValueError(f"unread cfg predicate at line {tokens[start].line}")
    return value


def item_end(tokens: list[Token], pairs: dict[int, int], start: int) -> int:
    """The last token of the item, field, arm or statement starting at
    `start`: its body's closing brace, or the `;` or `,` that ends it."""
    angles = 0
    index = start
    while index < len(tokens):
        text = tokens[index].text
        if tokens[index].kind == "punct":
            if text in ("(", "["):
                index = pairs[index] + 1
                continue
            if text == "{":
                return pairs[index]
            if text in (")", "]", "}"):
                return index - 1
            if text == ";" or (text == "," and angles == 0):
                return index
            if text == "<":
                angles += 1
            elif text == ">" and angles > 0:
                angles -= 1
        index += 1
    return len(tokens) - 1


@dataclass(frozen=True)
class ModDeclaration:
    name: str
    path: str | None  # its `#[path]`
    inline_modules: tuple[str, ...]  # the inline modules it is declared in
    test: bool


@dataclass(frozen=True)
class Inclusion:
    path: str  # its `include!` path, relative to the including file
    test: bool


@dataclass
class RustFile:
    test_ranges: list[tuple[int, int]]
    declarations: list[ModDeclaration]
    inclusions: list[Inclusion]
    whole_file_test: bool


def read_rust(tokens: list[Token]) -> RustFile:
    pairs = bracket_pairs(tokens)
    enclosing: list[int | None] = []  # each token's innermost enclosing `{`
    braces: list[int] = []
    for index, token in enumerate(tokens):
        if token.kind == "punct" and token.text == "}":
            braces.pop()
        enclosing.append(braces[-1] if braces else None)
        if token.kind == "punct" and token.text == "{":
            braces.append(index)

    test_ranges: list[tuple[int, int]] = []
    whole_file_test = False
    # `#[path]` values, keyed by the first token of the item they belong to.
    paths: dict[int, str] = {}
    index = 0
    while index < len(tokens):
        inner = tokens[index + 1].text == "!" if index + 1 < len(tokens) else False
        opening = index + 2 if inner else index + 1
        if (
            tokens[index].text != "#"
            or tokens[index].kind != "punct"
            or opening >= len(tokens)
            or tokens[opening].text != "["
        ):
            index += 1
            continue
        closing = pairs[opening]
        name = tokens[opening + 1].text
        test_only = (
            name == "cfg"
            and cfg_value(tokens, opening + 3, pairs[opening + 2]) is False
        )
        if inner:
            if test_only:
                brace = enclosing[index]
                if brace is None:
                    whole_file_test = True
                else:
                    test_ranges.append((brace, pairs[brace]))
            index = closing + 1
            continue
        start = closing + 1
        while tokens[start].text == "#" and tokens[start + 1].text == "[":
            start = pairs[start + 1] + 1
        if test_only:
            test_ranges.append((index, item_end(tokens, pairs, start)))
        if name == "path" and tokens[opening + 2].text == "=":
            paths[start] = tokens[opening + 3].text[1:-1]
        index = closing + 1

    def in_test(position: int) -> bool:
        return whole_file_test or any(
            first <= position <= last for first, last in test_ranges
        )

    def inline_modules(position: int) -> tuple[str, ...]:
        names: list[str] = []
        brace = enclosing[position]
        while brace is not None:
            if brace >= 2 and tokens[brace - 2].text == "mod":
                names.append(tokens[brace - 1].text)
            brace = enclosing[brace]
        return tuple(reversed(names))

    declarations = []
    for index, token in enumerate(tokens[:-2]):
        if not (
            token.kind == "id"
            and token.text == "mod"
            and tokens[index + 1].kind == "id"
            and tokens[index + 2].text == ";"
        ):
            continue
        start = index
        if tokens[start - 1].text == ")" and tokens[pairs[start - 1] - 1].text == "pub":
            start = pairs[start - 1] - 1
        elif tokens[start - 1].text == "pub":
            start -= 1
        declarations.append(
            ModDeclaration(
                name=tokens[index + 1].text,
                path=paths.get(start),
                inline_modules=inline_modules(index),
                test=in_test(index),
            )
        )
    inclusions = [
        Inclusion(tokens[index + 3].text[1:-1], in_test(index))
        for index in range(len(tokens) - 3)
        if tokens[index].text == "include"
        and tokens[index + 1].text == "!"
        and tokens[index + 2].text == "("
        and tokens[index + 3].kind == "str"
    ]
    return RustFile(test_ranges, declarations, inclusions, whole_file_test)


def module_file(declaring: Path, owns_directory: bool, declaration: ModDeclaration) -> tuple[Path, bool]:
    """The file a `mod name;` loads, and whether that file owns its
    directory (a `mod.rs`, or one loaded through `#[path]`) for the modules
    it declares in turn."""
    if declaring.name in ("lib.rs", "main.rs", "mod.rs") or owns_directory:
        directory = declaring.parent
    else:
        directory = declaring.parent / declaring.stem
    if declaration.path is not None:
        if declaration.inline_modules:
            directory = directory.joinpath(*declaration.inline_modules)
        else:
            directory = declaring.parent
        return directory / declaration.path, True
    directory = directory.joinpath(*declaration.inline_modules)
    for candidate in (directory / f"{declaration.name}.rs", directory / declaration.name / "mod.rs"):
        if candidate.is_file():
            return candidate, False
    raise ValueError(f"{declaring}: no file for mod {declaration.name}")


def workspace_targets(root: Path, crates: set[Path] | None = None) -> list[tuple[Path, bool]]:
    """Every workspace target's root file, and whether it is a test, a bench
    or an example: code that never ships. `crates`, the resolved directories
    of some workspace crates, limits it to their targets."""
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root,
            check=True,
            capture_output=True,
        ).stdout
    )
    targets = []
    for package in metadata["packages"]:
        if crates is not None and Path(package["manifest_path"]).parent.resolve() not in crates:
            continue
        for target in package["targets"]:
            test = any(kind in ("test", "bench", "example") for kind in target["kind"])
            targets.append((Path(target["src_path"]), test))
    return targets


@dataclass
class ReachedFile:
    tokens: list[Token]
    read: RustFile
    # A test, bench or example target compiles it, or test code declares or
    # includes it.
    test: bool

    def in_test(self, position: int) -> bool:
        """Whether the token at `position` is test code."""
        return (
            self.test
            or self.read.whole_file_test
            or any(first <= position <= last for first, last in self.read.test_ranges)
        )


def reached_files(root: Path, crates: set[Path] | None = None) -> dict[Path, ReachedFile]:
    """Every file the workspace's targets compile (those of `crates`, the
    resolved directories of some workspace crates, when given: a crate's
    modules are files of its own directory), by its resolved path: test
    code throughout when any test code declares or includes it. A target's
    root owns its directory, as a mod.rs does. An included file is read as
    its own: it may declare no module, whose file would depend on the file
    including it."""
    reached: dict[Path, ReachedFile] = {}
    pending = [(path, True, test, False) for path, test in workspace_targets(root, crates)]
    while pending:
        path, owns_directory, test, included = pending.pop()
        path = path.resolve()
        if path in reached and (reached[path].test or not test):
            continue
        tokens = tokenize(path.read_text(), "rust")
        read = read_rust(tokens)
        reached[path] = ReachedFile(tokens, read, test)
        if included and read.declarations:
            raise ValueError(f"{path}: an included file declares a module")
        for declaration in read.declarations:
            child, child_owns_directory = module_file(path, owns_directory, declaration)
            pending.append((child, child_owns_directory, test or declaration.test, False))
        for inclusion in read.inclusions:
            pending.append((path.parent / inclusion.path, False, test or inclusion.test, True))
    return reached
