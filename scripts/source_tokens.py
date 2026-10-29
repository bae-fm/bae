"""Swift, Kotlin, C# and Rust sources as tokens.

The tokenizer drops comments, keeps each string literal's text out of the
code, and keeps the expressions interpolated into a string as code, so
`Text("\\(rate)")` reads `rate`: a check reads the identifiers a file's code
names, not what its comments or strings say.
"""

from __future__ import annotations

import re
from dataclasses import dataclass


@dataclass(frozen=True)
class Token:
    kind: str  # "id", "num", "str", "punct"
    text: str
    line: int
    # A line break comes before this token, so it may start a statement.
    newline: bool


PUNCTUATION = (
    "...",
    "..<",
    "?.",
    "?:",
    "!!",
    "->",
    "=>",
    "::",
    "==",
    "!=",
    "<=",
    ">=",
    "&&",
    "||",
    "+=",
    "-=",
    "*=",
    "/=",
    "..",
)
IDENTIFIER_START = re.compile(r"[A-Za-z_]")
IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NUMBER = re.compile(r"[0-9][0-9A-Za-z_.]*")


class Tokenizer:
    def __init__(self, source: str, language: str) -> None:
        self.source = source
        self.language = language
        self.index = 0
        self.line = 1
        self.newline = True
        self.tokens: list[Token] = []

    def emit(self, kind: str, text: str) -> None:
        self.tokens.append(Token(kind, text, self.line, self.newline))
        self.newline = False

    def run(self, stop_at_close: bool = False) -> None:
        """Tokenize until the end, or, inside an interpolation, until the
        unmatched `)` or `}` that closes it."""
        depth = 0
        source = self.source
        while self.index < len(source):
            char = source[self.index]
            following = source[self.index + 1] if self.index + 1 < len(source) else ""
            if char == "\n":
                self.line += 1
                self.newline = True
                self.index += 1
                continue
            if char in " \t\r\f":
                self.index += 1
                continue
            if char == "/" and following == "/":
                end = source.find("\n", self.index)
                self.index = len(source) if end == -1 else end
                continue
            if char == "/" and following == "*":
                self.block_comment()
                continue
            if self.string_start():
                continue
            if self.language == "rust" and char == "'":
                if self.rust_char_or_lifetime():
                    continue
            if self.language == "swift" and char == "`":
                end = source.index("`", self.index + 1)
                self.emit("id", source[self.index + 1 : end])
                self.index = end + 1
                continue
            if self.language == "swift" and char == "$" and following.isdigit():
                match = re.compile(r"\$[0-9]+").match(source, self.index)
                assert match is not None
                self.emit("id", match.group(0))
                self.index = match.end()
                continue
            if IDENTIFIER_START.match(char):
                match = IDENTIFIER.match(source, self.index)
                assert match is not None
                self.emit("id", match.group(0))
                self.index = match.end()
                continue
            if char.isdigit():
                match = NUMBER.match(source, self.index)
                assert match is not None
                self.emit("num", match.group(0))
                self.index = match.end()
                continue
            if stop_at_close:
                if char in "({[":
                    depth += 1
                elif char in ")}]":
                    if depth == 0:
                        return
                    depth -= 1
            for punct in PUNCTUATION:
                if source.startswith(punct, self.index):
                    self.emit("punct", punct)
                    self.index += len(punct)
                    break
            else:
                self.emit("punct", char)
                self.index += 1
        if stop_at_close:
            raise ValueError(f"unclosed interpolation at line {self.line}")

    def block_comment(self) -> None:
        depth = 0
        source = self.source
        while self.index < len(source):
            if source.startswith("/*", self.index):
                depth += 1
                self.index += 2
                continue
            if source.startswith("*/", self.index):
                depth -= 1
                self.index += 2
                if depth == 0 or self.language == "kotlin":
                    # Kotlin block comments do not nest.
                    if depth <= 0:
                        return
                continue
            if source[self.index] == "\n":
                self.line += 1
                self.newline = True
            self.index += 1
        raise ValueError("unclosed block comment")

    def string_start(self) -> bool:
        source = self.source
        index = self.index
        if self.language == "rust":
            match = re.compile(r'b?r(#*)"').match(source, index)
            if match is not None:
                closing = '"' + match.group(1)
                end = source.index(closing, match.end())
                self.skip_literal(source[index : end + len(closing)])
                return True
            if source.startswith('b"', index):
                self.index += 1
            if source[self.index] == '"':
                self.literal('"', interpolate=None)
                return True
            return False
        if self.language == "csharp":
            return self.csharp_string_start()
        if self.language == "swift":
            match = re.compile(r'(#+)("""|")').match(source, index)
            if match is not None:
                hashes = match.group(1)
                quote = match.group(2)
                self.index = match.end()
                self.literal(quote + hashes, interpolate="\\" + hashes + "(")
                return True
            if source.startswith('"""', index):
                self.index += 3
                self.literal('"""', interpolate="\\(")
                return True
            if source[index] == '"':
                self.index += 1
                self.literal('"', interpolate="\\(")
                return True
            return False
        # Kotlin
        if source.startswith('"""', index):
            self.index += 3
            self.literal('"""', interpolate="$", escapes=False)
            return True
        if source[index] == '"':
            self.index += 1
            self.literal('"', interpolate="$")
            return True
        if source[index] == "'":
            end = index + 1
            while source[end] != "'":
                end += 2 if source[end] == "\\" else 1
            self.skip_literal(source[index : end + 1])
            return True
        return False

    def csharp_string_start(self) -> bool:
        """A C# string at the cursor: raw, verbatim, interpolated, plain, or
        a character literal, its interpolated expressions read as code."""
        source = self.source
        index = self.index
        match = re.compile(r'(\$*)("{3,})').match(source, index)
        if match is not None:
            closing = match.group(2)
            end = source.index(closing, match.end())
            self.skip_literal(source[index : end + len(closing)])
            return True
        match = re.compile(r'(\$@|@\$|@|\$)?"').match(source, index)
        if match is not None:
            prefix = match.group(1) or ""
            self.index = match.end()
            self.csharp_literal(verbatim="@" in prefix, interpolated="$" in prefix)
            return True
        if source[index] == "'":
            end = index + 1
            while source[end] != "'":
                end += 2 if source[end] == "\\" else 1
            self.skip_literal(source[index : end + 1])
            return True
        return False

    def csharp_literal(self, verbatim: bool, interpolated: bool) -> None:
        source = self.source
        self.emit("str", '"')
        while True:
            if self.index >= len(source):
                raise ValueError(f"unclosed string at line {self.line}")
            char = source[self.index]
            following = source[self.index + 1 : self.index + 2]
            if char == '"':
                if verbatim and following == '"':
                    self.index += 2
                    continue
                self.index += 1
                return
            if char == "\n":
                self.line += 1
            if not verbatim and char == "\\":
                self.index += 2
                continue
            if interpolated and char == "{":
                if following == "{":
                    self.index += 2
                    continue
                self.index += 1
                self.interpolation("{", "}")
                continue
            self.index += 1

    def skip_literal(self, text: str) -> None:
        self.emit("str", text)
        self.line += text.count("\n")
        self.index += len(text)

    def literal(
        self, closing: str, interpolate: str | None, escapes: bool = True
    ) -> None:
        """Read a string literal up to `closing`, its interpolated
        expressions emitted as code between parentheses."""
        source = self.source
        if interpolate is None:
            # Rust: no interpolation.
            self.index += 1
            start = self.index
            while source[self.index] != '"':
                if source[self.index] == "\\":
                    self.index += 1
                if source[self.index] == "\n":
                    self.line += 1
                self.index += 1
            self.emit("str", source[start - 1 : self.index + 1])
            self.index += 1
            return
        self.emit("str", '"')
        while True:
            if self.index >= len(source):
                raise ValueError(f"unclosed string at line {self.line}")
            if source.startswith(closing, self.index):
                self.index += len(closing)
                return
            char = source[self.index]
            if char == "\n":
                self.line += 1
                self.index += 1
                continue
            if self.language == "swift" and source.startswith(interpolate, self.index):
                self.index += len(interpolate)
                self.interpolation("(", ")")
                continue
            if self.language == "kotlin" and char == "$":
                following = source[self.index + 1 : self.index + 2]
                if following == "{":
                    self.index += 2
                    self.interpolation("{", "}")
                    continue
                match = IDENTIFIER.match(source, self.index + 1)
                if match is not None:
                    self.emit("punct", "(")
                    self.emit("id", match.group(0))
                    self.emit("punct", ")")
                    self.index = match.end()
                    continue
            if escapes and char == "\\":
                self.index += 2
                continue
            self.index += 1

    def interpolation(self, opening: str, closing: str) -> None:
        self.emit("punct", "(")
        self.run(stop_at_close=True)
        if self.source[self.index] != closing:
            raise ValueError(f"interpolation closed by {self.source[self.index]!r}")
        self.index += 1
        self.emit("punct", ")")

    def rust_char_or_lifetime(self) -> bool:
        source = self.source
        match = re.compile(r"'(?:\\.[^']*|[^\\'\n])'").match(source, self.index)
        if match is not None:
            self.skip_literal(match.group(0))
            return True
        match = re.compile(r"'[A-Za-z_][A-Za-z0-9_]*").match(source, self.index)
        if match is not None:
            self.emit("punct", "'")
            self.index += 1
            return True
        return False


def tokenize(source: str, language: str) -> list[Token]:
    tokenizer = Tokenizer(source, language)
    tokenizer.run()
    return tokenizer.tokens


OPENING = {"(": ")", "[": "]", "{": "}"}
CLOSING = {")", "]", "}"}


def bracket_pairs(tokens: list[Token]) -> dict[int, int]:
    """Each bracket's partner, both ways. Angle brackets are not matched:
    they are also comparison operators."""
    pairs: dict[int, int] = {}
    stack: list[int] = []
    for index, token in enumerate(tokens):
        if token.kind != "punct":
            continue
        if token.text in OPENING:
            stack.append(index)
        elif token.text in CLOSING:
            if not stack or OPENING[tokens[stack[-1]].text] != token.text:
                raise ValueError(
                    f"unbalanced {token.text!r} at line {token.line}"
                )
            opening = stack.pop()
            pairs[opening] = index
            pairs[index] = opening
    if stack:
        raise ValueError(f"unclosed {tokens[stack[-1]].text!r} at line {tokens[stack[-1]].line}")
    return pairs

