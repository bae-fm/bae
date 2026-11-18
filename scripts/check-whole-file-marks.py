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

# Test files that start tasks and threads with the spawns clippy.toml
# disallows, under one whole-file mark of this lint each. The list only
# shrinks: a file may keep its mark, for this lint alone, and a file whose
# mark is gone leaves the list.
FROZEN_LINT = "clippy::disallowed_methods"
FROZEN = {
    "bae-app/tests/integration/test_import_live_query_reruns.rs",
    "bae-app/tests/integration/test_live_reads.rs",
    "bae-app/tests/integration/test_scan_cost.rs",
    "bae-app/tests/integration/test_startup_sync.rs",
    "bae-audio/src/decode_tests.rs",
    "bae-audio/src/decode/seek_tests.rs",
    "bae-audio/src/sparse_buffer_tests.rs",
    "bae-audio/src/stream_wake_tests.rs",
    "bae-audio/src/track_stream_tests.rs",
    "bae-bridge/src/handle_tests.rs",
    "bae-bridge/src/handle/live_tests.rs",
    "bae-bridge/src/host_tests.rs",
    "bae-bridge/src/operation_runtime_tests.rs",
    "bae-bridge/src/types/configuration/settings_tests.rs",
    "bae-cast/src/airplay/ap2_session_tests.rs",
    "bae-cast/src/airplay/fake_receiver.rs",
    "bae-cast/src/airplay/pairing_tests.rs",
    "bae-cast/src/airplay/rtsp_tests.rs",
    "bae-cast/src/airplay/session_tests.rs",
    "bae-cast/src/airplay/stream_tests.rs",
    "bae-cast/src/dlna/mod_tests.rs",
    "bae-cast/src/lib_tests.rs",
    "bae-catalogs/src/discogs/client_tests.rs",
    "bae-catalogs/src/musicbrainz/mod_tests.rs",
    "bae-catalogs/src/musicbrainz/rate_tests.rs",
    "bae-catalogs/src/payloads/traversal_tests.rs",
    "bae-catalogs/src/rate_limiter_tests.rs",
    "bae-catalogs/src/remote_image_tests.rs",
    "bae-catalogs/src/remote_image/archive_tests.rs",
    "bae-catalogs/src/search/failure_tests.rs",
    "bae-catalogs/src/source_release_tests.rs",
    "bae-diagnostics/src/lib_tests.rs",
    "bae-diagnostics/src/panic_reports_tests.rs",
    "bae-import/src/catalog_sessions_tests.rs",
    "bae-import/src/folder_scanner/scan_tests.rs",
    "bae-import/src/folder_scanner/scan/root_tests.rs",
    "bae-import/src/folder_state_commit_tests.rs",
    "bae-import/src/handle/commit_tests.rs",
    "bae-import/src/handle/file_tags_tests.rs",
    "bae-import/src/handle/identification_tests.rs",
    "bae-import/src/handle/import_tests.rs",
    "bae-import/src/handle/list_tests.rs",
    "bae-import/src/identification_tests.rs",
    "bae-import/src/identification/queue_tests.rs",
    "bae-import/src/import_cancel_tests.rs",
    "bae-import/src/scan_passes_tests.rs",
    "bae-import/src/scan_passes/folder_reading_tests.rs",
    "bae-import/src/scan_passes/scanning_tests.rs",
    "bae-import/src/tables/import_state/import_commit_tests.rs",
    "bae-import/src/watching_tests.rs",
    "bae-library/src/local_lifecycle_tests.rs",
    "bae-library/src/manager/downloads_tests.rs",
    "bae-library/src/manager/locality_tests.rs",
    "bae-library/src/manager/pin_progress_tests.rs",
    "bae-library/src/manager/pin_tests.rs",
    "bae-library/src/manager/service_tests.rs",
    "bae-library/src/manager/uploads_tests.rs",
    "bae-library/tests/integration/dropbox_sign_in.rs",
    "bae-library/tests/integration/test_playback_behavior/local_sparse_buffer/queue_and_restore.rs",
    "bae-library/tests/integration/test_playback_behavior/queue_and_pregap/queue.rs",
    "bae-playback/src/device_writer_tests.rs",
    "bae-playback/src/service/mod_tests.rs",
    "bae-playback/src/service/output_tests.rs",
    "bae-playback/src/service/segment_start_mp3_tests.rs",
    "bae-playback/src/tables/queue_tests.rs",
    "bae-store/src/live_query/read_tests.rs",
    "bae-track-audio/src/data_source_tests.rs",
    "bae-track-audio/src/decode_tests.rs",
    "bae-transfers/src/queued_releases_tests.rs",
    "bae-util/src/http_tests.rs",
    "bae-util/src/retry_tests.rs",
    "bae-util/src/test_logs_tests.rs",
}


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
    marks = whole_module_marks(source)
    frozen = name in FROZEN
    violations = []
    for mark in marks:
        lints = [lint for lint in mark.lints if not (frozen and lint == FROZEN_LINT)]
        if lints:
            violations.append(
                f"{name}:{mark.line}: this mark switches off {', '.join(lints)} for a "
                "whole module; put #[expect(<lint>, reason = \"...\")] on the one "
                "function or item that does what the lint forbids"
            )
    if frozen and not any(FROZEN_LINT in mark.lints for mark in marks):
        violations.append(
            f"{name}: the file no longer marks itself whole for {FROZEN_LINT}; "
            "remove it from FROZEN in scripts/check-whole-file-marks.py"
        )
    return violations


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
    when none are named, where each frozen file must also still exist."""
    whole = names is None
    if names is None:
        names = listed_rust_files(root)
    found = []
    for name in names:
        path = root / name
        if not name.endswith(".rs") or not path.is_file():
            continue
        found.extend(file_violations(Path(name).as_posix(), path.read_text()))
    if whole:
        for name in sorted(FROZEN):
            if not (root / name).is_file():
                found.append(
                    f"{name}: the file is gone; remove it from FROZEN in "
                    "scripts/check-whole-file-marks.py"
                )
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
