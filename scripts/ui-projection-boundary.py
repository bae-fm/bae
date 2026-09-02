#!/usr/bin/env python3
"""Keep render-only repeated children on their declared projection inputs."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class RenderLeaf:
    path: str
    symbol: str
    language: str
    declaration: str
    callable_declaration: bool = False


# Repetition syntax does not say whether its child renders a supplied value or
# owns a paging slot. This inventory records the former. Paging controls and
# repeated containers are intentionally absent.
RENDER_LEAVES = (
    RenderLeaf(
        "bae-macos/bae/bae/Views/Import/Candidates/TriageRowView.swift",
        "TriageRowView",
        "swift",
        r"\bstruct\s+TriageRowView\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-macos/bae/bae/Views/Library/Grid/AlbumCardView.swift",
        "AlbumCardView",
        "swift",
        r"\bstruct\s+AlbumCardView\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Library/DownloadsView.swift",
        "DownloadQueueRow",
        "swift",
        r"\b(?:private\s+)?struct\s+DownloadQueueRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-macos/bae/bae/Views/Import/Search/ImportSearchResultRow.swift",
        "ImportSearchResultRow",
        "swift",
        r"\bstruct\s+ImportSearchResultRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-macos/bae/bae/Views/Library/AlbumDetail/TrackRowView.swift",
        "TrackRowView",
        "swift",
        r"\bstruct\s+TrackRowView\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-macos/bae/bae/Views/Library/Browse/BrowseListRow.swift",
        "BrowseSummaryRow",
        "swift",
        r"\bstruct\s+BrowseSummaryRow\s*<[^>{}]+>\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Playback/Queue/QueueRow.swift",
        "QueueRow",
        "swift",
        r"\bstruct\s+QueueRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Library/AlbumGrid.swift",
        "AlbumCard",
        "swift",
        r"\bstruct\s+AlbumCard\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Library/Browse/ArtistListView.swift",
        "ArtistSummaryRow",
        "swift",
        r"\bstruct\s+ArtistSummaryRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Library/Browse/ComposerListView.swift",
        "ComposerSummaryRow",
        "swift",
        r"\bstruct\s+ComposerSummaryRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Library/SearchResultsView.swift",
        "AlbumResultRow",
        "swift",
        r"\bstruct\s+AlbumResultRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Library/SearchResultsView.swift",
        "TrackResultRow",
        "swift",
        r"\bstruct\s+TrackResultRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-ios/bae/bae/Views/Library/SearchResultsView.swift",
        "WorkResultRow",
        "swift",
        r"\bstruct\s+WorkResultRow\s*:\s*View\b",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/albumdetail/AlbumTrackRow.kt",
        "TrackRow",
        "kotlin",
        r"\bfun\s+TrackRow\s*\(",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/playback/QueueRow.kt",
        "QueueRow",
        "kotlin",
        r"\bfun\s+QueueRow\s*\(",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/library/LibraryGrid.kt",
        "AlbumGridCard",
        "kotlin",
        r"\bfun\s+AlbumGridCard\s*\(",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/library/ArtistBrowser.kt",
        "ArtistSummaryRow",
        "kotlin",
        r"\bfun\s+ArtistSummaryRow\s*\(",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/library/ComposerBrowser.kt",
        "ComposerSummaryRow",
        "kotlin",
        r"\bfun\s+ComposerSummaryRow\s*\(",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/library/SearchResultsScreen.kt",
        "AlbumResultRow",
        "kotlin",
        r"\bfun\s+AlbumResultRow\s*\(",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/library/SearchResultsScreen.kt",
        "TrackResultRow",
        "kotlin",
        r"\bfun\s+TrackResultRow\s*\(",
    ),
    RenderLeaf(
        "bae-android/app/src/main/java/fm/bae/app/ui/library/SearchResultsScreen.kt",
        "WorkResultRow",
        "kotlin",
        r"\bfun\s+WorkResultRow\s*\(",
    ),
    RenderLeaf(
        "bae-avalonia/Views/Library/AlbumExpansionRows.cs",
        "AlbumExpansionRows.BuildTrackRow",
        "csharp",
        r"\bBuildTrackRow\s*\(",
    ),
)


# These types can obtain entity facts independently of a leaf's declared
# projection. Image resolvers, localization, styling, presentation values,
# bindings, callbacks, and local state are not entity-data owners.
ENTITY_DATA_OWNERS = {
    "swift": (
        "AppService",
        "CandidateRuntime",
        "Cast",
        "ConfigStore",
        "Database",
        "Discogs",
        "DownloadStore",
        "Downloads",
        "Importer",
        "ImportStore",
        "Library",
        "LibraryBrowseSession",
        "LibraryStore",
        "OutboxStore",
        "OutputStore",
        "Outputs",
        "Playback",
        "PlaybackStore",
        "PreviewAudio",
        "Queue",
        "ReleaseEditor",
        "StorageStore",
        "Sync",
    ),
    "kotlin": (
        "AppSession",
        "ConfigStore",
        "Database",
        "LibraryStore",
        "OpenLibrary",
        "OutboxStore",
        "PlaybackStore",
    ),
    "csharp": (
        "AppService",
        "AlbumDetailStore",
        "Database",
        "ImportService",
        "ImportStore",
        "LibraryBrowserStore",
        "LibraryService",
        "PlaybackStore",
        "ReleaseEditorService",
        "StorageStore",
    ),
}

OWNER_TYPE = re.compile(r"\b[A-Z][A-Za-z0-9_]*(?:Store|Service|Session)\b")
NON_ENTITY_UI_OWNERS = {
    "ImageStore",
    "LocalImageStore",
    "UiStore",
}

# A leaf's inputs are what its parent passes in. Pulling an object out of the
# SwiftUI environment is the one way a leaf can grow an input its call site
# never shows, so every environment object a leaf takes is checked by type,
# whatever the type is named. Key-path environment values (`\.displayScale`,
# `\.dismiss`) are presentation values and are not matched.
SWIFT_ENVIRONMENT_OBJECT = re.compile(
    r"@Environment\s*\(\s*([A-Z][A-Za-z0-9_]*)\.self\s*\)"
    r"|@(?:EnvironmentObject|ObservedObject|StateObject)\b"
    r"(?:\s+(?:private|fileprivate|internal|public)(?:\(set\))?)*"
    r"\s+var\s+[A-Za-z_][A-Za-z0-9_]*\s*:\s*([A-Z][A-Za-z0-9_]*)"
)

SWIFT_SOURCE_ROOTS = (
    "bae-macos/bae/bae",
    "bae-ios/bae/bae",
)
KOTLIN_SOURCE_ROOT = "bae-android/app/src/main/java"
AVALONIA_SOURCE_ROOT = "bae-avalonia/Views"

# These direct repeated children own paging or playback state rather than
# rendering one complete database projection. Keeping the exceptions named
# makes a new state-connected repeated child a conscious boundary decision.
DISCOVERED_OWNER_EXEMPTIONS = {
    (
        "swift",
        "bae-ios/bae/bae/Views/Library/AlbumGrid.swift",
        "AlbumCell",
    ): ({"LibraryStore"}, "paging slot"),
    (
        "swift",
        "bae-ios/bae/bae/Views/Library/Browse/ArtistListView.swift",
        "ArtistRowSlot",
    ): ({"LibraryStore"}, "paging slot"),
    (
        "swift",
        "bae-ios/bae/bae/Views/Library/Browse/ComposerListView.swift",
        "ComposerRowSlot",
    ): ({"LibraryStore"}, "paging slot"),
    (
        "swift",
        "bae-ios/bae/bae/Views/AlbumDetail/TrackList.swift",
        "TrackRow",
    ): ({"Playback", "PlaybackStore"}, "playback-connected control"),
}

AVALONIA_ITEM_TEMPLATES = {
    ("bae-avalonia/Views/Playback/QueuePane.cs", "BuildRowVisual"): "state-connected control",
    ("bae-avalonia/Views/Storage/StorageTableView.cs", "StorageRowControl"): "paging slot",
    ("bae-avalonia/Views/Library/IncrementalListView.cs", "IncrementalRow"): "paging slot",
    ("bae-avalonia/Views/Library/AlbumExpansionView.cs", "TextBlock"): "projection renderer",
    ("bae-avalonia/Views/Library/AlbumGridView.cs", "AlbumRowControl"): "paging slot",
}

FORBIDDEN_SECONDARY_PROJECTIONS = (
    "Candidate",
    "BridgeImportCandidateDetail",
)


@dataclass(frozen=True)
class SourceFragment:
    path: str
    source: str
    full_source: str
    offset: int


def mask_comments_and_literals(source: str) -> str:
    """Replace comments and quoted literals while retaining source positions."""
    result = list(source)
    index = 0
    state = "code"
    quote = ""
    block_depth = 0
    while index < len(source):
        current = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if state == "code":
            if current == "/" and following == "/":
                result[index] = result[index + 1] = " "
                index += 2
                state = "line_comment"
                continue
            if current == "/" and following == "*":
                result[index] = result[index + 1] = " "
                index += 2
                state = "block_comment"
                block_depth = 1
                continue
            if source.startswith('"""', index):
                result[index : index + 3] = "   "
                index += 3
                state = "triple_literal"
                continue
            if current in {'"', "'"}:
                quote = current
                result[index] = " "
                index += 1
                state = "literal"
                continue
        elif state == "line_comment":
            if current == "\n":
                state = "code"
            else:
                result[index] = " "
            index += 1
            continue
        elif state == "block_comment":
            if current == "/" and following == "*":
                result[index] = result[index + 1] = " "
                index += 2
                block_depth += 1
                continue
            if current == "*" and following == "/":
                result[index] = result[index + 1] = " "
                index += 2
                block_depth -= 1
                if block_depth == 0:
                    state = "code"
                continue
            if current != "\n":
                result[index] = " "
            index += 1
            continue
        elif state == "triple_literal":
            if source.startswith('"""', index):
                result[index : index + 3] = "   "
                index += 3
                state = "code"
                continue
            if current != "\n":
                result[index] = " "
            index += 1
            continue
        elif state == "literal":
            if current == "\\":
                result[index] = " "
                if index + 1 < len(source):
                    result[index + 1] = " "
                index += 2
                continue
            if current == quote:
                result[index] = " "
                index += 1
                state = "code"
                continue
            if current != "\n":
                result[index] = " "
            index += 1
            continue
        index += 1
    return "".join(result)


