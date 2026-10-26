#!/usr/bin/env python3
"""Keep render-only repeated children on their declared projection inputs.

A repeated row's inputs are what its parent passes in, and so are the inputs of
every view the row builds: a child that reads a store redraws, and reads, once
per row just as the row itself would. The check therefore follows each repeated
row's child views, transitively and wherever they are declared, and holds each
one to the same inputs as the row.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import deque
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class RenderLeaf:
    path: str
    symbol: str
    language: str
    declaration: str
    callable_declaration: bool = False
    # The type a Swift view function is a member of; members are callable
    # unqualified only from inside that type, except members of `extension
    # View`, which are modifiers any view calls. `None` for top-level
    # functions, view structs and Kotlin composables.
    owner: str | None = None
    # Declared `private`/`fileprivate`, so visible only from its own file.
    file_private: bool = False


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
)


@dataclass(frozen=True)
class OpenFinding:
    language: str
    path: str
    symbol: str
    owner: str
    reason: str


# Views under a repeated row that read an entity-data owner where no parent
# can hand them the value as an input: each names what it reads and why the
# row's placement cannot read it instead. Frozen: any other finding fails, and
# an entry that stops producing its finding fails until it is removed.
OPEN_CHILD_FINDINGS = (
    OpenFinding(
        "swift",
        "bae-macos/bae/bae/Views/Storage/Table/StorageStateLabel.swift",
        "StorageStateLabel",
        "OutboxStore",
        "reads a release's upload observation inside an NSTableView cell; "
        "AppKit places the row, so no SwiftUI parent observes the outbox to "
        "pass it in",
    ),
    OpenFinding(
        "swift",
        "bae-macos/bae/bae/Views/Components/MetadataForm/ReleaseMetadataHeader.swift",
        "ArtistAssignmentsField",
        "Library",
        "searches artists from its popover as the user types; the results are "
        "the editor's, not a fact the track row renders",
    ),
    OpenFinding(
        "swift",
        "bae-macos/bae/bae/Views/Import/CandidateRuntimeReader.swift",
        "CandidateRuntimeReader",
        "Importer",
        "filters the shared candidate-runtime signal to one key and reads the "
        "key's current value when it appears, so a progress tick redraws "
        "ImportProgressLine alone rather than its triage row",
    ),
)


# These types can obtain entity facts independently of a leaf's declared
# projection. Image resolvers, localization, styling, presentation values,
# bindings, callbacks, and local state are not entity-data owners.
ENTITY_DATA_OWNERS = {
    "swift": (
        "AppService",
        "Cast",
        "ConfigStore",
        "Discogs",
        "DownloadStore",
        "Downloads",
        "Importer",
        "ImportStore",
        "Library",
        "LibraryBrowseSession",
        "OutboxStore",
        "OutputStore",
        "Outputs",
        "Playback",
        "PlaybackStore",
        "PreviewAudio",
        "Queue",
        "ReleaseEditor",
        "Sync",
    ),
    "kotlin": (
        "ConfigStore",
        "OpenLibrary",
        "OutboxStore",
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

FORBIDDEN_SECONDARY_PROJECTIONS = (
    "Candidate",
    "BridgeImportCandidateDetail",
)


class Sources:
    """One read and one masking per file for a whole check."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self._text: dict[str, str] = {}
        self._masked: dict[str, str] = {}
        self._files: dict[tuple[str, str], list[str]] = {}

    def text(self, relative: str) -> str:
        if relative not in self._text:
            self._text[relative] = (self.root / relative).read_text()
        return self._text[relative]

    def masked(self, relative: str) -> str:
        if relative not in self._masked:
            self._masked[relative] = mask_comments_and_literals(self.text(relative))
        return self._masked[relative]

    def files(self, source_root: str, suffix: str) -> list[str]:
        key = (source_root, suffix)
        if key not in self._files:
            self._files[key] = [
                str(path.relative_to(self.root))
                for path in sorted((self.root / source_root).rglob(f"*{suffix}"))
            ]
        return self._files[key]


@dataclass(frozen=True)
class SourceFragment:
    path: str
    source: str
    # `source` with comments and literals blanked, position for position.
    masked: str
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


