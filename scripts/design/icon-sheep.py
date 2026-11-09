#!/usr/bin/env python3
"""Write design/system/assets/Logos/bae-icon-sheep.svg from bae-sheep.svg.

The app icon's sheep: the sheep's own outlines (body, ears, face, hands) 1.6
times as thick and the head's 1.3 times, all a touch lighter, so the drawing
holds up at small sizes; the headphones and the player keep their lines. The
view box grows so the thicker outline isn't cut off.

Usage: scripts/design/icon-sheep.py
"""
import pathlib
import re

LOGOS = pathlib.Path(__file__).resolve().parents[2] / "design/system/assets/Logos"
THICKER = 1.6
LINE = "#40183d"
LIGHTER_LINE = "#4C1F47"
KEEP = {"headband", "left-earcup", "right-earcup", "player"}
# The wool on the head sits under the headband; a full 1.6 there crowds it.
HEAD = "head"
HEAD_THICKER = 1.3


def main():
    src = (LOGOS / "bae-sheep.svg").read_text()
    groups = [(m.start(), m.group(1)) for m in re.finditer(r'<g id="([^"]*)"', src)]
    if not groups:
        raise SystemExit("bae-sheep.svg has no named groups")
    ends = [start for start, _ in groups[1:]] + [len(src)]
    out = [src[: groups[0][0]]]
    for (start, name), end in zip(groups, ends):
        part = src[start:end]
        if name not in KEEP:
            k = HEAD_THICKER if name == HEAD else THICKER
            part = re.sub(
                r'stroke-width="([\d.]+)"',
                lambda m, k=k: 'stroke-width="%.1f"' % (float(m.group(1)) * k),
                part,
            )
        out.append(part)
    svg = "".join(out).replace(LINE, LIGHTER_LINE).replace("#4a2e5e", LIGHTER_LINE)
    box = re.search(r'viewBox="0 0 ([\d.]+) ([\d.]+)" width="[\d.]+" height="[\d.]+"', svg)
    if not box:
        raise SystemExit("bae-sheep.svg's view box isn't 0 0 w h")
    w, h = float(box.group(1)), float(box.group(2))
    widest = max(float(x) for x in re.findall(r'stroke-width="([\d.]+)"', src))
    pad = round((THICKER - 1) * widest / 2 + 2, 1)
    svg = svg.replace(
        box.group(0),
        f'viewBox="{-pad} {-pad} {w + 2 * pad:g} {h + 2 * pad:g}" width="{w + 2 * pad:g}" height="{h + 2 * pad:g}"',
        1,
    )
    (LOGOS / "bae-icon-sheep.svg").write_text(svg)


if __name__ == "__main__":
    main()