def matching_delimiter(source: str, start: int, opening: str, closing: str) -> int:
    depth = 0
    for index in range(start, len(source)):
        if source[index] == opening:
            depth += 1
        elif source[index] == closing:
            depth -= 1
            if depth == 0:
                return index
    raise ValueError(f"unclosed {opening!r} at offset {start}")


def declaration_source(source: str, leaf: RenderLeaf) -> tuple[str, int]:
    masked = mask_comments_and_literals(source)
    matches = list(re.finditer(leaf.declaration, masked))
    if len(matches) != 1:
        raise ValueError(
            f"expected one declaration for {leaf.symbol}, found {len(matches)}"
        )
    match = matches[0]
    if leaf.callable_declaration or leaf.language in {"kotlin", "csharp"}:
        parameters = masked.find("(", match.start(), match.end() + 1)
        if parameters == -1:
            raise ValueError(f"missing parameter list for {leaf.symbol}")
        parameters_end = matching_delimiter(masked, parameters, "(", ")")
        body_start = masked.find("{", parameters_end)
    else:
        body_start = masked.find("{", match.end())
    if body_start == -1:
        raise ValueError(f"missing body for {leaf.symbol}")
    body_end = matching_delimiter(masked, body_start, "{", "}")
    return source[match.start() : body_end + 1], match.start()


