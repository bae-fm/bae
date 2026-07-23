#!/usr/bin/env python3
"""Build the static screenshot gallery for GitHub Pages.

Input: a directory of <scene>@<platform>.png files (flattened from the four
platform capture artifacts). Output: a site directory with the images and an
index.html grouping them scene row x platform column.

Usage: gallery.py <shots-dir> <site-dir> [--sha <commit>]
"""

import argparse
import html
import pathlib
import re
import shutil
import sys

PLATFORM_ORDER = ["macos", "ios", "android", "windows"]
PLATFORM_LABELS = {
    "macos": "macOS",
    "ios": "iOS",
    "android": "Android",
    "windows": "Windows",
}
# Known scenes render in this order; unknown scene ids sort after, named as-is.
SCENE_ORDER = ["welcome", "welcome-restore", "library-grid", "album-detail"]

SHOT_RE = re.compile(r"^(?P<scene>[a-z0-9-]+)@(?P<platform>[a-z]+)\.png$")


def collect(shots_dir: pathlib.Path) -> dict[str, dict[str, pathlib.Path]]:
    scenes: dict[str, dict[str, pathlib.Path]] = {}
    pngs = sorted(shots_dir.rglob("*.png"))
    if not pngs:
        sys.exit(f"no PNGs found under {shots_dir}")
    for png in pngs:
        m = SHOT_RE.match(png.name)
        if not m:
            sys.exit(f"unrecognized screenshot name: {png.name}")
        platform = m["platform"]
        if platform not in PLATFORM_ORDER:
            sys.exit(f"unknown platform '{platform}' in {png.name}")
        prior = scenes.setdefault(m["scene"], {})
        if platform in prior:
            sys.exit(f"duplicate shot for {m['scene']}@{platform}")
        prior[platform] = png
    return scenes


def scene_sort_key(scene: str):
    try:
        return (0, SCENE_ORDER.index(scene))
    except ValueError:
        return (1, scene)


def build(shots_dir: pathlib.Path, site_dir: pathlib.Path, sha: str) -> None:
    scenes = collect(shots_dir)
    site_dir.mkdir(parents=True, exist_ok=True)
    shots_out = site_dir / "shots"
    shots_out.mkdir(exist_ok=True)

    rows = []
    for scene in sorted(scenes, key=scene_sort_key):
        cells = []
        for platform in PLATFORM_ORDER:
            png = scenes[scene].get(platform)
            if png is None:
                cells.append("<td class='missing'>—</td>")
                continue
            name = f"{scene}@{platform}.png"
            shutil.copyfile(png, shots_out / name)
            cells.append(
                f"<td><a href='shots/{name}'>"
                f"<img src='shots/{name}' alt='{html.escape(scene)} on "
                f"{PLATFORM_LABELS[platform]}' loading='lazy'></a></td>"
            )
        rows.append(
            f"<tr><th scope='row'>{html.escape(scene)}</th>{''.join(cells)}</tr>"
        )

    heads = "".join(f"<th scope='col'>{PLATFORM_LABELS[p]}</th>" for p in PLATFORM_ORDER)
    sha_line = (
        f"<p class='meta'>built from <code>{html.escape(sha)}</code></p>" if sha else ""
    )
    (site_dir / "index.html").write_text(f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>bae screenshots</title>
<style>
  body {{ font: 15px/1.5 -apple-system, system-ui, sans-serif; margin: 2rem;
         background: #16161a; color: #e8e8ea; }}
  table {{ border-collapse: collapse; width: 100%; }}
  th, td {{ padding: 0.6rem; text-align: left; vertical-align: top;
            border-bottom: 1px solid #2a2a30; }}
  th[scope=row] {{ white-space: nowrap; font-family: ui-monospace, monospace;
                   font-weight: 600; }}
  img {{ max-width: 340px; max-height: 260px; width: auto; height: auto;
         border-radius: 6px; border: 1px solid #2a2a30; display: block; }}
  td.missing {{ color: #55555e; }}
  .meta {{ color: #8a8a94; }}
  a {{ color: inherit; }}
</style>
</head>
<body>
<h1>bae screenshots</h1>
{sha_line}
<table>
<thead><tr><th scope='col'>scene</th>{heads}</tr></thead>
<tbody>
{chr(10).join(rows)}
</tbody>
</table>
</body>
</html>
""")
    print(f"gallery: {len(scenes)} scene(s) -> {site_dir}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("shots_dir", type=pathlib.Path)
    parser.add_argument("site_dir", type=pathlib.Path)
    parser.add_argument("--sha", default="")
    args = parser.parse_args()
    build(args.shots_dir, args.site_dir, args.sha)


if __name__ == "__main__":
    main()
