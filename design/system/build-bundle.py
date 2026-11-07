#!/usr/bin/env python3
"""Build components/bundle.js from components/src/*.js.

The design system's format takes one classic script, components/bundle.js. Its
sources are split by area in components/src/, read in file-name order: the first
opens the bundle's function and the last (99-export.js) registers every
component on window.Bae and closes it. Line 1 of the result is the header the
format reads: the namespace and the components in export order. The sheep from
assets/Logos goes in as a data URI, so the bundle needs no files beside it.

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
sheep = components.parent / "assets" / "Logos" / "bae-sheep.svg"
body = body.replace("@@bae-sheep.svg@@", base64.b64encode(sheep.read_bytes()).decode())
exports = body[body.rindex("window.Bae = Object.assign(window.Bae || {}, {"):]
names = re.findall(r"(\w+): \1\b", exports)
if not names:
    sys.exit("99-export.js names no components")
header = {"format": 4, "namespace": "Bae", "components": [{"name": name} for name in names]}
(components / "bundle.js").write_text("/* @ds-bundle: " + json.dumps(header, separators=(",", ":")) + " */\n" + body)
print(f"bundle.js: {len(sources)} sources, {len(names)} components")
