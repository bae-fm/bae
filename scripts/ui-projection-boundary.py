#!/usr/bin/env python3
"""Keep render-only repeated children on their declared projection inputs.

The idiom this enforces: a view draws one of three things, and only these —
the rows of a live query, its own row's data as its parent passes it in, or a
keyed stream for a value no table holds (an import's progress). Views never
cache, join, filter or sort data themselves; core hands them the rows.

A repeated row's inputs are what its parent passes in, and so are the inputs of
every view the row builds: a child that reads a store redraws, and reads, once
per row just as the row itself would. The check therefore follows each repeated
row's child views, transitively and wherever they are declared, and holds each
one to the same inputs as the row. A leaf that reads a keyed stream
(`KEYED_STREAMS`) by its own key passes: it reads one value for the key its
parent gave it, which no row's projection carries.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import deque
from dataclasses import dataclass
from pathlib import Path

from ui_view_sources import (
    RenderLeaf,
    SourceFragment,
    Sources,
    child_views,
    declaration_indexes,
    declaration_source,
    discovered_render_leaves,
    leaf_fragments,
    leaf_key,
    mask_comments_and_literals,
)


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
OPEN_CHILD_FINDINGS: tuple[OpenFinding, ...] = ()


@dataclass(frozen=True)
class KeyedStream:
    owner: str
    # The owner's method that reads one key's current value, taking the key as
    # its one argument.
    read: str
    reason: str


# Values no table holds, delivered as one shared signal a leaf filters to its
# own key, with a read of the key's current value for when the leaf appears.
# A Swift view that takes `owner` from the environment and uses it only to
# call `read` with one of its own `let` inputs passes; any other use of the
# owner is a finding. Frozen like the findings: an entry no view under a
# repeated row reads fails until it is removed.
KEYED_STREAMS = (
    KeyedStream(
        "Importer",
        "candidateRuntime",
        "what identification and the import are doing for a candidate right "
        "now, which core sends as it moves and keeps in no table",
    ),
    KeyedStream(
        "OutboxStore",
        "fileShare",
        "the latest share a file being sent has moved, which coven's upload "
        "watch tells and no table keeps; the leaf hands it to a native view "
        "that reads it each display frame",
    ),
    KeyedStream(
        "OutboxStore",
        "uploadProgress",
        "the latest progress of a release's upload, which core measures from "
        "coven's upload watch and keeps in no table; the leaf hands it to a "
        "native view that reads it each display frame",
    ),
    KeyedStream(
        "DownloadStore",
        "downloadProgress",
        "the latest bytes a release's download has moved, which core sends as "
        "they arrive and keeps in no table; the leaf hands it to a native view "
        "that reads it each display frame",
    ),
    KeyedStream(
        "OutboxStore",
        "releaseTransfer",
        "the stream of how far and how fast a release's upload goes, which "
        "core measures from coven's upload watch and keeps in no table; the "
        "leaf hands it to a native view that sets itself from it",
    ),
    KeyedStream(
        "DownloadStore",
        "releaseTransfer",
        "the stream of the bytes a release's download has moved, which core "
        "sends as they arrive and keeps in no table; the leaf hands it to a "
        "native view that sets itself from it",
    ),
    KeyedStream(
        "DownloadStore",
        "fileShare",
        "the latest share a file being downloaded to keep whole has moved, "
        "which coven's download watch tells and no table keeps; the leaf hands "
        "it to a native view that reads it each display frame",
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

FORBIDDEN_SECONDARY_PROJECTIONS = (
    "Candidate",
    "BridgeImportCandidateDetail",
)


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


SWIFT_STORED_LET = re.compile(
    r"(?<!static )\blet\s+([A-Za-z_][A-Za-z0-9_]*)\s*:"
)


def swift_view_inputs(fragment: SourceFragment) -> set[str]:
    """The `let` properties a view struct declares at its top level: what its
    parent passes in."""
    masked = fragment.masked
    inputs: set[str] = set()
    for match in SWIFT_STORED_LET.finditer(masked):
        depth = masked.count("{", 0, match.start()) - masked.count(
            "}", 0, match.start()
        )
        if depth == 1:
            inputs.add(match.group(1))
    return inputs


def keyed_stream_reads(
    fragments: list[SourceFragment], view: RenderLeaf
) -> dict[str, set[KeyedStream]]:
    """The owners `view` reaches only to read a keyed stream by one of its own
    inputs, each with the streams it reads.

    The shape is a Swift view struct taking the owner from the environment,
    unwrapping it, and calling a declared read with a `let` input as the key;
    any other mention of the owner or its instance is not the shape.
    """
    if view.language != "swift" or view.callable_declaration:
        return {}
    inputs = swift_view_inputs(fragments[0])
    allowed: dict[str, set[KeyedStream]] = {}
    for owner in sorted({stream.owner for stream in KEYED_STREAMS}):
        streams = {
            stream.read: stream for stream in KEYED_STREAMS if stream.owner == owner
        }
        injection = re.compile(
            rf"@Environment\s*\(\s*{re.escape(owner)}\.self\s*\)"
            r"(?:\s+(?:private|fileprivate|internal|public))*"
            r"\s+var\s+([A-Za-z_][A-Za-z0-9_]*)"
            rf"(?:\s*:\s*{re.escape(owner)}\s*\??)?"
        )
        names: set[str] = set()
        # Each fragment with the owner's injection blanked out, position for
        # position: what is left must use the owner only through the reads.
        rest: list[str] = []
        for fragment in fragments:
            for match in injection.finditer(fragment.masked):
                names.add(match.group(1))
            rest.append(
                injection.sub(lambda match: " " * len(match.group(0)), fragment.masked)
            )
        if not names or any(
            re.search(rf"\b{re.escape(owner)}\b", text) for text in rest
        ):
            continue
        read: set[KeyedStream] = set()
        shaped = True
        for text in rest:
            for name in names:
                for use in re.finditer(rf"(?<!\w){re.escape(name)}\b", text):
                    before = text[max(0, use.start() - 32) : use.start()]
                    if before.endswith(".") and not re.search(
                        r"\bself\s*\.$", before
                    ):
                        # Another value's member of the same name.
                        continue
                    stream = keyed_read(text[use.start() :], name, streams, inputs)
                    if stream is not None:
                        read.add(stream)
                    elif not swift_unwraps(before, text[use.end() :]):
                        shaped = False
        if shaped and read:
            allowed[owner] = read
    return allowed


def keyed_read(
    use: str,
    name: str,
    streams: dict[str, KeyedStream],
    inputs: set[str],
) -> KeyedStream | None:
    """The stream `use`, which starts at the owner's instance `name`, reads:
    one of `streams` called with one of the view's `inputs` as its key, its
    argument labelled or not."""
    call = re.match(
        rf"{re.escape(name)}\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)"
        r"\s*\(\s*(?:[A-Za-z_][A-Za-z0-9_]*\s*:\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\)",
        use,
    )
    if call is None or call.group(2) not in inputs:
        return None
    return streams.get(call.group(1))


def swift_unwraps(before: str, after: str) -> bool:
    """Whether a name between `before` and `after` is `guard let name` or
    `if let name` unwrapping itself."""
    return (
        re.search(r"\b(?:guard|if)\s+let\s+$", before) is not None
        and re.match(r"\s*=", after) is None
    )


def all_render_leaves(
    sources: Sources, indexes: dict[str, dict[str, list[RenderLeaf]]]
) -> tuple[RenderLeaf, ...]:
    leaves = {leaf_key(leaf): leaf for leaf in RENDER_LEAVES}
    for leaf in discovered_render_leaves(sources, indexes):
        leaves.setdefault(leaf_key(leaf), leaf)
    return tuple(leaves[key] for key in sorted(leaves))


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
    unread_streams = set(KEYED_STREAMS)
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
        for owner, streams in keyed_stream_reads(fragments, view).items():
            unread_streams.difference_update(streams)
            found = [
                item
                for item in found
                if not item.endswith(owner_suffixes(owner))
            ]
        finding = open_findings.pop(leaf_key(view), None)
        if finding is not None:
            remaining = [
                item
                for item in found
                if not item.endswith(owner_suffixes(finding.owner))
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
    for stream in sorted(
        unread_streams, key=lambda stream: (stream.owner, stream.read)
    ):
        violations.append(
            f"{stream.owner}.{stream.read}: no view under a repeated row reads "
            "it by its own key; remove it from KEYED_STREAMS"
        )
    return violations


def owner_suffixes(owner: str) -> tuple[str, str]:
    """The endings of the two findings a view reaching `owner` produces."""
    return (
        f"reaches entity-data owner {owner}",
        f"takes {owner} from the environment",
    )


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
