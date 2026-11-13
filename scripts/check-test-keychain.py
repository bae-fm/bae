#!/usr/bin/env python3
"""Refuse test code that reaches the person's own keychain or bae directory.

`Coven::new` builds coven's installation over the OS keychain, where it keeps
keys, credentials and saved restore codes: the person's login keychain,
iCloud Keychain (which reaches every device on their Apple account) or their
phone's Block Store. `BridgeHost::new` is the host the apps start over it. A
test, a bench, an example or a test helper keeps its keys in an in-memory
keychain instead (`Coven::in_memory`, `BridgeHost::with_test_keyring`). `dirs::home_dir` is the person's own home, whose
`~/.bae` holds their libraries; test code puts its bae directory under a
temporary one. This check refuses each of those names in test code:

- Rust: the code of every workspace `test`, `bench` and `example` target;
  code under a `cfg` that holds only in a test build, one that needs `test`
  or a `test-*` feature (`test-utils`, `test-keyring`); and every module file
  such code declares, as `rust_modules` reaches them from Cargo's targets.
- Swift: the sources of each XcodeGen project's test bundles.
- Kotlin: Gradle's test source sets (`src/test*`, `src/androidTest*`).
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
from pathlib import Path

from rust_modules import reached_files
from source_tokens import Token, tokenize

# The Rust names that reach the person's keychain or home: two tokens around
# `::`.
RUST_NAMES = {
    ("Coven", "new"),
    ("BridgeHost", "new"),
    ("dirs", "home_dir"),
}
# What a fake host is made from in each app's bindings: no keychain behind it.
SWIFT_HANDLE_LABELS = {"unsafeFromHandle", "noHandle"}
KOTLIN_HANDLE_ARGUMENTS = {"NoHandle"}
KOTLIN_TEST_SOURCE_SETS = ("test", "androidTest")
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


def rust_uses(tokens: list[Token]) -> list[tuple[int, str]]:
    """Each use of a name in `RUST_NAMES`: its first token's index, and the
    name. A use list (`dirs::{data_dir, home_dir}`) names one too."""
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


def staged_crate_directories(root: Path, names: set[str]) -> set[Path]:
    """The directories of the workspace crates that hold one of `names`."""
    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root,
            check=True,
            capture_output=True,
        ).stdout
    )
    directories = {Path(package["manifest_path"]).parent.resolve() for package in metadata["packages"]}
    return {
        directory
        for directory in directories
        for name in names
        if (root / name).resolve().is_relative_to(directory)
    }


def check_rust(root: Path, names: set[str] | None) -> list[str]:
    crates = None if names is None else staged_crate_directories(root, names)
    violations = [
        f"{path.relative_to(root.resolve()).as_posix()}:{file.tokens[position].line}: {name}"
        for path, file in reached_files(root, crates).items()
        if names is None or path.relative_to(root.resolve()).as_posix() in names
        for position, name in rust_uses(file.tokens)
        if file.in_test(position)
    ]
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


def check_apps(root: Path, files: list[str], names: set[str] | None) -> list[str]:
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
        if names is not None and relative not in names:
            continue
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


def check(root: Path, names: list[str] | None = None) -> list[str]:
    """The violations in the whole repository, or in the files `names` alone
    when it is not empty. With `names`, the Rust read is limited to the targets
    of the crates holding a named file (a crate's modules are files of its own
    directory), and the Rust files are read only when a named file is Rust. The
    test source folders of Swift, the test source sets of Kotlin and the test
    projects of C# are found from every tracked file either way."""
    only = set(names) if names else None
    rust = [] if only is not None and not any(n.endswith(".rs") for n in only) else check_rust(root, only)
    return rust + check_apps(root, listed_files(root), only)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="repository root",
    )
    parser.add_argument(
        "files",
        nargs="*",
        help="repo-relative files to check; every tracked file when none",
    )
    args = parser.parse_args()
    violations = check(args.root, args.files)
    for violation in violations:
        print(violation)
    if violations:
        print(
            "Test code keeps its keys in an in-memory keychain "
            "(Coven::in_memory, BridgeHost::with_test_keyring) and its bae "
            "directory under a temporary home, never the person's own "
            "keychain or home.",
            file=sys.stderr,
        )
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
