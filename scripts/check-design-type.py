#!/usr/bin/env python3
"""Refuse a literal text size, weight or line height in the design system's
component CSS.

Every text style a component draws comes from a type token of
design/system/tokens.json, through the variables tokens.css gives each style
(`--type-<style>-size`, `--type-<style>-weight` and
`--type-<style>-line-height`), so a component's style cannot drift from its
token. A `font`, `font-size`, `font-weight` or `line-height` declaration may
hold variables, `inherit` and `normal`, and nothing else that sets a size,
weight or line height: no number and no weight word.

The few declarations that draw something other than text keep their own
measures, each named in ALLOWED with why.

  python3 scripts/check-design-type.py [<file>...]

With no file, the check reads every component CSS file.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

SOURCES = "design/system/components/src/"

# (selector, property): why the declaration keeps a literal measure.
ALLOWED = {
    (".bae-a-key", "font"): "the drawn Android keyboard is the system's, not bae's text",
    (".bae-a-key-mod", "font-size"): "the drawn Android keyboard is the system's, not bae's text",
    (".bae-msym", "font-size"): "a Material Symbols glyph fills its SVG's 24-unit view box",
    (".bae-msym", "font-weight"): "Material Symbols draws its glyphs at weight 400",
    (".bae-scan-qr", "line-height"): "the QR code's box holds a drawing and no line of text",
}

TYPE_PROPERTIES = ("font", "font-size", "font-weight", "line-height")
COMMENT = re.compile(r"/\*.*?\*/", re.S)
RULE = re.compile(r"([^{}]*)\{([^{}]*)\}")
DECLARATION = re.compile(r"(?:^|;)\s*([\w-]+)\s*:\s*([^;]*)")
LITERAL = re.compile(r"\d|\b(?:bold|bolder|lighter)\b")


def without_variables(value: str) -> str:
    """<value> with each `var(...)`, nested ones included, taken out."""
    out, depth, at = [], 0, 0
    while at < len(value):
        if depth == 0 and value.startswith("var(", at):
            depth, at = 1, at + 4
            continue
        if depth:
            depth += {"(": 1, ")": -1}.get(value[at], 0)
        else:
            out.append(value[at])
        at += 1
    return "".join(out)


def problems(path: str, text: str) -> list[str]:
    """Each literal type measure in the CSS <text> of <path>, as `path:line: why`."""
    blank = COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group()), text)
    found = []
    for rule in RULE.finditer(blank):
        selector = " ".join(rule.group(1).split())
        for declaration in DECLARATION.finditer(rule.group(2)):
            prop, value = declaration.group(1), declaration.group(2).strip()
            if prop not in TYPE_PROPERTIES or (selector, prop) in ALLOWED:
                continue
            if LITERAL.search(without_variables(value)):
                line = blank.count("\n", 0, rule.start(2) + declaration.start(1)) + 1
                found.append(
                    f"{path}:{line}: `{selector}` sets {prop}: {value}; take it from a type"
                    " token's variable (--type-<style>-size, -weight, -line-height)"
                )
    return found


def main(paths: list[str]) -> int:
    if not paths:
        paths = sorted(str(path) for path in Path(SOURCES).glob("*.css"))
    found = []
    for path in paths:
        if path.startswith(SOURCES) and path.endswith(".css"):
            found += problems(path, Path(path).read_text())
    for problem in found:
        print(problem, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