def swift_root_for(path: str) -> str:
    matches = [root for root in SWIFT_SOURCE_ROOTS if path.startswith(f"{root}/")]
    if len(matches) != 1:
        raise ValueError(f"no Swift source root for {path}")
    return matches[0]


def swift_extension_fragments(root: Path, leaf: RenderLeaf) -> list[SourceFragment]:
    fragments: list[SourceFragment] = []
    extension_pattern = re.compile(
        rf"\bextension\s+{re.escape(leaf.symbol)}\b[^{{]*{{"
    )
    for path in sorted((root / swift_root_for(leaf.path)).rglob("*.swift")):
        source = path.read_text()
        if "extension" not in source or leaf.symbol not in source:
            continue
        masked = mask_comments_and_literals(source)
        for match in extension_pattern.finditer(masked):
            body_start = masked.find("{", match.start(), match.end())
            body_end = matching_delimiter(masked, body_start, "{", "}")
            fragments.append(
                SourceFragment(
                    str(path.relative_to(root)),
                    source[match.start() : body_end + 1],
                    source,
                    match.start(),
                )
            )
    return fragments


def leaf_fragments(root: Path, leaf: RenderLeaf) -> list[SourceFragment]:
    path = root / leaf.path
    source = path.read_text()
    declaration, offset = declaration_source(source, leaf)
    fragments = [SourceFragment(leaf.path, declaration, source, offset)]
    if leaf.language == "swift":
        fragments.extend(swift_extension_fragments(root, leaf))
    return fragments


