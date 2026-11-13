#!/usr/bin/env python3
"""Reject exported async Rust methods that run on a foreign caller stack."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parent.parent


def reject_flush_on_worker_thread(root: Path, violations: list[str]) -> None:
    """The Avalonia diagnostics flush is an async bridge export, which runs on
    the bridge's runtime: a `Task.Run(...)` around it puts it on a second
    worker thread for nothing."""
    path = root / "bae-avalonia" / "BaeLogger.cs"
    masked = mask_non_code(path.read_text())
    for found in re.finditer(r"\bTask\s*\.\s*Run\s*\(", masked):
        opening = found.end() - 1
        closing = matching_bracket(masked, opening, "(", ")")
        if re.search(r"\bFlushDiagnostics\b", masked[opening:closing]):
            violations.append(
                "Avalonia diagnostics flush duplicates the bridge runtime dispatch"
            )
            return


def reject_detached_async_bridge_calls(root: Path, violations: list[str]) -> None:
    pattern = re.compile(r"Task\.detached(?:\([^)]*\))?\s*\{")
    swift_roots = ("BaeKit", "bae-macos", "bae-ios")
    for path in sorted(
        swift_path
        for base in swift_roots
        for swift_path in (root / base).rglob("*.swift")
        if ".build" not in swift_path.parts and "BaeBridge" not in swift_path.parts
    ):
        source = path.read_text()
        masked = mask_non_code(source)
        for found in pattern.finditer(masked):
            opening = masked.find("{", found.start())
            try:
                closing = matching_brace(masked, opening)
            except ValueError:
                violations.append(
                    f"{path.relative_to(root)} has an unreadable Task.detached body"
                )
                continue
            body = re.sub(
                r"\bawait\s+MainActor\.run\b",
                "MainActor.run",
                source[opening + 1 : closing],
            )
            if re.search(r"\bawait\b", body):
                line = source.count("\n", 0, found.start()) + 1
                violations.append(
                    f"{path.relative_to(root)}:{line} wraps an async bridge call in Task.detached"
                )


# An `OperationRuntime` run: the AppHandle's or a subscription's `runtime`, or
# the host's `operations`.
OWNED_RUNTIME_CALL = re.compile(
    r"\b(?:runtime|operations)\s*(?:\.clone\(\)\s*)?\.(?:run|run_to_completion)\("
)


def lower_camel(name: str) -> str:
    first, *rest = name.split("_")
    return first + "".join(part.title() for part in rest)


def reject_android_async_worker_wrappers(
    root: Path, async_exports: set[str], violations: list[str]
) -> None:
    if not async_exports:
        return
    pattern = re.compile(r"withContext\s*\([^)]*\)\s*\{")
    method_pattern = re.compile(
        r"\b(" + "|".join(map(re.escape, sorted(async_exports))) + r")\s*\("
    )
    android_root = root / "bae-android" / "app" / "src" / "main"
    for path in sorted(android_root.rglob("*.kt")):
        source = path.read_text()
        masked = mask_non_code(source)
        for found in pattern.finditer(masked):
            opening = masked.find("{", found.start())
            try:
                closing = matching_brace(masked, opening)
            except ValueError:
                violations.append(
                    f"{path.relative_to(root)} has an unreadable withContext body"
                )
                continue
            wrapped = method_pattern.search(masked[opening + 1 : closing])
            if wrapped:
                line = source.count("\n", 0, found.start()) + 1
                violations.append(
                    f"{path.relative_to(root)}:{line} wraps async {wrapped.group(1)} "
                    "in withContext"
                )


def mask_non_code(source: str) -> str:
    chars = list(source)
    index = 0
    block_depth = 0
    while index < len(chars):
        if block_depth:
            if source.startswith("/*", index):
                chars[index : index + 2] = "  "
                block_depth += 1
                index += 2
            elif source.startswith("*/", index):
                chars[index : index + 2] = "  "
                block_depth -= 1
                index += 2
            else:
                if chars[index] != "\n":
                    chars[index] = " "
                index += 1
            continue
        if source.startswith("//", index):
            end = source.find("\n", index)
            end = len(source) if end == -1 else end
            chars[index:end] = " " * (end - index)
            index = end
            continue
        if source.startswith("/*", index):
            chars[index : index + 2] = "  "
            block_depth = 1
            index += 2
            continue

        raw = re.match(r"(?:b|c)?r(#+)?\"", source[index:])
        if raw:
            hashes = raw.group(1) or ""
            start_length = raw.end()
            terminator = '"' + hashes
            end = source.find(terminator, index + start_length)
            end = len(source) if end == -1 else end + len(terminator)
            chars[index:end] = " " * (end - index)
            index = end
            continue

        prefix_length = 2 if source.startswith(('b"', 'c"', "b'"), index) else 1
        quote = source[index + prefix_length - 1]
        if quote in ('"', "'"):
            cursor = index + prefix_length
            while cursor < len(source):
                if source[cursor] == "\\":
                    cursor += 2
                    continue
                if source[cursor] == quote:
                    cursor += 1
                    break
                cursor += 1
            chars[index:cursor] = " " * (cursor - index)
            index = cursor
            continue
        index += 1
    return "".join(chars)


def matching_bracket(masked: str, opening: int, open_char: str, close_char: str) -> int:
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == open_char:
            depth += 1
        elif masked[index] == close_char:
            depth -= 1
            if depth == 0:
                return index
    raise ValueError(f"unclosed {open_char} at byte {opening}")


def matching_brace(masked: str, opening: int) -> int:
    return matching_bracket(masked, opening, "{", "}")


def exported_methods(path: Path) -> list[tuple[str, bool, str]]:
    """Each exported method's name, whether it is async, and its body with
    comments and strings masked."""
    masked = mask_non_code(path.read_text())
    methods: list[tuple[str, bool, str]] = []
    attribute = re.compile(r"#\s*\[\s*uniffi::export\b")
    method = re.compile(r"\bpub\s+(?:\([^)]*\)\s+)?(async\s+)?fn\s+(\w+)\b")

    for export in attribute.finditer(masked):
        attribute_end = masked.find("]", export.end())
        if attribute_end == -1:
            raise ValueError(f"unclosed export attribute in {path}")
        tail = masked[attribute_end + 1 :]
        impl_match = re.match(r"\s*impl\b", tail)
        if impl_match is None:
            continue
        impl_start = attribute_end + 1 + impl_match.start()
        opening = masked.find("{", impl_start)
        closing = matching_brace(masked, opening)
        impl_masked = masked[opening + 1 : closing]
        for found in method.finditer(impl_masked):
            body_opening = impl_masked.find("{", found.end())
            body_closing = matching_brace(impl_masked, body_opening)
            methods.append(
                (
                    found.group(2),
                    found.group(1) is not None,
                    impl_masked[body_opening + 1 : body_closing],
                )
            )
    return methods


SPAWN_CALL = re.compile(r"\bspawn\s*\(")
OPERATION_RUNTIME_SPAWN = re.compile(r"\boperation_runtime\s*::\s*spawn\b")
RUN_EXPORTED = re.compile(r"\brun_exported\b")


def check(root: Path) -> tuple[list[str], int, int]:
    """The violations, and the counts of async and task-producing exports."""
    violations: list[str] = []
    async_count = 0
    spawned_count = 0
    async_exports = set()
    for path in sorted((root / "bae-bridge" / "src").rglob("*.rs")):
        for name, is_async, body in exported_methods(path):
            relative = path.relative_to(root)
            if is_async:
                async_count += 1
                async_exports.add(lower_camel(name))
                if not OWNED_RUNTIME_CALL.search(body) and not RUN_EXPORTED.search(body):
                    violations.append(f"{relative}::{name}")
            if SPAWN_CALL.search(body):
                spawned_count += 1
                if not OPERATION_RUNTIME_SPAWN.search(body):
                    violations.append(f"{relative}::{name} constructs a task before scheduling")

    reject_flush_on_worker_thread(root, violations)
    reject_detached_async_bridge_calls(root, violations)
    reject_android_async_worker_wrappers(root, async_exports, violations)
    return violations, async_count, spawned_count


def main() -> int:
    violations, async_count, spawned_count = check(ROOT)
    if violations:
        print("UniFFI runtime boundary violations:", file=sys.stderr)
        for violation in violations:
            print(f"  {violation}", file=sys.stderr)
        return 1
    print(
        "UniFFI runtime boundary: "
        f"{async_count} async exports and {spawned_count} task-producing exports "
        "use an owned runtime"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