def declaration_source(
    source: str, leaf: RenderLeaf, masked: str | None = None
) -> tuple[str, int]:
    if masked is None:
        masked = mask_comments_and_literals(source)
    matches = list(re.finditer(leaf.declaration, masked))
    if len(matches) > 1 and leaf.language == "swift" and leaf.callable_declaration:
        # Same-named view functions on different types in one file.
        spans = swift_type_spans(masked)
        matches = [
            match
            for match in matches
            if enclosing_type(spans, match.start()) == leaf.owner
        ]
    if len(matches) != 1:
        raise ValueError(
            f"expected one declaration for {leaf.symbol}, found {len(matches)}"
        )
    match = matches[0]
    if leaf.callable_declaration or leaf.language == "kotlin":
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


def swift_extension_fragments(
    sources: Sources, leaf: RenderLeaf
) -> list[SourceFragment]:
    fragments: list[SourceFragment] = []
    extension_pattern = re.compile(
        rf"\bextension\s+{re.escape(leaf.symbol)}\b[^{{]*{{"
    )
    for relative in sources.files(swift_root_for(leaf.path), ".swift"):
        source = sources.text(relative)
        if "extension" not in source or leaf.symbol not in source:
            continue
        masked = sources.masked(relative)
        for match in extension_pattern.finditer(masked):
            body_start = masked.find("{", match.start(), match.end())
            body_end = matching_delimiter(masked, body_start, "{", "}")
            fragments.append(
                SourceFragment(
                    relative,
                    source[match.start() : body_end + 1],
                    masked[match.start() : body_end + 1],
                    source,
                    match.start(),
                )
            )
    return fragments


def leaf_fragments(sources: Sources, leaf: RenderLeaf) -> list[SourceFragment]:
    source = sources.text(leaf.path)
    masked = sources.masked(leaf.path)
    declaration, offset = declaration_source(source, leaf, masked)
    fragments = [
        SourceFragment(
            leaf.path,
            declaration,
            masked[offset : offset + len(declaration)],
            source,
            offset,
        )
    ]
    if leaf.language == "swift" and not leaf.callable_declaration:
        fragments.extend(swift_extension_fragments(sources, leaf))
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
    subject: str,
) -> list[str]:
    masked = fragment.masked
    owners = set(ENTITY_DATA_OWNERS[leaf.language])
    owners.update(OWNER_TYPE.findall(masked))
    owners.difference_update(NON_ENTITY_UI_OWNERS)
    violations: list[str] = []
    for owner in sorted(owners):
        match = re.search(rf"\b{re.escape(owner)}\b", masked)
        if match is None:
            continue
        root_offset = fragment.offset + match.start()
        source_line = fragment.full_source.count("\n", 0, root_offset) + 1
        violations.append(
            f"{fragment.path}:{source_line}: {subject} reaches entity-data owner {owner}"
        )
    return violations


def fragment_environment_violations(
    fragment: SourceFragment,
    leaf: RenderLeaf,
    subject: str,
) -> list[str]:
    if leaf.language != "swift":
        return []
    masked = fragment.masked
    violations: list[str] = []
    for match in SWIFT_ENVIRONMENT_OBJECT.finditer(masked):
        injected = match.group(1) or match.group(2)
        if injected in NON_ENTITY_UI_OWNERS:
            continue
        root_offset = fragment.offset + match.start()
        source_line = fragment.full_source.count("\n", 0, root_offset) + 1
        violations.append(
            f"{fragment.path}:{source_line}: {subject} takes {injected} "
            "from the environment"
        )
    return violations


def fragment_projection_violations(
    fragment: SourceFragment, subject: str
) -> list[str]:
    masked = fragment.masked
    violations: list[str] = []
    for projection in FORBIDDEN_SECONDARY_PROJECTIONS:
        match = re.search(rf"\b{re.escape(projection)}\b", masked)
        if match is None:
            continue
        root_offset = fragment.offset + match.start()
        source_line = fragment.full_source.count("\n", 0, root_offset) + 1
        violations.append(
            f"{fragment.path}:{source_line}: {subject} reaches secondary entity "
            f"projection {projection}"
        )
    return violations


SWIFT_TYPE_DECLARATION = re.compile(
    r"\b(?:struct|class|enum|actor|extension|protocol)\s+"
    r"(?!func\b|var\b|let\b)([A-Za-z_][A-Za-z0-9_.]*)[^{};]*\{"
)


