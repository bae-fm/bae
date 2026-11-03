#!/usr/bin/env python3
"""Refuse test code that reaches the person's own keychain.

`Keychain::System` keeps coven's keys, credentials and saved restore codes in
the OS keychain: the person's login keychain, iCloud Keychain (which reaches
every device on their Apple account) or their phone's Block Store.
`BridgeHost::new` is the host the apps start over it. A test, a bench or a
test helper keeps its keys in an in-memory keychain instead
(`Keychain::in_memory()`, `BridgeHost::with_test_keyring`). This check
refuses either name in test code:

- Rust: the code of every workspace `test` and `bench` target; code under a
  `cfg` that holds only in a test build, one that needs `test` or a `test-*`
  feature (`test-utils`, `test-keyring`); and every module file such code
  declares, found by following `mod` declarations from Cargo's targets.
- Swift: the sources of each XcodeGen project's test bundles.
- Kotlin: Gradle's test source sets (`src/test*`, `src/androidTest*`,
  `src/screenshotTest*`).
- C#: the projects that reference `Microsoft.NET.Test.Sdk`.

The apps call `BridgeHost::new` through their generated bindings as the
`BridgeHost(...)` constructor; a fake made from a handle
(`BridgeHost(unsafeFromHandle:)`, `BridgeHost(NoHandle)`) is no host and
passes.

Code is read as tokens: a name in a comment or a string is not a use.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

from source_tokens import Token, bracket_pairs, tokenize

# The Rust names that reach the person's keychain: two tokens around `::`.
RUST_NAMES = {("Keychain", "System"), ("BridgeHost", "new")}
# What a fake host is made from in each app's bindings: no keychain behind it.
SWIFT_HANDLE_LABELS = {"unsafeFromHandle", "noHandle"}
KOTLIN_HANDLE_ARGUMENTS = {"NoHandle"}
KOTLIN_TEST_SOURCE_SETS = ("test", "androidTest", "screenshotTest")
XCODEGEN_TEST_TYPES = {"bundle.unit-test", "bundle.ui-testing"}


def listed_files(root: Path) -> list[str]:
    listed = subprocess.run(
        ["git", "ls-files", "-co", "--exclude-standard", "-z"],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout.decode()
    return sorted(name for name in listed.split("\0") if name)


# Rust


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


def rust_uses(tokens: list[Token]) -> list[tuple[int, str]]:
    """Each use of a name in `RUST_NAMES`: its first token's index, and the
    name. A use list (`Keychain::{InMemory, System}`) names one too."""
    uses = []
    for index in range(len(tokens) - 2):
        if tokens[index + 1].text != "::":
            continue
        following = tokens[index + 2].text
        if (tokens[index].text, following) in RUST_NAMES:
            uses.append((index, f"{tokens[index].text}::{following}"))
        elif following == "{":
            listed = set()
            position = index + 3
            while position < len(tokens) and tokens[position].text != "}":
                listed.add(tokens[position].text)
                position += 1
            uses.extend(
                (index, f"{prefix}::{name}")
                for prefix, name in sorted(RUST_NAMES)
                if prefix == tokens[index].text and name in listed
            )
    return uses


def workspace_targets(root: Path) -> list[tuple[Path, bool]]:
    """Every workspace target's root file, and whether it is a test or a
    bench."""
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
        for target in package["targets"]:
            test = any(kind in ("test", "bench") for kind in target["kind"])
            targets.append((Path(target["src_path"]), test))
    return targets


def check_rust(root: Path) -> list[str]:
    violations: list[str] = []
    # Each file once, as test code wherever any path to it is. A target's
    # root owns its directory, as a mod.rs does. An included file is read as
    # its own: it may declare no module, whose file would depend on the file
    # including it.
    seen: dict[Path, bool] = {}
    pending = [(path, True, test, False) for path, test in workspace_targets(root)]
    while pending:
        path, owns_directory, test, included = pending.pop()
        path = path.resolve()
        if path in seen and (seen[path] or not test):
            continue
        seen[path] = test
        tokens = tokenize(path.read_text(), "rust")
        read = read_rust(tokens)
        for position, name in rust_uses(tokens):
            if test or read.whole_file_test or any(
                first <= position <= last for first, last in read.test_ranges
            ):
                violations.append(
                    f"{path.relative_to(root.resolve())}:{tokens[position].line}: {name}"
                )
        if included and read.declarations:
            raise ValueError(f"{path}: an included file declares a module")
        for declaration in read.declarations:
            child, child_owns_directory = module_file(path, owns_directory, declaration)
            pending.append(
                (child, child_owns_directory, test or declaration.test, False)
            )
        for inclusion in read.inclusions:
            pending.append(
                (path.parent / inclusion.path, False, test or inclusion.test, True)
            )
    return sorted(set(violations))


# The apps


def xcodegen_test_sources(root: Path, project: str) -> list[str]:
    """The source folders of the test bundles an XcodeGen project lists."""
    sources: list[str] = []
    target_type = None
    target_sources: list[str] = []
    in_targets = in_sources = False

    def finish() -> None:
        if target_type in XCODEGEN_TEST_TYPES:
            sources.extend(target_sources)

    for line in (root / project).read_text().splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if not line.startswith(" "):
            if in_targets:
                finish()
            in_targets = line.rstrip() == "targets:"
            target_type, target_sources, in_sources = None, [], False
            continue
        if not in_targets:
            continue
        if re.match(r"^  \S", line):
            finish()
            target_type, target_sources, in_sources = None, [], False
        elif match := re.match(r"^    type:\s*(\S+)", line):
            target_type = match.group(1)
        elif re.match(r"^    sources:\s*$", line):
            in_sources = True
        elif re.match(r"^    \S", line):
            in_sources = False
        elif in_sources and (match := re.match(r"^      - (?:path:\s*)?(\S+)\s*$", line)):
            target_sources.append(match.group(1))
    if in_targets:
        finish()
    directory = str(Path(project).parent)
    return [f"{directory}/{source}/" for source in sources]


def constructor_calls(tokens: list[Token], allowed_first: set[str]) -> list[Token]:
    """Each `BridgeHost(...)` call whose first argument is not one of
    `allowed_first`."""
    calls = []
    for index in range(len(tokens) - 2):
        if tokens[index].text != "BridgeHost" or tokens[index].kind != "id":
            continue
        opening = index + 1
        if tokens[opening].text == "." and tokens[opening + 1].text == "init":
            opening += 2
        if tokens[opening].text == "(" and tokens[opening + 1].text not in allowed_first:
            calls.append(tokens[index])
    return calls


def check_apps(root: Path, files: list[str]) -> list[str]:
    violations: list[str] = []
    swift_roots = [
        source
        for project in files
        if re.fullmatch(r"[^/]+/[^/]+/project\.yml", project)
        for source in xcodegen_test_sources(root, project)
    ]
    test_projects = [
        str(Path(project).parent) + "/"
        for project in files
        if project.endswith(".csproj")
        and "Microsoft.NET.Test.Sdk" in (root / project).read_text()
    ]
    for relative in files:
        path = root / relative
        if not path.is_file():
            continue
        parts = Path(relative).parts
        if relative.endswith(".swift") and any(relative.startswith(source) for source in swift_roots):
            calls = constructor_calls(tokenize(path.read_text(), "swift"), SWIFT_HANDLE_LABELS)
        elif relative.endswith(".kt") and any(
            part == "src" and index + 1 < len(parts) and parts[index + 1].startswith(KOTLIN_TEST_SOURCE_SETS)
            for index, part in enumerate(parts)
        ):
            calls = constructor_calls(tokenize(path.read_text(), "kotlin"), KOTLIN_HANDLE_ARGUMENTS)
        elif relative.endswith(".cs") and any(relative.startswith(project) for project in test_projects):
            tokens = tokenize(path.read_text(), "csharp")
            calls = [
                token
                for index, token in enumerate(tokens[1:], start=1)
                if token.text == "BridgeHost"
                and tokens[index - 1].text == "new"
                and tokens[index + 1].text == "("
            ]
        else:
            continue
        violations.extend(f"{relative}:{call.line}: BridgeHost(...)" for call in calls)
    return violations


def check(root: Path) -> list[str]:
    return check_rust(root) + check_apps(root, listed_files(root))


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
            "Test code keeps its keys in an in-memory keychain "
            "(Keychain::in_memory(), BridgeHost::with_test_keyring), never "
            "the person's own keychain.",
            file=sys.stderr,
        )
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