def leaf_violations(source: str, leaf: RenderLeaf) -> list[tuple[int, str]]:
    declaration, offset = declaration_source(source, leaf)
    masked = mask_comments_and_literals(declaration)
    violations: list[tuple[int, str]] = []
    owners = set(ENTITY_DATA_OWNERS[leaf.language])
    owners.update(OWNER_TYPE.findall(masked))
    owners.difference_update(NON_ENTITY_UI_OWNERS)
    for owner in sorted(owners):
        match = re.search(rf"\b{re.escape(owner)}\b", masked)
        if match is None:
            continue
        line = source.count("\n", 0, offset + match.start()) + 1
        violations.append((line, owner))
    return violations


def fragment_owner_violations(
    fragment: SourceFragment,
    leaf: RenderLeaf,
    allowed_owners: set[str],
) -> list[str]:
    masked = mask_comments_and_literals(fragment.source)
    owners = set(ENTITY_DATA_OWNERS[leaf.language])
    owners.update(OWNER_TYPE.findall(masked))
    owners.difference_update(NON_ENTITY_UI_OWNERS)
    owners.difference_update(allowed_owners)
    violations: list[str] = []
    for owner in sorted(owners):
        match = re.search(rf"\b{re.escape(owner)}\b", masked)
        if match is None:
            continue
        root_offset = fragment.offset + match.start()
        source_line = fragment.full_source.count("\n", 0, root_offset) + 1
        violations.append(
            f"{fragment.path}:{source_line}: {leaf.symbol} reaches entity-data owner {owner}"
        )
    return violations


def fragment_environment_violations(
    fragment: SourceFragment,
    leaf: RenderLeaf,
    allowed_owners: set[str],
) -> list[str]:
    if leaf.language != "swift":
        return []
    masked = mask_comments_and_literals(fragment.source)
    violations: list[str] = []
    for match in SWIFT_ENVIRONMENT_OBJECT.finditer(masked):
        injected = match.group(1) or match.group(2)
        if injected in NON_ENTITY_UI_OWNERS or injected in allowed_owners:
            continue
        root_offset = fragment.offset + match.start()
        source_line = fragment.full_source.count("\n", 0, root_offset) + 1
        violations.append(
            f"{fragment.path}:{source_line}: {leaf.symbol} takes {injected} "
            "from the environment"
        )
    return violations


def fragment_projection_violations(
    fragment: SourceFragment, leaf: RenderLeaf
) -> list[str]:
    masked = mask_comments_and_literals(fragment.source)
    violations: list[str] = []
    for projection in FORBIDDEN_SECONDARY_PROJECTIONS:
        match = re.search(rf"\b{re.escape(projection)}\b", masked)
        if match is None:
            continue
        root_offset = fragment.offset + match.start()
        source_line = fragment.full_source.count("\n", 0, root_offset) + 1
        violations.append(
            f"{fragment.path}:{source_line}: {leaf.symbol} reaches secondary entity "
            f"projection {projection}"
        )
    return violations