def swift_type_spans(masked: str) -> list[tuple[int, int, str]]:
    """Each type body in a file, with the type's unqualified name."""
    spans: list[tuple[int, int, str]] = []
    for match in SWIFT_TYPE_DECLARATION.finditer(masked):
        body_start = match.end() - 1
        try:
            body_end = matching_delimiter(masked, body_start, "{", "}")
        except ValueError:
            continue
        spans.append((body_start, body_end, match.group(1).split(".")[-1]))
    return spans


def enclosing_type(spans: list[tuple[int, int, str]], offset: int) -> str | None:
    innermost: tuple[int, int, str] | None = None
    for span in spans:
        if span[0] < offset < span[1] and (
            innermost is None or span[0] > innermost[0]
        ):
            innermost = span
    return None if innermost is None else innermost[2]


def declaration_indexes(sources: Sources) -> dict[str, dict[str, list[RenderLeaf]]]:
    indexes: dict[str, dict[str, list[RenderLeaf]]] = {
        "swift": {},
        "kotlin": {},
    }
    swift_pattern = re.compile(
        r"\b(?:(private|fileprivate)\s+)?struct\s+([A-Z][A-Za-z0-9_]*)"
        r"(?:\s*<[^>{}]+>)?\s*:\s*View\b"
    )
    function_pattern = re.compile(
        r"\b((?:(?:private|fileprivate|internal|public|static|nonisolated)\s+)*)"
        r"func\s+([A-Za-z_][A-Za-z0-9_]*)\s*\("
    )
    for source_root in SWIFT_SOURCE_ROOTS:
        for relative in sources.files(source_root, ".swift"):
            source = sources.masked(relative)
            for match in swift_pattern.finditer(source):
                symbol = match.group(2)
                leaf = RenderLeaf(
                    relative,
                    symbol,
                    "swift",
                    rf"\b(?:private\s+)?struct\s+{re.escape(symbol)}"
                    r"(?:\s*<[^>{}]+>)?\s*:\s*View\b",
                    file_private=match.group(1) is not None,
                )
                indexes["swift"].setdefault(symbol, []).append(leaf)
            spans = swift_type_spans(source)
            for match in function_pattern.finditer(source):
                parameters = source.find("(", match.start(), match.end() + 1)
                parameters_end = matching_delimiter(source, parameters, "(", ")")
                body_start = source.find("{", parameters_end)
                if body_start == -1 or not re.search(
                    r"->\s*some\s+View\b", source[parameters_end:body_start]
                ):
                    continue
                symbol = match.group(2)
                leaf = RenderLeaf(
                    relative,
                    symbol,
                    "swift",
                    rf"\b(?:(?:private|fileprivate|internal|public)\s+)?"
                    rf"func\s+{re.escape(symbol)}\s*\(",
                    True,
                    owner=enclosing_type(spans, match.start()),
                    file_private=re.search(
                        r"\b(?:private|fileprivate)\b", match.group(1)
                    )
                    is not None,
                )
                indexes["swift"].setdefault(symbol, []).append(leaf)

    kotlin_pattern = re.compile(
        r"@Composable\b(?:\s+@[A-Za-z][^\n]*)*\s+"
        r"(?:(private|internal|public)\s+)?fun\s+"
        r"([A-Za-z_][A-Za-z0-9_]*)\s*\("
    )
    for relative in sources.files(KOTLIN_SOURCE_ROOT, ".kt"):
        source = sources.masked(relative)
        for match in kotlin_pattern.finditer(source):
            symbol = match.group(2)
            leaf = RenderLeaf(
                relative,
                symbol,
                "kotlin",
                rf"\b(?:(?:private|internal|public)\s+)?fun\s+"
                rf"{re.escape(symbol)}\s*\(",
                True,
                file_private=match.group(1) == "private",
            )
            indexes["kotlin"].setdefault(symbol, []).append(leaf)
    return indexes


def resolved_leaf(
    indexes: dict[str, dict[str, list[RenderLeaf]]],
    language: str,
    symbol: str,
    callsite: str,
    caller_owner: str | None,
) -> RenderLeaf | None:
    """The view a call at `callsite`, inside type `caller_owner`, constructs."""
    declarations = indexes[language].get(symbol, [])
    if language == "swift":
        source_root = swift_root_for(callsite)
        declarations = [
            leaf
            for leaf in declarations
            if leaf.path.startswith(f"{source_root}/")
        ]
    declarations = [
        leaf
        for leaf in declarations
        if (not leaf.file_private or leaf.path == callsite)
        and leaf.owner in {None, "View", caller_owner}
    ]
    same_file = [leaf for leaf in declarations if leaf.path == callsite]
    if len(same_file) == 1:
        return same_file[0]
    if len(declarations) == 1:
        return declarations[0]
    if len(declarations) > 1:
        paths = ", ".join(sorted(leaf.path for leaf in declarations))
        raise ValueError(f"ambiguous view {symbol} from {callsite}: {paths}")
    return None


