#!/usr/bin/env python3
"""Write the generator config that lists a build's cancellable Swift calls.

swift-cancellable.toml groups the names by the cfg that compiles each
callable. This writes, as a uniffi global config, the names of the groups the
build compiles: `every_build`, `desktop` when the build is for macOS, and each
`feature = "<name>"` whose feature the build turns on.

Usage: swift-cancellable.py desktop|mobile <features> <out>
  <features> is the build's comma-separated cargo feature list.
"""

import pathlib
import re
import sys
import tomllib

GROUPS = pathlib.Path(__file__).with_name("swift-cancellable.toml")
FEATURE = re.compile(r'feature = "([a-z0-9-]+)"')


def compiled(cfg, desktop, features):
    """Whether a build compiles the callables gated on `cfg`."""
    if cfg == "desktop":
        return desktop
    match = FEATURE.fullmatch(cfg)
    if match:
        return match.group(1) in features
    sys.exit(f"swift-cancellable.toml: no rule for the cfg group '{cfg}'")


def main():
    if len(sys.argv) != 4 or sys.argv[1] not in ("desktop", "mobile"):
        sys.exit(__doc__)
    desktop = sys.argv[1] == "desktop"
    features = {name for name in sys.argv[2].split(",") if name}
    out = pathlib.Path(sys.argv[3])

    groups = tomllib.loads(GROUPS.read_text())
    names = list(groups["every_build"])
    for cfg, group in groups["cfg"].items():
        if compiled(cfg, desktop, features):
            names.extend(group)
    listed = groups["every_build"] + [n for g in groups["cfg"].values() for n in g]
    twice = sorted({n for n in listed if listed.count(n) > 1})
    if twice:
        sys.exit(f"swift-cancellable.toml lists these in more than one group: {twice}")

    lines = ["[crates.bae_bridge.bindings.swift]", "async_cancellable = ["]
    lines += [f'    "{name}",' for name in sorted(names)]
    lines += ["]", ""]
    out.write_text("\n".join(lines))


if __name__ == "__main__":
    main()