def declaration_indexes(root: Path) -> dict[str, dict[str, list[RenderLeaf]]]:
    indexes: dict[str, dict[str, list[RenderLeaf]]] = {
        "swift": {},
        "kotlin": {},
        "csharp": {},
    }
    swift_pattern = re.compile(
        r"\b(?:private\s+)?struct\s+([A-Z][A-Za-z0-9_]*)"
        r"(?:\s*<[^>{}]+>)?\s*:\s*View\b"
    )
    for source_root in SWIFT_SOURCE_ROOTS:
        for path in sorted((root / source_root).rglob("*.swift")):
            source = mask_comments_and_literals(path.read_text())
            for match in swift_pattern.finditer(source):
                symbol = match.group(1)
                leaf = RenderLeaf(
                    str(path.relative_to(root)),
                    symbol,
                    "swift",
                    rf"\b(?:private\s+)?struct\s+{re.escape(symbol)}"
                    r"(?:\s*<[^>{}]+>)?\s*:\s*View\b",
                )
                indexes["swift"].setdefault(symbol, []).append(leaf)
            function_pattern = re.compile(
                r"\b(?:(?:private|fileprivate|internal|public)\s+)?"
                r"func\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("
            )
            for match in function_pattern.finditer(source):
                parameters = source.find("(", match.start(), match.end() + 1)
                parameters_end = matching_delimiter(source, parameters, "(", ")")
                body_start = source.find("{", parameters_end)
                if body_start == -1 or not re.search(
                    r"->\s*some\s+View\b", source[parameters_end:body_start]
                ):
                    continue
                symbol = match.group(1)
                leaf = RenderLeaf(
                    str(path.relative_to(root)),
                    symbol,
                    "swift",
                    rf"\b(?:(?:private|fileprivate|internal|public)\s+)?"
                    rf"func\s+{re.escape(symbol)}\s*\(",
                    True,
                )
                indexes["swift"].setdefault(symbol, []).append(leaf)

    kotlin_pattern = re.compile(
        r"@Composable\b(?:\s+@[A-Za-z][^\n]*)*\s+"
        r"(?:(?:private|internal|public)\s+)?fun\s+"
        r"([A-Za-z_][A-Za-z0-9_]*)\s*\("
    )
    for path in sorted((root / KOTLIN_SOURCE_ROOT).rglob("*.kt")):
        source = mask_comments_and_literals(path.read_text())
        for match in kotlin_pattern.finditer(source):
            symbol = match.group(1)
            leaf = RenderLeaf(
                str(path.relative_to(root)),
                symbol,
                "kotlin",
                rf"\b(?:(?:private|internal|public)\s+)?fun\s+"
                rf"{re.escape(symbol)}\s*\(",
                True,
            )
            indexes["kotlin"].setdefault(symbol, []).append(leaf)
    return indexes


def resolved_leaf(
    indexes: dict[str, dict[str, list[RenderLeaf]]],
    language: str,
    symbol: str,
    callsite: Path,
) -> RenderLeaf | None:
    declarations = indexes[language].get(symbol, [])
    if language == "swift":
        source_root = swift_root_for(str(callsite))
        declarations = [
            leaf
            for leaf in declarations
            if leaf.path.startswith(f"{source_root}/")
        ]
    same_file = [leaf for leaf in declarations if leaf.path == str(callsite)]
    if len(same_file) == 1:
        return same_file[0]
    if len(declarations) == 1:
        return declarations[0]
    if len(declarations) > 1:
        paths = ", ".join(sorted(leaf.path for leaf in declarations))
        raise ValueError(
            f"ambiguous repeated child {symbol} from {callsite}: {paths}"
        )
    return None


def repeated_regions(
    masked: str, construct: re.Pattern[str]
) -> list[tuple[str, str | None]]:
    regions: list[tuple[str, str | None]] = []
    for match in construct.finditer(masked):
        parameters = masked.find("(", match.start(), match.end() + 1)
        if parameters == -1:
            continue
        try:
            parameters_end = matching_delimiter(masked, parameters, "(", ")")
        except ValueError:
            continue
        parameters_source = masked[parameters + 1 : parameters_end]
        trailing = re.match(r"\s*{", masked[parameters_end + 1 :])
        if trailing is None:
            regions.append((parameters_source, None))
            continue
        body_start = parameters_end + 1 + trailing.end() - 1
        try:
            body_end = matching_delimiter(masked, body_start, "{", "}")
        except ValueError:
            regions.append((parameters_source, None))
            continue
        regions.append((parameters_source, masked[body_start + 1 : body_end]))
    return regions


def lambda_body(source: str, label: str) -> str | None:
    match = re.search(rf"\b{re.escape(label)}\s*(?::|=)\s*{{", source)
    if match is None:
        return None
    body_start = source.find("{", match.start(), match.end())
    try:
        body_end = matching_delimiter(source, body_start, "{", "}")
    except ValueError:
        return None
    return source[body_start + 1 : body_end]