def repeated_regions(
    masked: str, construct: re.Pattern[str]
) -> list[tuple[int, str, str | None]]:
    regions: list[tuple[int, str, str | None]] = []
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
            regions.append((match.start(), parameters_source, None))
            continue
        body_start = parameters_end + 1 + trailing.end() - 1
        try:
            body_end = matching_delimiter(masked, body_start, "{", "}")
        except ValueError:
            regions.append((match.start(), parameters_source, None))
            continue
        regions.append(
            (match.start(), parameters_source, masked[body_start + 1 : body_end])
        )
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


def discovered_render_leaves(
    sources: Sources, indexes: dict[str, dict[str, list[RenderLeaf]]]
) -> set[RenderLeaf]:
    discovered: set[RenderLeaf] = set()
    call_pattern = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")

    swift_construct = re.compile(r"\b(?:ForEach|List|Table)\s*\(")
    for source_root in SWIFT_SOURCE_ROOTS:
        for relative in sources.files(source_root, ".swift"):
            masked = sources.masked(relative)
            spans = swift_type_spans(masked)
            for start, parameters, body in repeated_regions(masked, swift_construct):
                owner = enclosing_type(spans, start)
                regions = [body] if body is not None else []
                content_body = lambda_body(parameters, "content")
                if content_body is not None:
                    regions.append(content_body)
                for region in regions:
                    for call in call_pattern.finditer(region):
                        leaf = resolved_leaf(
                            indexes, "swift", call.group(1), relative, owner
                        )
                        if leaf is not None:
                            discovered.add(leaf)
                for reference in re.finditer(
                    r"\bcontent\s*:\s*([A-Za-z_][A-Za-z0-9_]*)(?:\.init)?\b",
                    parameters,
                ):
                    leaf = resolved_leaf(
                        indexes, "swift", reference.group(1), relative, owner
                    )
                    if leaf is not None:
                        discovered.add(leaf)

    kotlin_construct = re.compile(r"\bitems(?:Indexed)?\s*\(")
    for relative in sources.files(KOTLIN_SOURCE_ROOT, ".kt"):
        masked = sources.masked(relative)
        for _, parameters, body in repeated_regions(masked, kotlin_construct):
            regions = [body] if body is not None else []
            item_content = lambda_body(parameters, "itemContent")
            if item_content is not None:
                regions.append(item_content)
            for region in regions:
                for call in call_pattern.finditer(region):
                    leaf = resolved_leaf(
                        indexes, "kotlin", call.group(1), relative, None
                    )
                    if leaf is not None:
                        discovered.add(leaf)
    return discovered


def leaf_key(leaf: RenderLeaf) -> tuple[str, str, str]:
    return (leaf.language, leaf.path, leaf.symbol)


def all_render_leaves(
    sources: Sources, indexes: dict[str, dict[str, list[RenderLeaf]]]
) -> tuple[RenderLeaf, ...]:
    leaves = {leaf_key(leaf): leaf for leaf in RENDER_LEAVES}
    for leaf in discovered_render_leaves(sources, indexes):
        leaves.setdefault(leaf_key(leaf), leaf)
    return tuple(leaves[key] for key in sorted(leaves))


# A view built by name: `Child(...)`, `Child { ... }`, `Child<T>(...)`,
# `Child.init(...)`, `self.child(...)`, or a `Child.init` reference handed to a
# builder. A lowercase name followed by `{` is a control-flow subject
# (`switch section {`), not a construction, so only calls count for it. A
# `.name(...)` call is a modifier only when it resolves to `extension View`.
SWIFT_CHILD_CONSTRUCTION = re.compile(
    r"(?:(?<![.\w])|(?<=\bself\.)|(?<=\bSelf\.))"
    r"(?:([A-Z][A-Za-z0-9_]*)(?:\s*<[^<>(){}]*>)?(?:\.init\b|\s*[({])"
    r"|([a-z_][A-Za-z0-9_]*)\s*\()"
    r"|(?<=[^.\w]\.)(?<!\bself\.)(?<!\bSelf\.)([a-z_][A-Za-z0-9_]*)\s*\("
)
KOTLIN_CHILD_CONSTRUCTION = re.compile(
    r"(?<![.\w])(?:([A-Z][A-Za-z0-9_]*)\s*[({]|([a-z_][A-Za-z0-9_]*)\s*\()"
)
DECLARATION_KEYWORD = re.compile(
    r"\b(?:struct|class|enum|actor|extension|protocol|typealias|func|fun)\s*$"
)


