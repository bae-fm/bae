"""Row work in the apps' sources: a view that sorts, filters, joins, groups,
counts or sums the rows core handed it.

The idiom is `scripts/ui-projection-boundary.py`'s: core's query does the work
and the screen draws the projection it gets. This pass reads the tracked Swift
and Kotlin sources as tokens (a name in a comment or a string is not a use) and
finds the operations that do a query's work in the app:

- `sort`: Swift `sorted`, `sort`; Kotlin `sorted`, `sortedBy`, ...
- `filter`: Swift `filter`; Kotlin `filter`, `filterNot`, ...
- `group`: Swift `Dictionary(grouping:`; Kotlin `groupBy`, `groupingBy`
- `join`: Swift `Dictionary(uniqueKeysWithValues:`; Kotlin `associateBy`, ...
- `aggregate`: Swift `reduce`, `count(where:`, `max`, `min`; Kotlin `sumOf`, ...
- `dedup`: Swift `uniqued`; Kotlin `distinct`, `distinctBy`
- `shown-count`: a `.count` / `.size` inside a string interpolation, or as an
  argument of `pluralStringResource` / `String(localized:`

A member call counts (`rows.filter { }`, `rows?.filter { }`); a free function
(`max(a, b)`) or a constant (`Int.max`) is not row work.

`scripts/ui-row-work.toml` holds the two lists, each row keyed by path and
operation with an exact count: `frozen` (today's findings, which only shrink)
and `allowed` (uses over values that are not a screen's rows, with what they
are). A count that is too low, a row whose file or operation is gone, and a key
in both lists all fail.
"""

from __future__ import annotations

import subprocess
import tomllib
from dataclasses import dataclass
from pathlib import Path

from source_tokens import Token, bracket_pairs, tokenize

LISTS_FILE = "scripts/ui-row-work.toml"

SOURCE_ROOTS = (
    ("BaeKit/Sources", ".swift"),
    ("bae-macos/bae/bae", ".swift"),
    ("bae-ios/bae/bae", ".swift"),
    ("bae-android/app/src/main", ".kt"),
)

# Operation name -> what the view does, per language. A name followed by `(`
# or `{` (after an optional generic argument list) as a member call.
SWIFT_MEMBERS = {
    "sorted": "sorts",
    "sort": "sorts",
    "filter": "filters",
    "reduce": "aggregates",
    "max": "aggregates",
    "min": "aggregates",
    "uniqued": "dedups",
}
KOTLIN_MEMBERS = {
    "sorted": "sorts",
    "sortedBy": "sorts",
    "sortedByDescending": "sorts",
    "sortedWith": "sorts",
    "sortedDescending": "sorts",
    "sort": "sorts",
    "sortBy": "sorts",
    "sortWith": "sorts",
    "filter": "filters",
    "filterNot": "filters",
    "filterIsInstance": "filters",
    "partition": "filters",
    "groupBy": "groups",
    "groupingBy": "groups",
    "associateBy": "joins",
    "associate": "joins",
    "associateWith": "joins",
    "sumOf": "aggregates",
    "sum": "aggregates",
    "maxOf": "aggregates",
    "minOf": "aggregates",
    "maxBy": "aggregates",
    "minBy": "aggregates",
    "fold": "aggregates",
    "reduce": "aggregates",
    "distinct": "dedups",
    "distinctBy": "dedups",
}
SWIFT_INITIALIZERS = {
    "grouping": ("Dictionary(grouping:", "groups"),
    "uniqueKeysWithValues": ("Dictionary(uniqueKeysWithValues:", "joins"),
}
SWIFT_COUNT_WHERE = "count(where:"
KOTLIN_COUNT_BLOCK = "count {"
SHOWN_COUNT = "shown-count"

OPERATIONS = {
    *SWIFT_MEMBERS,
    *KOTLIN_MEMBERS,
    *(name for name, _ in SWIFT_INITIALIZERS.values()),
    SWIFT_COUNT_WHERE,
    KOTLIN_COUNT_BLOCK,
    SHOWN_COUNT,
}
VERBS = {
    **SWIFT_MEMBERS,
    **KOTLIN_MEMBERS,
    **{name: verb for name, verb in SWIFT_INITIALIZERS.values()},
    SWIFT_COUNT_WHERE: "aggregates",
    KOTLIN_COUNT_BLOCK: "aggregates",
    SHOWN_COUNT: "shows a count of",
}
SHOWN_COUNT_ARGUMENT_CALLS = ("pluralStringResource",)

