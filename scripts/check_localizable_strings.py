#!/usr/bin/env python3
"""The macOS chrome-string gate: compiled code ⇄ Localizable.xcstrings.

The Swift compiler writes each file's localizable strings to a .stringsdata
file during compilation (SWIFT_EMIT_LOC_STRINGS, set in project.yml). This
script unions that ground truth for the app target and holds it against
bae-macos/bae/bae/Localizable.xcstrings:

  1. every extracted key has a catalog entry;
  2. every catalog entry is extracted from compiled code (no orphans);
  3. every catalog entry carries a "translated"-state value for every locale
     the generated Core.xcstrings ships (the locale set single-sourced from
     the bridge catalog).

Exits non-zero listing every offender. Xcode's parser-based catalog sync is
not trustworthy for this codebase — it misses strings in some closure shapes
and then prunes their keys — so only the compile-emitted data is used.
"""

import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CATALOG = REPO / "bae-macos/bae/bae/Localizable.xcstrings"
CORE_CATALOG = REPO / "bae-macos/bae/bae/Core.xcstrings"
DERIVED_DATA = REPO / "bae-macos/bae/.build/derivedData"
# The app target and the BaeKit package target. BaeKit's Text/String(localized:)
# calls resolve against the main bundle at runtime, so its strings are app-
# catalog obligations like the app's own. Other packages (Sparkle, Sentry) keep
# their own tables and stay out of the union.
STRINGSDATA_TARGETS = ("bae", "BaeKit")


def extracted_keys(derived_data: Path) -> set[str]:
    root = derived_data / "Build/Intermediates.noindex"
    files: list[Path] = []
    for target in STRINGSDATA_TARGETS:
        files += sorted(
            (root / f"{target}.build").glob(
                f"*/{target}.build/Objects-normal/*/*.stringsdata"
            )
        )
    if not files:
        sys.exit(
            f"no .stringsdata under {root} — build the bae scheme with "
            "SWIFT_EMIT_LOC_STRINGS=YES first"
        )
    # Old build-config and architecture directories keep stale .stringsdata
    # forever; per source file, only the newest emission tells the truth, and
    # a source file that no longer exists tells none.
    newest: dict[str, tuple[float, dict]] = {}
    for f in files:
        data = json.loads(f.read_text())
        source = data.get("source", "")
        if not Path(source).exists():
            continue
        mtime = f.stat().st_mtime
        if source not in newest or mtime > newest[source][0]:
            newest[source] = (mtime, data)
    keys: set[str] = set()
    for _, data in newest.values():
        for entry in data.get("tables", {}).get("Localizable", []):
            keys.add(entry["key"])
    return keys


def locale_set() -> set[str]:
    core = json.loads(CORE_CATALOG.read_text())
    locales: set[str] = set()
    for entry in core["strings"].values():
        locales |= set(entry.get("localizations", {}))
    if not locales:
        sys.exit(f"{CORE_CATALOG} lists no locales — run loc-gen first")
    return locales


def translated(localization: dict) -> bool:
    """A localization carries either a plain stringUnit or per-plural
    variations; it counts as translated when every unit it has is."""
    if "stringUnit" in localization:
        unit = localization["stringUnit"]
        return unit.get("state") == "translated" and bool(unit.get("value"))
    variations = localization.get("variations", {}).get("plural", {})
    if not variations:
        return False
    return all(
        v.get("stringUnit", {}).get("state") == "translated"
        and bool(v.get("stringUnit", {}).get("value"))
        for v in variations.values()
    )


def untranslated(entry: dict, locales: set[str]) -> list[str]:
    gaps = []
    localizations = entry.get("localizations", {})
    for locale in sorted(locales):
        if locale == "en" and locale not in localizations:
            # The key itself is the English source; an explicit en slot is
            # optional.
            continue
        if not translated(localizations.get(locale, {})):
            gaps.append(locale)
    return gaps


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--derived-data",
        type=Path,
        default=DERIVED_DATA,
        help="derived-data path of a completed bae-scheme build",
    )
    args = parser.parse_args()

    extracted = extracted_keys(args.derived_data)
    catalog = json.loads(CATALOG.read_text())["strings"]
    locales = locale_set()

    missing = sorted(extracted - catalog.keys())
    orphans = sorted(catalog.keys() - extracted)
    gaps = {
        key: locs
        for key in sorted(catalog)
        if (locs := untranslated(catalog[key], locales))
    }

    ok = True
    if missing:
        ok = False
        print(f"❌ {len(missing)} extracted string(s) missing from the catalog")
        print("   (translate them, or Text(verbatim:) if not prose):")
        for key in missing:
            print(f"   + {key!r}")
    if orphans:
        ok = False
        print(f"❌ {len(orphans)} catalog key(s) no compiled code references:")
        for key in orphans:
            print(f"   - {key!r}")
    if gaps:
        ok = False
        print(f"❌ {len(gaps)} catalog key(s) missing translations:")
        for key, locs in gaps.items():
            print(f"   ~ {key!r}: {', '.join(locs)}")
    if not ok:
        return 1
    print(
        f"✓ {len(extracted)} strings, {len(locales)} locales — "
        "catalog and code agree"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
