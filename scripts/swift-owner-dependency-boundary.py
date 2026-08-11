#!/usr/bin/env python3
"""Reject Swift fields and getters that expose retained capabilities."""

from __future__ import annotations

import re
import sys
from pathlib import Path


CAPABILITY_TYPES = {
    "AppHandle",
    "AppHandleProtocol",
    "AppService",
    "AppServiceTestAccess",
    "Automation",
    "BridgeDiagnostics",
    "Cast",
    "CastStore",
    "CancellableTaskSlot",
    "CloudKitDriver",
    "CloudKitService",
    "CloudSyncSetup",
    "CommonProjections",
    "ConfigStore",
    "CurrentValueSubject",
    "Database",
    "Discogs",
    "DesktopProjections",
    "DesktopEventHandler",
    "DisconnectSyncFlow",
    "DownloadStore",
    "Downloads",
    "ImageStore",
    "Importer",
    "ImportStore",
    "Library",
    "LibraryBrowseSession",
    "LibrarySetup",
    "LibraryStore",
    "MediaControlService",
    "MediaPaths",
    "OutboxStore",
    "OutputStore",
    "Outputs",
    "PaginatedList",
    "Playback",
    "PlaybackEventHandler",
    "PlaybackStore",
    "PassthroughSubject",
    "PreviewAudio",
    "Projection",
    "ProjectionRegistration",
    "ProjectionRegistry",
    "Queue",
    "ReleaseEditor",
    "RendererBrowser",
    "SubsonicServer",
    "Sync",
    "TrackSave",
    "UiEventPump",
    "UiStore",
}

ALLOWED_OUTPUTS = {
    (
        "BaeKit/Sources/AppleHost/CloudKitService.swift",
        "public static func bae",
    ),
    (
        "BaeKit/Sources/BaeKit/BaeDiagnostics.swift",
        "public static func configure",
    ),
    (
        "BaeKit/Sources/BaeKit/Services/Projection.swift",
        "public func register",
    ),
    ("BaeKit/Sources/BaeKit/Services/Cast.swift", "public static func stub"),
    (
        "BaeKit/Sources/BaeKit/Services/Downloads.swift",
        "public static func stub",
    ),
    (
        "BaeKit/Sources/BaeKit/Services/ImageStore.swift",
        "public static func stub",
    ),
    (
        "BaeKit/Sources/BaeKit/Services/Library.swift",
        "public static func stub",
    ),
    (
        "BaeKit/Sources/BaeKit/Services/MediaPaths.swift",
        "public static func stub",
    ),
    (
        "BaeKit/Sources/BaeKit/Services/Playback.swift",
        "public static func stub",
    ),
    (
        "BaeKit/Sources/BaeKit/Services/PreviewAudio.swift",
        "public static func stub",
    ),
    ("BaeKit/Sources/BaeKit/Services/Queue.swift", "public static func stub"),
    ("BaeKit/Sources/BaeKit/Services/Sync.swift", "public static func stub"),
}

DECLARATION = re.compile(
    r"^\s*(?P<modifiers>(?:(?:public|open|package|internal|private|fileprivate|"
    r"nonisolated|weak|unowned|lazy|static|class|override|final|"
    r"private\(set\)|package\(set\))\s+)*)"
    r"(?P<kind>let|var|func)\b"
)
TYPE_NAME = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")


def exposed(modifiers: str) -> bool:
    words = modifiers.split()
    return any(word in {"public", "open", "package"} for word in words)


def function_return_type(declaration: str) -> str | None:
    parameters = declaration.find("(", declaration.find("func"))
    if parameters == -1:
        return None
    depth = 0
    for index in range(parameters, len(declaration)):
        character = declaration[index]
        if character == "(":
            depth += 1
        elif character == ")":
            depth -= 1
            if depth == 0:
                arrow = declaration.find("->", index + 1)
                if arrow == -1:
                    return None
                return declaration[arrow + 2 :].split("{", 1)[0].strip()
    return None