def child_views(
    view: RenderLeaf,
    fragments: list[SourceFragment],
    indexes: dict[str, dict[str, list[RenderLeaf]]],
) -> list[RenderLeaf]:
    """The views `view` builds, in source order, excluding its own members."""
    pattern = (
        SWIFT_CHILD_CONSTRUCTION
        if view.language == "swift"
        else KOTLIN_CHILD_CONSTRUCTION
    )
    # Members of a view struct are already part of its fragments; a view
    # function's sibling members are not, and are followed like any child.
    caller_owner = view.owner if view.callable_declaration else view.symbol
    children: list[RenderLeaf] = []
    for fragment in fragments:
        masked = fragment.masked
        for match in pattern.finditer(masked):
            if (
                DECLARATION_KEYWORD.search(
                    masked, max(0, match.start() - 32), match.start()
                )
                is not None
            ):
                continue
            modifier = match.lastindex == 3
            symbol = match.group(match.lastindex)
            child = resolved_leaf(
                indexes, view.language, symbol, fragment.path, caller_owner
            )
            if child is None or leaf_key(child) == leaf_key(view):
                continue
            if modifier and child.owner != "View":
                continue
            if not view.callable_declaration and child.owner == view.symbol:
                continue
            if child not in children:
                children.append(child)
    return children


def check(root: Path) -> list[str]:
    violations: list[str] = []
    sources = Sources(root)
    try:
        indexes = declaration_indexes(sources)
        leaves = all_render_leaves(sources, indexes)
    except ValueError as error:
        return [str(error)]
    # Each repeated row, then every view under one, breadth first, so a view
    # is reported once, from the first row that reaches it by the shortest
    # path. A view that is itself repeated is reported as a row.
    pending: deque[tuple[RenderLeaf, tuple[str, ...]]] = deque(
        (leaf, ()) for leaf in leaves
    )
    reached = {leaf_key(leaf) for leaf in leaves}
    open_findings = {
        (finding.language, finding.path, finding.symbol): finding
        for finding in OPEN_CHILD_FINDINGS
    }
    while pending:
        view, under = pending.popleft()
        if not (root / view.path).is_file():
            violations.append(f"{view.path}: missing inventoried render leaf")
            continue
        try:
            fragments = leaf_fragments(sources, view)
        except ValueError as error:
            violations.append(f"{view.path}: {error}")
            continue
        subject = (
            view.symbol
            if not under
            else f"{view.symbol} (built under repeated {' > '.join(under)})"
        )
        found: list[str] = []
        for fragment in fragments:
            found.extend(fragment_owner_violations(fragment, view, subject))
            found.extend(fragment_environment_violations(fragment, view, subject))
            found.extend(fragment_projection_violations(fragment, subject))
        finding = open_findings.pop(leaf_key(view), None)
        if finding is not None:
            held = {
                f"reaches entity-data owner {finding.owner}",
                f"takes {finding.owner} from the environment",
            }
            remaining = [
                item
                for item in found
                if not any(item.endswith(suffix) for suffix in held)
            ]
            if len(remaining) == len(found):
                violations.append(
                    f"{view.path}: {view.symbol} no longer reaches "
                    f"{finding.owner}; remove it from OPEN_CHILD_FINDINGS"
                )
            found = remaining
        violations.extend(found)
        try:
            children = child_views(view, fragments, indexes)
        except ValueError as error:
            violations.append(f"{view.path}: {error}")
            continue
        for child in children:
            if leaf_key(child) in reached:
                continue
            reached.add(leaf_key(child))
            pending.append((child, (*under, view.symbol)))
    for _, path, symbol in sorted(open_findings):
        violations.append(
            f"{path}: {symbol} is no longer under a repeated row; remove it "
            "from OPEN_CHILD_FINDINGS"
        )
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
