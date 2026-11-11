#!/usr/bin/env python3
"""Build the static screenshot gallery for GitHub Pages.

Input: a directory of <board>@<platform>.png files (flattened from the two
platform capture artifacts), each named by its board (`mac.welcome.libraries`,
`android.welcome.libraries`). Output: a site directory with the images and an
index.html with a row for each screen (the board name after the platform
part) and a column for each platform.

Usage: gallery.py <shots-dir> <site-dir> [--sha <commit>]
"""

import argparse
import html
import pathlib
import re
import shutil
import sys

PLATFORM_ORDER = ["macos", "android"]
PLATFORM_LABELS = {
    "macos": "macOS",
    "android": "Android",
}
# A board is named `<platform>.<area>.<screen>[.<state>]` (design-to-
# implementation, "One name for each screen and each part"), and the preview
# of a board carries its name: `mac.welcome.libraries` on macOS,
# `android.welcome.libraries` on Android. The platform part is the board's
# platform (`mac`, `android`); the capture's own platform follows the `@`.
BOARD_PLATFORMS = {"mac": "macos", "android": "android"}
# What follows the platform part names the screen; one row of the gallery
# holds the previews of that screen, a column for each platform.
SHOT_RE = re.compile(
    r"^(?P<board>(?P<board_platform>[a-z]+)\.(?P<screen>[a-z0-9.-]+?))"
    r"(?P<look>-(?:dark|light)-[a-z]+(?:-[a-z]+)?)?"
    r"@(?P<platform>[a-z]+)\.png$"
)


def collect(shots_dir: pathlib.Path) -> dict[str, dict[str, pathlib.Path]]:
    """The shots by screen, then by platform. A capture in another look than
    the stock one (`-light-slate`, ...) is left out: the gallery shows each
    board's preview once."""
    scenes: dict[str, dict[str, pathlib.Path]] = {}
    pngs = sorted(shots_dir.rglob("*.png"))
    if not pngs:
        sys.exit(f"no PNGs found under {shots_dir}")
    other_looks = 0
    for png in pngs:
        m = SHOT_RE.match(png.name)
        if not m:
            sys.exit(f"unrecognized screenshot name: {png.name}")
        platform = m["platform"]
        if platform not in PLATFORM_ORDER:
            sys.exit(f"unknown platform '{platform}' in {png.name}")
        if BOARD_PLATFORMS.get(m["board_platform"]) != platform:
            sys.exit(
                f"{png.name}: the board's platform '{m['board_platform']}' "
                f"is not the capture's platform '{platform}'"
            )
        if m["look"]:
            other_looks += 1
            continue
        prior = scenes.setdefault(m["screen"], {})
        if platform in prior:
            sys.exit(f"duplicate shot for {m['board']}@{platform}")
        prior[platform] = png
    if other_looks:
        print(f"skipping {other_looks} shot(s) in a look other than the stock one")
    if not scenes:
        sys.exit("no shots found among the PNGs")
    return scenes


def build(shots_dir: pathlib.Path, site_dir: pathlib.Path, sha: str) -> None:
    scenes = collect(shots_dir)
    site_dir.mkdir(parents=True, exist_ok=True)
    shots_out = site_dir / "shots"
    shots_out.mkdir(exist_ok=True)

    rows = []
    for scene in sorted(scenes):
        cells = []
        for platform in PLATFORM_ORDER:
            png = scenes[scene].get(platform)
            if png is None:
                cells.append("<td class='missing'>—</td>")
                continue
            prefix = next(k for k, v in BOARD_PLATFORMS.items() if v == platform)
            name = f"{prefix}.{scene}@{platform}.png"
            shutil.copyfile(png, shots_out / name)
            cells.append(
                f"<td><a href='shots/{name}'>"
                f"<img src='shots/{name}' alt='{html.escape(scene)} on "
                f"{PLATFORM_LABELS[platform]}' loading='lazy'></a></td>"
            )
        head = html.escape(scene)
        rows.append(f"<tr><th scope='row'>{head}</th>{''.join(cells)}</tr>")

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
<thead><tr><th scope='col'>screen</th>{heads}</tr></thead>
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