def declared_type(kind: str, declaration: str) -> str | None:
    if kind == "func":
        return function_return_type(declaration)
    colon = declaration.find(":")
    if colon != -1:
        return re.split(r"[={]", declaration[colon + 1 :], maxsplit=1)[0].strip()
    initializer = declaration.find("=")
    if initializer == -1:
        return None
    expression = declaration[initializer + 1 :].split("{", 1)[0].strip()
    constructed = re.match(
        r"(?:[A-Za-z_][A-Za-z0-9_]*\.)*(?P<type>[A-Z][A-Za-z0-9_]*)",
        expression,
    )
    return constructed.group("type") if constructed else None


def complete_type(type_text: str | None) -> bool:
    if not type_text:
        return False
    pairs = (("(", ")"), ("[", "]"), ("<", ">"))
    return all(type_text.count(left) == type_text.count(right) for left, right in pairs)


def declarations(source: str) -> list[tuple[int, str, str, str]]:
    lines = source.splitlines()
    result: list[tuple[int, str, str, str]] = []
    for index, line in enumerate(lines):
        match = DECLARATION.match(line)
        if match is None:
            continue
        kind = match.group("kind")
        parts = [line.strip()]
        for continuation in lines[index + 1 : index + 12]:
            declaration = " ".join(parts)
            if complete_type(declared_type(kind, declaration)):
                break
            stripped = continuation.strip()
            if not stripped or stripped.startswith(("//", "@", "#", "}")):
                break
            if DECLARATION.match(continuation):
                break
            parts.append(stripped)
        declaration = " ".join(parts)
        result.append(
            (index + 1, match.group("modifiers"), kind, declaration)
        )
    return result


def findings(source: str) -> list[tuple[int, str]]:
    result: list[tuple[int, str]] = []
    for line_number, modifiers, kind, declaration in declarations(source):
        type_text = declared_type(kind, declaration)
        retained_singleton = kind != "func" and "static" in modifiers.split()
        if type_text is None or not (exposed(modifiers) or retained_singleton):
            continue
        type_names = set(TYPE_NAME.findall(type_text))
        if type_names.isdisjoint(CAPABILITY_TYPES):
            continue
        if kind != "func" and ":" not in declaration:
            signature = declaration.split("{", 1)[0].strip()
        else:
            signature = re.split(r"[={]", declaration, maxsplit=1)[0].strip()
        result.append((line_number, signature))
    return result


def verify_parser() -> None:
    fixture = """
final class Owner {
    private let handle: AppHandle
    public let leaked: AppHandle
    public func handle() -> AppHandle { handle }
    public let optionalLeak:
        AppService?
    public func database()
        -> Database
    {
        fatalError()
    }
    public let subject = PassthroughSubject<Int, Never>()
    static let singleton = LibrarySetup()
}
"""
    actual = findings(fixture)
    expected = [
        (4, "public let leaked: AppHandle"),
        (5, "public func handle() -> AppHandle"),
        (6, "public let optionalLeak: AppService?"),
        (8, "public func database() -> Database"),
        (13, "public let subject = PassthroughSubject<Int, Never>()"),
        (14, "static let singleton = LibrarySetup()"),
    ]
    if actual != expected:
        raise RuntimeError(f"Swift ownership parser self-test failed: {actual!r}")


def swift_files(root: Path) -> list[Path]:
    source_roots = (
        root / "BaeKit" / "Sources" / "BaeKit",
        root / "BaeKit" / "Sources" / "AppleHost",
        root / "bae-ios" / "bae" / "bae",
        root / "bae-macos" / "bae" / "bae",
    )
    return sorted(
        path
        for source_root in source_roots
        for path in source_root.rglob("*.swift")
        if not path.name.startswith("bae_bridge_")
    )


def allowed_output(root: Path, path: Path, declaration: str) -> bool:
    relative = str(path.relative_to(root))
    return any(
        relative == allowed_path and declaration.startswith(prefix)
        for allowed_path, prefix in ALLOWED_OUTPUTS
    )


def main() -> int:
    verify_parser()
    root = Path(__file__).resolve().parent.parent
    violations = [
        (path, line_number, line)
        for path in swift_files(root)
        for line_number, line in findings(path.read_text())
        if not allowed_output(root, path, line)
    ]
    if not violations:
        return 0
    for path, line_number, line in violations:
        print(f"{path.relative_to(root)}:{line_number}: exposed capability: {line}")
    print(
        "Swift owners must keep retained capabilities private and expose named operations.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
