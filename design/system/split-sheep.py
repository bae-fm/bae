#!/usr/bin/env python3
"""Write the sheep's moving parts as separate drawings from icon-source.svg.

ImportSheep animates the logo's own drawing, so each part it moves is cut out of
icon-source.svg and saved in assets/Logos/sheep-parts/, one file per part. Every
file has the logo's viewBox, so the parts stack in place with no offsets, and
each carries the shapes it uses and the outline's stroke settings the logo's
outer group gives it.

Usage: split-sheep.py [--check]   (--check fails when a file is out of date)
"""
import pathlib
import sys
import xml.etree.ElementTree as ET

SVG = "http://www.w3.org/2000/svg"
XLINK = "http://www.w3.org/1999/xlink"
ET.register_namespace("", SVG)
ET.register_namespace("xlink", XLINK)

ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE = ROOT / "icon-source.svg"
OUT = pathlib.Path(__file__).with_name("assets") / "Logos" / "sheep-parts"


def tag(name):
    return f"{{{SVG}}}{name}"


# Each part: its file name, the logo's group it comes from, and which of that
# group's children (from, to) it takes; the whole group when none.
PARTS = [
    ("headband", "headband", None),
    ("earcup-left", "left-earcup", None),
    ("body", "body", None),
    ("ear-left", "ears", (0, 3)),
    ("ear-right", "ears", (3, 6)),
    # The face's fill, wool and outline; its two cheeks; the mouth and tongue; the closed eyes.
    ("face", "face", (0, 3)),
    ("cheek-left", "face", (3, 4)),
    ("cheek-right", "face", (4, 5)),
    ("mouth", "face", (5, 7)),
    ("eyes", "face", (7, 8)),
    ("wool", "head", None),
    ("earcup-right", "right-earcup", None),
    # The player's case and screen, the heart on the screen, the screen's bubbles.
    ("player", "player", (0, 4)),
    ("heart", "player", (4, 5)),
    ("bubbles", "player", (5, 6)),
    ("hand-left", "hands", (0, 3)),
    ("hand-right", "hands", (3, 6)),
]


def render(root, name, group_id, span):
    group = next(el for el in root.iter() if el.get("id") == group_id)
    children = list(group) if span is None else list(group)[span[0]:span[1]]
    return assemble(root, name, children, group.get("stroke"))


def assemble(root, name, children, stroke_override):
    body = ET.Element(tag("g"), {"stroke": "#40183d", "stroke-linejoin": "round", "stroke-linecap": "round"})
    # The headband's group turns the outline off for its own layers.
    holder = ET.SubElement(body, tag("g"), {"stroke": stroke_override} if stroke_override else {})
    for child in children:
        holder.append(child)
    return wrap(root, name, body)


def references(el):
    """The ids an element's subtree points at, through href and url(#id)."""
    found = set()
    for node in el.iter():
        for key, value in node.attrib.items():
            if key == f"{{{XLINK}}}href":
                found.add(value[1:])
            elif "url(#" in value:
                found.add(value[value.index("url(#") + 5:value.index(")")])
    return found


def wrap(root, name, body):
    shapes = {el.get("id"): el for el in root.find(tag("defs"))}
    needed, pending = set(), references(body)
    while pending:
        id_ = pending.pop()
        if id_ not in needed:
            needed.add(id_)
            pending |= references(shapes[id_])
    defs = ET.Element(tag("defs"))
    for id_, el in shapes.items():
        if id_ in needed:
            defs.append(el)
    svg = ET.Element(tag("svg"), {"viewBox": "0 0 584 545", "width": "584", "height": "545"})
    if len(defs):
        svg.append(defs)
    svg.append(body)
    ET.indent(svg, space="")
    return ET.tostring(svg, encoding="unicode") + "\n"


def screen_glow(root):
    """The player's screen alone, filled white with no outline: it flashes over the player."""
    screen = list(next(el for el in root.iter() if el.get("id") == "player"))[3]
    flash = ET.Element(tag("path"), {"d": screen.get("d"), "fill": "#ffffff", "stroke": "none"})
    return wrap(root, "screen-glow", flash)


def face_clip(root):
    """The face's shape filled solid: the mask that keeps the features inside the face."""
    use = ET.Element(tag("use"), {f"{{{XLINK}}}href": "#face-shape", "fill": "#000000", "stroke": "none"})
    return wrap(root, "face-clip", use)


def files():
    out = {}
    for name, group, span in PARTS:
        # Re-parse for every part: appending a child to a new parent moves it.
        out[f"{name}.svg"] = render(ET.parse(SOURCE).getroot(), name, group, span)
    out["screen-glow.svg"] = screen_glow(ET.parse(SOURCE).getroot())
    out["face-clip.svg"] = face_clip(ET.parse(SOURCE).getroot())
    return out


def main():
    check = sys.argv[1:] == ["--check"]
    if sys.argv[1:] and not check:
        sys.exit(__doc__)
    stale = []
    expected = files()
    for name, text in expected.items():
        path = OUT / name
        if check:
            if not path.exists() or path.read_text() != text:
                stale.append(name)
        else:
            OUT.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    if check:
        extra = sorted(p.name for p in OUT.glob("*.svg") if p.name not in expected) if OUT.exists() else []
        if stale or extra:
            sys.exit(f"sheep parts out of date ({', '.join(stale + extra)}): run design/system/split-sheep.py")


main()