REMEDY = (
    "Core's query does this work and the screen draws the projection it gets: "
    "the statement filters, sorts, joins, groups, counts and sums, in the same "
    "query or its own live query beside it "
    "(projects/bae-rules/reactive-screens-read-plain-live-queries.md in "
    "agent-instructions). Frozen rows only shrink: never add one or raise a "
    f"count. A use over values that are not a screen's rows goes in `allowed` "
    f"in {LISTS_FILE} with what they are."
)


@dataclass(frozen=True)
class Finding:
    path: str
    line: int
    operation: str


@dataclass(frozen=True)
class Row:
    path: str
    operation: str
    count: int
    # `finding` in the frozen list, `reason` in the allowed one.
    note: str


@dataclass(frozen=True)
class Lists:
    frozen: tuple[Row, ...]
    allowed: tuple[Row, ...]


def language_of(path: str) -> str:
    return "swift" if path.endswith(".swift") else "kotlin"


def skip_generics(tokens: list[Token], index: int) -> int:
    """The index after a `<...>` argument list starting at `index`, or `index`
    when none starts there."""
    if index >= len(tokens) or tokens[index].text != "<":
        return index
    depth = 0
    for cursor in range(index, min(index + 40, len(tokens))):
        if tokens[cursor].text == "<":
            depth += 1
        elif tokens[cursor].text == ">":
            depth -= 1
            if depth == 0:
                return cursor + 1
        elif tokens[cursor].text in ("{", "}", ";", "=", "&&", "||"):
            return index
    return index


def is_member(tokens: list[Token], index: int) -> bool:
    """Called on a value (`rows.max()`), not on a module (`Swift.max(a, b)`,
    the free function)."""
    if index == 0 or tokens[index - 1].text not in (".", "?."):
        return False
    return not (index > 1 and tokens[index - 2].text == "Swift")


def count_marker(tokens: list[Token], index: int, language: str) -> bool:
    """`.count` / `.size` / `.count()` at `index` shown as a number."""
    token = tokens[index]
    if token.kind != "id" or not is_member(tokens, index):
        return False
    following = tokens[index + 1].text if index + 1 < len(tokens) else ""
    if language == "swift":
        return token.text == "count" and following != "("
    if token.text == "size":
        return True
    return (
        token.text == "count"
        and following == "("
        and index + 2 < len(tokens)
        and tokens[index + 2].text == ")"
    )


def member_findings(
    path: str, tokens: list[Token], language: str
) -> list[tuple[int, str]]:
    members = SWIFT_MEMBERS if language == "swift" else KOTLIN_MEMBERS
    found: list[tuple[int, str]] = []
    for index, token in enumerate(tokens):
        if token.kind != "id":
            continue
        after = skip_generics(tokens, index + 1)
        following = tokens[after].text if after < len(tokens) else ""
        if is_member(tokens, index) and token.text in members:
            if following in ("(", "{"):
                found.append((index, token.text))
            continue
        if is_member(tokens, index) and token.text == "count":
            if language == "swift" and following == "(":
                if after + 2 < len(tokens) and (
                    tokens[after + 1].text == "where" and tokens[after + 2].text == ":"
                ):
                    found.append((index, SWIFT_COUNT_WHERE))
            elif language == "kotlin" and following == "{":
                found.append((index, KOTLIN_COUNT_BLOCK))
            continue
        if language == "swift" and token.text == "Dictionary" and not is_member(tokens, index):
            if (
                following == "("
                and after + 2 < len(tokens)
                and tokens[after + 2].text == ":"
                and tokens[after + 1].text in SWIFT_INITIALIZERS
            ):
                found.append((index, SWIFT_INITIALIZERS[tokens[after + 1].text][0]))
    return found


def shown_counts(
    path: str, tokens: list[Token], pairs: dict[int, int], language: str
) -> list[int]:
    """Token indexes of counts inside a string's interpolations, or an
    argument of a plural or localized-string call."""
    ranges: list[tuple[int, int]] = []
    for index, token in enumerate(tokens):
        if token.kind == "str":
            cursor = index + 1
            # Each interpolation is a parenthesized group right after the
            # string, or right after the previous interpolation's group.
            while cursor < len(tokens) and tokens[cursor].text == "(" and cursor in pairs:
                ranges.append((cursor, pairs[cursor]))
                cursor = pairs[cursor] + 1
        elif token.kind == "id" and index + 1 < len(tokens) and tokens[index + 1].text == "(":
            opening = index + 1
            plural = language == "kotlin" and token.text in SHOWN_COUNT_ARGUMENT_CALLS
            localized = (
                language == "swift"
                and token.text == "String"
                and opening + 2 < len(tokens)
                and tokens[opening + 1].text == "localized"
                and tokens[opening + 2].text == ":"
            )
            if (plural or localized) and opening in pairs:
                ranges.append((opening, pairs[opening]))
    hits: set[int] = set()
    for start, end in ranges:
        for cursor in range(start, end + 1):
            if count_marker(tokens, cursor, language):
                hits.add(cursor)
    return sorted(hits)