def discovered_render_leaves(root: Path) -> set[RenderLeaf]:
    indexes = declaration_indexes(root)
    discovered: set[RenderLeaf] = set()
    call_pattern = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")

    swift_construct = re.compile(r"\b(?:ForEach|List|Table)\s*\(")
    for source_root in SWIFT_SOURCE_ROOTS:
        for path in sorted((root / source_root).rglob("*.swift")):
            relative = path.relative_to(root)
            masked = mask_comments_and_literals(path.read_text())
            for parameters, body in repeated_regions(masked, swift_construct):
                regions = [body] if body is not None else []
                content_body = lambda_body(parameters, "content")
                if content_body is not None:
                    regions.append(content_body)
                for region in regions:
                    for call in call_pattern.finditer(region):
                        leaf = resolved_leaf(indexes, "swift", call.group(1), relative)
                        if leaf is not None:
                            discovered.add(leaf)
                for reference in re.finditer(
                    r"\bcontent\s*:\s*([A-Za-z_][A-Za-z0-9_]*)(?:\.init)?\b",
                    parameters,
                ):
                    leaf = resolved_leaf(
                        indexes, "swift", reference.group(1), relative
                    )
                    if leaf is not None:
                        discovered.add(leaf)

    kotlin_construct = re.compile(r"\bitems(?:Indexed)?\s*\(")
    for path in sorted((root / KOTLIN_SOURCE_ROOT).rglob("*.kt")):
        relative = path.relative_to(root)
        masked = mask_comments_and_literals(path.read_text())
        for parameters, body in repeated_regions(masked, kotlin_construct):
            regions = [body] if body is not None else []
            item_content = lambda_body(parameters, "itemContent")
            if item_content is not None:
                regions.append(item_content)
            for region in regions:
                for call in call_pattern.finditer(region):
                    leaf = resolved_leaf(indexes, "kotlin", call.group(1), relative)
                    if leaf is not None:
                        discovered.add(leaf)
    return discovered


def all_render_leaves(root: Path) -> tuple[RenderLeaf, ...]:
    leaves = {(leaf.language, leaf.path, leaf.symbol): leaf for leaf in RENDER_LEAVES}
    for leaf in discovered_render_leaves(root):
        leaves.setdefault((leaf.language, leaf.path, leaf.symbol), leaf)
    return tuple(
        leaves[key]
        for key in sorted(leaves, key=lambda value: (value[0], value[1], value[2]))
    )


def avalonia_template_target(source: str) -> str:
    for target in (
        "BuildRowVisual",
        "StorageRowControl",
        "IncrementalRow",
        "TextBlock",
        "AlbumRowControl",
    ):
        if re.search(rf"\b{target}(?:\s*<[^>]+>)?\s*(?:\(|{{)", source):
            return target
    return "unclassified"


def avalonia_assignment_end(masked: str, start: int) -> int:
    parentheses = 0
    braces = 0
    brackets = 0
    for index in range(start, len(masked)):
        character = masked[index]
        if character == "(":
            parentheses += 1
        elif character == ")":
            parentheses -= 1
        elif character == "{":
            braces += 1
        elif character == "}":
            if braces == 0 and parentheses == 0 and brackets == 0:
                return index
            braces -= 1
        elif character == "[":
            brackets += 1
        elif character == "]":
            brackets -= 1
        elif character in {",", ";"} and not (parentheses or braces or brackets):
            return index
    raise ValueError(f"unclosed Avalonia ItemTemplate assignment at offset {start}")


def avalonia_template_assignments(
    root: Path,
) -> list[tuple[str, str, int, str]]:
    source_root = root / AVALONIA_SOURCE_ROOT
    if not source_root.is_dir():
        return []
    assignments: list[tuple[str, str, int, str]] = []
    for path in sorted(source_root.rglob("*.cs")):
        source = path.read_text()
        masked = mask_comments_and_literals(source)
        relative = str(path.relative_to(root))
        for match in re.finditer(r"\bItemTemplate\s*=", masked):
            end = avalonia_assignment_end(masked, match.end())
            assignment = source[match.start() : end]
            target = avalonia_template_target(masked[match.start() : end])
            line = source.count("\n", 0, match.start()) + 1
            assignments.append((relative, target, line, assignment))
    return assignments


