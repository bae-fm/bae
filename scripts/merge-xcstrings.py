#!/usr/bin/env python3
"""Git merge driver for Xcode string catalogs (*.xcstrings): merges by key.

Usage (as git calls it): scripts/merge-xcstrings.py <base> <ours> <theirs>
Writes the merge into <ours>. Exits 1, leaving <ours> untouched, when both sides
changed one key differently or one side changed a key the other removed.

Each side is compared with the base per key of "strings"; a key one side added,
changed or removed is taken as that side left it. New keys go right after the
key that precedes them on the side that added them (Xcode keeps keys sorted, so
this is where it would put them), else at the start. Other top-level fields
(sourceLanguage, version) merge the same way as single values. Output uses
Xcode's own formatting, so a catalog Xcode wrote round-trips byte for byte.
"""

import json
import sys

MISSING = object()


def load(path):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    return json.loads(text), text


def dump(doc, trailing_newline):
    out = json.dumps(doc, indent=2, ensure_ascii=False, separators=(",", " : "))
    return out + ("\n" if trailing_newline else "")


def merge_value(key, base, ours, theirs, conflicts):
    """Three-way merge of one value; records a conflict and keeps ours."""
    if ours == theirs or theirs == base:
        return ours
    if ours == base:
        return theirs
    conflicts.append(key)
    return ours


def merge_keys(base, ours, theirs, conflicts):
    """Merge two dicts against a base, keeping ours' key order."""
    merged = {}
    for key, value in ours.items():
        b = base.get(key, MISSING)
        t = theirs.get(key, MISSING)
        result = merge_value(key, b, value, t, conflicts)
        if result is MISSING:
            continue
        merged[key] = result
    # Keys only theirs has: theirs added them, or ours removed them.
    theirs_keys = list(theirs)
    for index, key in enumerate(theirs_keys):
        if key in ours:
            continue
        b = base.get(key, MISSING)
        if b is MISSING:
            merged = insert_after(
                merged, theirs_keys[index - 1] if index else None, key, theirs[key], theirs
            )
        elif theirs[key] != b:
            conflicts.append(key)  # ours removed it, theirs changed it
        # else: ours removed it and theirs left it alone
    return merged


def insert_after(merged, after, key, value, theirs):
    # Land after `after`, then past keys only ours has (added on its side).
    items = list(merged.items())
    position = 0
    if after is not None:
        for i, (k, _) in enumerate(items):
            if k == after:
                position = i + 1
                break
        while position < len(items) and items[position][0] not in theirs:
            position += 1
    items.insert(position, (key, value))
    return dict(items)


def merge_docs(base, ours, theirs, conflicts):
    # Top-level fields other than "strings" merge as single values.
    merged = {}
    for key, value in ours.items():
        if key == "strings":
            continue
        result = merge_value(key, base.get(key, MISSING), value, theirs.get(key, MISSING), conflicts)
        if result is not MISSING:
            merged[key] = result
    for key, value in theirs.items():
        if key != "strings" and key not in ours and base.get(key, MISSING) is MISSING:
            merged[key] = value
    merged["strings"] = merge_keys(
        base.get("strings", {}), ours.get("strings", {}), theirs.get("strings", {}), conflicts
    )
    # Keep "strings" where ours had it among the top-level fields.
    order = list(ours)
    return {k: merged[k] for k in order if k in merged} | {
        k: v for k, v in merged.items() if k not in order
    }


def main(argv):
    if len(argv) != 4:
        print("usage: merge-xcstrings.py <base> <ours> <theirs>", file=sys.stderr)
        return 2
    (base, _), (ours, ours_text), (theirs, _) = (load(p) for p in argv[1:4])
    conflicts = []
    merged = merge_docs(base, ours, theirs, conflicts)
    if conflicts:
        print(
            "merge-xcstrings: both sides changed: " + ", ".join(repr(k) for k in conflicts),
            file=sys.stderr,
        )
        return 1
    with open(argv[2], "w", encoding="utf-8") as f:
        f.write(dump(merged, ours_text.endswith("\n")))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
