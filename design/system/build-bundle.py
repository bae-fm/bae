#!/usr/bin/env python3
"""Build components/bundle.js from components/src/*.js, and components/bundle.css from components/src/*.css.

The design system's format takes one classic script, components/bundle.js. Its
sources are split by area in components/src/, read in file-name order: the first
opens the bundle's function and the last (99-export.js) registers every
component on window.Bae and closes it. Line 1 of the result is the header the
format reads: the namespace and the components, in the order groups.json lists
them. groups.json is the design system's table of contents: its groups in
order, each with the components it holds directly and its subgroups, each
with theirs. Each preview's @dsCard group names its component's group there. The drawings
from assets/Logos go in as data URIs, so the bundle needs no files beside it.
The style sheet's parts, also one per area, are joined in file-name order, which
is the order of the cascade.

Usage: build-bundle.py [components-dir]   (default: components beside this file)
"""
import base64
import json
import pathlib
import re
import sys

components = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).with_name("components")
sources = sorted((components / "src").glob("*.js"))
if not sources:
    sys.exit(f"no sources in {components / 'src'}")
body = "".join(path.read_text() for path in sources)
logos = components.parent / "assets" / "Logos"
artwork = components.parent / "assets" / "Artwork"
pictures = {}
for title in json.loads((artwork / "titles.json").read_text()):
    png = (artwork / (title + ".png")).read_bytes()
    pictures[title] = "data:image/png;base64," + base64.b64encode(png).decode()
body = body.replace("/* board artwork */ {}", json.dumps(pictures))
body = re.sub(r"@@([\w.-]+)@@", lambda m: base64.b64encode((logos / m.group(1)).read_bytes()).decode(), body)
exports = body[body.rindex("window.Bae = Object.assign(window.Bae || {}, {"):]
names = re.findall(r"(\w+): \1\b", exports)
if not names:
    sys.exit("99-export.js names no components")


groups_file = components.parent / "groups.json"
order = {}
for group in json.loads(groups_file.read_text()):
    for listed in [group.get("components", [])] + [sub["components"] for sub in group.get("subgroups", [])]:
        for name in listed:
            order[name] = (len(order), group["name"])
for name in names:
    if name not in order:
        sys.exit(f"{groups_file.name} lists no group for the component {name}")
    first = (components / name / "preview.html").read_text().split("\n", 1)[0]
    found = re.search(r'group="([^"]+)"', first)
    if not found or found.group(1) != order[name][1]:
        sys.exit(f'the first line of {name}/preview.html must give group="{order[name][1]}", its group in {groups_file.name}')
names.sort(key=lambda name: order[name][0])
header = {"format": 4, "namespace": "Bae", "components": [{"name": name} for name in names]}
(components / "bundle.js").write_text("/* @ds-bundle: " + json.dumps(header, separators=(",", ":")) + " */\n" + body)
print(f"bundle.js: {len(sources)} sources, {len(names)} components")
styles = sorted((components / "src").glob("*.css"))
if not styles:
    sys.exit(f"no style sheets in {components / 'src'}")
(components / "bundle.css").write_text("".join(path.read_text() for path in styles))
print(f"bundle.css: {len(styles)} parts")