def findings(files: dict[str, str]) -> list[Finding]:
    found: list[Finding] = []
    for path in sorted(files):
        language = language_of(path)
        tokens = tokenize(files[path], language)
        pairs = bracket_pairs(tokens)
        for index, operation in member_findings(path, tokens, language):
            found.append(Finding(path, tokens[index].line, operation))
        for index in shown_counts(path, tokens, pairs, language):
            found.append(Finding(path, tokens[index].line, SHOWN_COUNT))
    return sorted(found, key=lambda item: (item.path, item.line, item.operation))


def tracked_sources(root: Path) -> list[str]:
    """The tracked Swift and Kotlin files under the app roots: the generated
    bindings are untracked, so never read."""
    listed = subprocess.run(
        ["git", "ls-files", "-z", *(source for source, _ in SOURCE_ROOTS)],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    suffixes = dict(SOURCE_ROOTS)
    return sorted(
        path
        for path in listed.split("\0")
        if path
        and any(
            path.startswith(source + "/") and path.endswith(suffix)
            for source, suffix in suffixes.items()
        )
    )


def load_lists(path: Path) -> Lists:
    data = tomllib.loads(path.read_text())

    def rows(name: str, note: str) -> tuple[Row, ...]:
        return tuple(
            Row(row["path"], row["operation"], row["count"], row[note])
            for row in data.get(name, [])
        )

    return Lists(rows("frozen", "finding"), rows("allowed", "reason"))


def violations(files: dict[str, str], lists: Lists) -> list[str]:
    problems: list[str] = []
    seen: dict[tuple[str, str], str] = {}
    for name, rows in (("frozen", lists.frozen), ("allowed", lists.allowed)):
        for row in rows:
            key = (row.path, row.operation)
            if key in seen:
                where = "twice in" if seen[key] == name else "both `frozen` and `allowed`, in"
                problems.append(
                    f"{row.path}: `{row.operation}` is listed {where} {LISTS_FILE}"
                    if seen[key] != name
                    else f"{row.path}: `{row.operation}` is listed twice in `{name}` of {LISTS_FILE}"
                )
                continue
            seen[key] = name
            if row.operation not in OPERATIONS:
                problems.append(
                    f"{row.path}: unknown operation `{row.operation}` in `{name}` of {LISTS_FILE}"
                )
            if row.count < 1:
                problems.append(
                    f"{row.path}: `{row.operation}` count must be at least 1 in `{name}` of {LISTS_FILE}"
                )
    found = findings(files)
    by_key: dict[tuple[str, str], list[Finding]] = {}
    for item in found:
        by_key.setdefault((item.path, item.operation), []).append(item)
    for (path, operation), name in sorted(seen.items()):
        if path not in files:
            problems.append(
                f"{path}: stale row for `{operation}` in `{name}` of {LISTS_FILE}: "
                "no such tracked app source"
            )
        elif (path, operation) not in by_key:
            problems.append(
                f"{path}: stale row for `{operation}` in `{name}` of {LISTS_FILE}: "
                "the file no longer has it"
            )
    counts = {
        (row.path, row.operation): row.count
        for row in (*lists.frozen, *lists.allowed)
    }
    for key, items in sorted(by_key.items()):
        if key not in seen:
            problems.extend(describe(item) for item in items)
            continue
        listed = counts[key]
        if len(items) < listed:
            problems.append(
                f"{key[0]}: `{key[1]}` is found {len(items)} times, listed {listed} "
                f"in `{seen[key]}` of {LISTS_FILE}: lower the count to {len(items)}"
            )
        elif len(items) > listed:
            problems.extend(describe(item) for item in items)
    return problems


def describe(item: Finding) -> str:
    return (
        f"{item.path}:{item.line}: view {VERBS[item.operation]} rows itself "
        f"({item.operation})"
    )


def check(root: Path) -> list[str]:
    files = {path: (root / path).read_text() for path in tracked_sources(root)}
    return violations(files, load_lists(root / LISTS_FILE))
