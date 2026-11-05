#!/usr/bin/env python3
"""Write design/system/tokens.css from design/system/tokens.json.

The stylesheet the design system's previews and the canvas load: one custom
property per token (colours per theme, the first theme on :root) and one class
per text style, laid out as the claude.ai Design System page compiles it.

Usage: scripts/design/tokens-css.py
"""

import json
import pathlib

SYSTEM = pathlib.Path(__file__).resolve().parents[2] / "design" / "system"


def main() -> None:
    tokens = json.loads((SYSTEM / "tokens.json").read_text())
    themes = [t["id"] for t in tokens["color"]["themes"]]
    colors = tokens["color"]["tokens"]
    out = [f"/* {tokens['name']} — generated from tokens.json */"]
    for i, theme in enumerate(themes):
        selector = f':root, [data-theme="{theme}"]' if i == 0 else f'[data-theme="{theme}"]'
        lines = [f"{selector} {{"]
        for t in colors:
            value = t["value"] if isinstance(t["value"], str) else t["value"][theme]
            lines.append(f"  --{t['name']}: {value};")
        lines.append("}")
        out.append("\n".join(lines))
    root = [":root {"]
    for family, body in tokens.items():
        if family in ("name", "version", "meta", "color", "type"):
            continue
        for t in body["tokens"]:
            root.append(f"  --{t['name']}: {t['value']};")
    for key, stack in tokens["type"]["families"].items():
        root.append(f"  --font-{key}: {stack};")
    root.append("}")
    out.append("\n".join(root))
    for group in tokens["type"]["groups"]:
        for style in group["styles"]:
            family = style.get("family", group["family"])
            decl = [
                f"font-family: var(--font-{family})",
                f"font-size: {style['fontSize']}",
                f"font-weight: {style['fontWeight']}",
            ]
            if "lineHeight" in style:
                decl.append(f"line-height: {style['lineHeight']}")
            if "letterSpacing" in style:
                decl.append(f"letter-spacing: {style['letterSpacing']}")
            if style["name"] == "eyebrow":
                decl.append("text-transform: uppercase")
            out.append(f".{style['name']} {{ " + "; ".join(decl) + "; }")
    (SYSTEM / "tokens.css").write_text("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
