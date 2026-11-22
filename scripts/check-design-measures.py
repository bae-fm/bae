#!/usr/bin/env python3
"""Refuse a literal text size, weight or line height, and a literal spacing or
corner radius, in the design system's component CSS.

Every text style a component draws comes from a type token of
design/system/tokens.json, through the variables tokens.css gives each style
(`--type-<style>-size`, `--type-<style>-weight` and
`--type-<style>-line-height`), so a component's style cannot drift from its
token. A glyph drawn in a symbol font takes its size from an icon token
(`--icon-<role>`, `--icon-android-<role>`) the same way. A `font`,
`font-size`, `font-weight` or `line-height` declaration may hold variables,
`inherit` and `normal`, and nothing else that sets a size, weight or line
height: no number and no weight word.

Padding, margins and gaps take a spacing token (`--space-<role>`) and corner
radii a radius token (`--radius-<role>`). Such a declaration may hold
variables, factors in a calc() such as `-1 *` or `/ 2`, `0` and `auto`, and
no length in px, em or rem, except a radius of `999px`, which rounds a corner
as far as its box allows.

The few declarations that draw something other than bae's own text and
layout keep their own measures, each named in ALLOWED with why.

  python3 scripts/check-design-measures.py [<file>...]

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
    (".bae-scan-qr", "line-height"): "the QR code's box holds a drawing and no line of text",
    (".bae-m-window", "border-radius"): "the drawn macOS window's corners are the system's",
    (".bae-m-mainwin", "border-radius"): "the drawn macOS window's corners are the system's",
    (".bae-m-storewin", "border-radius"): "the drawn macOS window's corners are the system's",
    (".bae-m-lights span", "border-radius"): "the drawn traffic lights are the system's",
    (".bae-m-titlebar", "padding"): "the title bar's leading padding clears the system's traffic lights",
    (".bae-m-storewin-bar", "padding"): "the toolbar's leading padding clears the system's traffic lights",
    (".bae-m-alert", "margin-top"): "the drawn macOS alert is the system's",
    (".bae-m-alert", "padding"): "the drawn macOS alert is the system's",
    (".bae-m-alert", "gap"): "the drawn macOS alert is the system's",
    (".bae-a-key", "border-radius"): "the drawn Android keyboard is the system's",
    (".bae-a-keyboard-gesture::before", "border-radius"): "the drawn gesture bar is the system's",
}

TYPE_PROPERTIES = ("font", "font-size", "font-weight", "line-height")
SPACING = re.compile(r"(?:padding|margin)(?:-[\w-]+)?|gap|row-gap|column-gap")
RADIUS = re.compile(r"border(?:-[\w-]+)?-radius")
COMMENT = re.compile(r"/\*.*?\*/", re.S)
RULE = re.compile(r"([^{}]*)\{([^{}]*)\}")
DECLARATION = re.compile(r"(?:^|;)\s*([\w-]+)\s*:\s*([^;]*)")
LITERAL = re.compile(r"\d|\b(?:bold|bolder|lighter)\b")
LENGTH = re.compile(r"(?<![\w.-])(\d*\.?\d+)(?:px|em|rem)\b")


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


def why_refused(prop: str, value: str) -> str | None:
    """Why the declaration <prop>: <value> is refused, or None."""
    bare = without_variables(value)
    if prop in TYPE_PROPERTIES:
        if LITERAL.search(bare):
            return "take it from a type token's variable (--type-<style>-size, -weight, -line-height)"
    elif SPACING.fullmatch(prop):
        if any(float(m.group(1)) for m in LENGTH.finditer(bare)):
            return "take it from a spacing token (--space-<role>)"
    elif RADIUS.fullmatch(prop):
        if any(float(m.group(1)) and m.group(0) != "999px" for m in LENGTH.finditer(bare)):
            return "take it from a radius token (--radius-<role>), or 999px for a fully round corner"
    return None


def problems(path: str, text: str) -> list[str]:
    """Each literal measure in the CSS <text> of <path>, as `path:line: why`."""
    blank = COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group()), text)
    found = []
    for rule in RULE.finditer(blank):
        selector = " ".join(rule.group(1).split())
        for declaration in DECLARATION.finditer(rule.group(2)):
            prop, value = declaration.group(1), declaration.group(2).strip()
            if (selector, prop) in ALLOWED:
                continue
            why = why_refused(prop, value)
            if why:
                line = blank.count("\n", 0, rule.start(2) + declaration.start(1)) + 1
                found.append(f"{path}:{line}: `{selector}` sets {prop}: {value}; {why}")
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
