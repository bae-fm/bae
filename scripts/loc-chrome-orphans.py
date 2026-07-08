#!/usr/bin/env python3
"""Flag chrome localization keys that no source file references (dead strings).

Per platform, load the chrome catalog's keys and grep that platform's source for
a reference. A key with zero references is a candidate orphan — delete it, or
allowlist it in scripts/loc-orphans-allowlist.txt if it is referenced
dynamically (a key built at runtime that grep can't see).

Gates CI: exits non-zero if any chrome key is unreferenced and not allowlisted.
The substring match is permissive (a key that is a substring of other text reads
as "used"), so it does not false-fail on referenced strings; the allowlist is
the escape hatch for genuinely runtime-built keys grep can't see. (core.* keys
are hard-gated separately by the loc_key_coverage test in bae-bridge.)
"""
import json, re, sys, subprocess, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
ALLOWLIST = ROOT / "scripts/loc-orphans-allowlist.txt"

def allow():
    if ALLOWLIST.exists():
        return {l.strip() for l in ALLOWLIST.read_text().splitlines()
                if l.strip() and not l.startswith("#")}
    return set()

def grep_refs(src_dirs, exts):
    """All text content of source files (joined) for substring checks."""
    blobs = []
    for d in src_dirs:
        p = ROOT / d
        if not p.exists():
            continue
        for f in p.rglob("*"):
            if f.suffix in exts and f.is_file():
                try:
                    blobs.append(f.read_text(errors="ignore"))
                except OSError as e:
                    # A file we can't read might be the one referencing a key, so
                    # a silent skip could produce a false orphan (this gates CI) —
                    # make it visible rather than swallow it.
                    print(f"warning: could not read {f}: {e}", file=sys.stderr)
    return "\n".join(blobs)

def xcstrings_keys(path):
    d = json.loads((ROOT / path).read_text())
    return list(d.get("strings", {}).keys())

def android_keys(path):
    txt = (ROOT / path).read_text()
    return re.findall(r'<string name="([^"]+)"', txt) + re.findall(r'<plurals name="([^"]+)"', txt)

def resw_keys(path):
    txt = (ROOT / path).read_text()
    return re.findall(r'<data name="([^"]+)"', txt)

def swift_escaped(s):
    # Swift source writes non-ASCII as `\u{XXXX}` (e.g. `…` -> `\u{2026}`,
    # `–` -> `\u{2013}`), so the literal key won't grep-match; check that form too.
    return "".join(c if ord(c) < 128 else f"\\u{{{ord(c):04x}}}" for c in s)

def apple_ref(k, src):
    # A key with a format specifier is the format string of an interpolated
    # `Text("\(a) by \(b)")` / `String(format:)` — the literal never appears in
    # source, so grep can't verify it. Treat as used (don't false-flag).
    if re.search(r"%(\d+\$)?[@a-zA-Z]|%lld", k):
        return True
    return (k in src) or (swift_escaped(k) in src)

# XAML x:Uid keys are "<Uid>.<Property>" (Content/Text/PlaceholderText/Header/
# ToolTip) or "<Uid>.[using:...]Prop" — the source carries x:Uid="<Uid>".
XAML_PROP = re.compile(r"^(.+?)\.(\[using:|Content$|Text$|PlaceholderText$|Header$|ToolTip$)")

def windows_ref(k, src):
    m = XAML_PROP.match(k)
    if m:
        return f'x:Uid="{m.group(1)}"' in src
    return (f'"{k}"' in src) or (f'"{k.replace(".", "/")}"' in src)

PLATFORMS = [
    # (label, keys, is-referenced predicate, source dirs, source extensions)
    # Both apps compile the shared BaeKit package (Sources/BaeKit), so a key may
    # be referenced from package code (UnlockView, PauseBetweenSidesToggle, etc.)
    # rather than app-owned source — shared code resolves keys from each app's
    # bundle catalog (NSLocalizedString(..., bundle: .main)), so BaeKit's
    # references count for both catalogs.
    ("macOS", xcstrings_keys("bae-macos/bae/bae/Localizable.xcstrings"),
     apple_ref, ["bae-macos/bae/bae", "BaeKit/Sources/BaeKit"], {".swift"}),
    # The iOS chrome catalog overlaps with strings authored for the macOS app
    # (shared onboarding/restore-code chrome). The check is permissive by design
    # (extra scan roots only ever make a key read as more-referenced, never as a
    # false orphan), so the iOS scan includes the macOS tree as well as BaeKit.
    ("iOS", xcstrings_keys("bae-ios/bae/bae/Localizable.xcstrings"),
     apple_ref, ["bae-ios/bae/bae", "bae-macos/bae/bae", "BaeKit/Sources/BaeKit"],
     {".swift"}),
    # Scan all of src/main so strings referenced from the manifest
    # (android:label) and res/xml (a widget provider's android:description), not
    # just Kotlin and layouts, count as used.
    ("Android", android_keys("bae-android/app/src/main/res/values/strings.xml"),
     lambda k, src: (f"R.string.{k}" in src) or (f"@string/{k}" in src)
     or (f"R.plurals.{k}" in src) or (f"@plurals/{k}" in src),
     ["bae-android/app/src/main"], {".kt", ".xml"}),
    ("Windows", resw_keys("bae-windows/Strings/en-US/Resources.resw"),
     windows_ref, ["bae-windows"], {".cs", ".xaml"}),
]

def main():
    allowed = allow()
    total_orphans = 0
    for label, keys, ref, dirs, exts in PLATFORMS:
        src = grep_refs(dirs, exts)
        orphans = [k for k in keys if not ref(k, src) and k not in allowed]
        print(f"=== {label}: {len(orphans)} orphan(s) of {len(keys)} keys ===")
        for k in sorted(orphans):
            print(f"  {k!r}")
        total_orphans += len(orphans)
    print(f"\nTOTAL candidate orphans: {total_orphans} (allowlist: {len(allowed)})")
    if total_orphans:
        print(
            f"\nUnreferenced chrome strings — delete them, or if a key is built "
            f"at runtime add it to {ALLOWLIST.name}.",
            file=sys.stderr,
        )
    return 1 if total_orphans else 0

if __name__ == "__main__":
    sys.exit(main())