def avalonia_template_violations(root: Path) -> list[str]:
    seen: set[tuple[str, str]] = set()
    violations: list[str] = []
    for path, target, line, assignment in avalonia_template_assignments(root):
        key = (path, target)
        masked = mask_comments_and_literals(assignment)
        owner = next(
            (
                candidate
                for candidate in ENTITY_DATA_OWNERS["csharp"]
                if re.search(rf"\b{re.escape(candidate)}\b", masked)
            ),
            None,
        )
        if key in seen or key not in AVALONIA_ITEM_TEMPLATES:
            violations.append(
                f"{path}:{line}: repeated Avalonia child {target} has no "
                "projection-boundary classification"
            )
        elif owner is not None:
            violations.append(
                f"{path}:{line}: repeated Avalonia child {target} reaches "
                f"entity-data owner {owner}"
            )
        seen.add(key)
    return violations


def check(root: Path) -> list[str]:
    violations: list[str] = []
    try:
        leaves = all_render_leaves(root)
    except ValueError as error:
        return [str(error)]
    for leaf in leaves:
        key = (leaf.language, leaf.path, leaf.symbol)
        allowed_owners = DISCOVERED_OWNER_EXEMPTIONS.get(key, (set(), ""))[0]
        path = root / leaf.path
        if not path.is_file():
            violations.append(f"{leaf.path}: missing inventoried render leaf")
            continue
        try:
            fragments = leaf_fragments(root, leaf)
        except ValueError as error:
            violations.append(f"{leaf.path}: {error}")
            continue
        for fragment in fragments:
            violations.extend(
                fragment_owner_violations(fragment, leaf, allowed_owners)
            )
            violations.extend(
                fragment_environment_violations(fragment, leaf, allowed_owners)
            )
            violations.extend(fragment_projection_violations(fragment, leaf))
    violations.extend(avalonia_template_violations(root))
    return violations


def verify_parser() -> None:
    swift = RenderLeaf("fixture.swift", "Leaf", "swift", r"\bstruct\s+Leaf\b")
    legal_swift = """
struct Leaf: View {
    let projection: BridgeTriageRow
    let selected: Binding<Bool>
    let onActivate: () -> Void
    @State private var hovered = false
    var body: some View { Text("AppService in a literal") }
}
"""
    if leaf_violations(legal_swift, swift):
        raise RuntimeError("legal Swift projection leaf failed parser self-test")
    forbidden_swift = legal_swift.replace(
        "let projection: BridgeTriageRow",
        "let projection: BridgeTriageRow\n    let selectedDetail: ImportStore",
    )
    if leaf_violations(forbidden_swift, swift) != [(4, "ImportStore")]:
        raise RuntimeError("forbidden Swift projection leaf escaped parser self-test")

    kotlin = RenderLeaf("fixture.kt", "Leaf", "kotlin", r"\bfun\s+Leaf\s*\(")
    forbidden_kotlin = """
fun Leaf(data: RowData, session: OpenLibrary, onClick: () -> Unit) {
    var selected = false
}
"""
    if leaf_violations(forbidden_kotlin, kotlin) != [(2, "OpenLibrary")]:
        raise RuntimeError("forbidden Kotlin projection leaf escaped parser self-test")

    csharp = RenderLeaf("fixture.cs", "Rows.Build", "csharp", r"\bBuild\s*\(")
    forbidden_csharp = """
static Control Build(RowData row, AppService app, Action onClick)
{
    return new Control();
}
"""
    if leaf_violations(forbidden_csharp, csharp) != [(2, "AppService")]:
        raise RuntimeError("forbidden C# projection leaf escaped parser self-test")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parent.parent,
        help="repository root containing the inventoried source files",
    )
    args = parser.parse_args()
    verify_parser()
    violations = check(args.root)
    for violation in violations:
        print(violation)
    if violations:
        print(
            "Render leaves receive entity facts through their declared projection only.",
            file=sys.stderr,
        )
    return 1 if violations else 0


if __name__ == "__main__":
    raise SystemExit(main())
