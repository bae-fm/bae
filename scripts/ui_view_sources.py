"""Swift and Kotlin view sources as the UI projection boundary check reads
them: comments and literals masked, view declarations indexed, the rows each
repetition builds discovered, and the views each view builds resolved the way
the language resolves them."""

from __future__ import annotations

import re
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


SWIFT_SOURCE_ROOTS = (
    "bae-macos/bae/bae",
    "bae-ios/bae/bae",
)
KOTLIN_SOURCE_ROOT = "bae-android/app/src/main/java"


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
