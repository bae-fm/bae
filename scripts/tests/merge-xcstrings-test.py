#!/usr/bin/env python3
"""Tests for scripts/merge-xcstrings.py, the git merge driver for string catalogs."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

DRIVER = Path(__file__).resolve().parent.parent / "merge-xcstrings.py"


def entry(text):
    return {"localizations": {"en": {"stringUnit": {"state": "translated", "value": text}}}}


def catalog(**strings):
    return {"sourceLanguage": "en", "strings": {k: entry(v) for k, v in strings.items()}, "version": "1.0"}


def dump(doc):
    return json.dumps(doc, indent=2, ensure_ascii=False, separators=(",", " : ")) + "\n"


def merge(base, ours, theirs):
    """Run the driver; returns (exit code, merged catalog or None, ours file text)."""
    with tempfile.TemporaryDirectory() as tmp:
        paths = []
        for name, doc in (("base", base), ("ours", ours), ("theirs", theirs)):
            path = Path(tmp) / name
            path.write_text(dump(doc), encoding="utf-8")
            paths.append(str(path))
        run = subprocess.run([sys.executable, str(DRIVER), *paths], capture_output=True, text=True)
        text = Path(paths[1]).read_text(encoding="utf-8")
        return run.returncode, json.loads(text), text


class MergeTest(unittest.TestCase):
    def test_keys_added_on_both_sides_are_both_kept_in_order(self):
        base = catalog(A="a", D="d")
        code, merged, text = merge(base, catalog(A="a", B="b", D="d"), catalog(A="a", C="c", D="d"))
        self.assertEqual(code, 0)
        self.assertEqual(list(merged["strings"]), ["A", "B", "C", "D"])
        self.assertEqual(text, dump(merged))

    def test_a_key_added_before_the_first_goes_first(self):
        base = catalog(B="b")
        _, merged, _ = merge(base, catalog(B="b", C="c"), catalog(A="a", B="b"))
        self.assertEqual(list(merged["strings"]), ["A", "B", "C"])

    def test_change_on_one_side_and_removal_on_the_other(self):
        base = catalog(A="a", B="b", C="c")
        code, merged, _ = merge(base, catalog(A="a2", B="b", C="c"), catalog(A="a", C="c"))
        self.assertEqual(code, 0)
        self.assertEqual(merged["strings"], catalog(A="a2", C="c")["strings"])

    def test_the_same_change_on_both_sides_is_not_a_conflict(self):
        base = catalog(A="a")
        code, merged, _ = merge(base, catalog(A="x"), catalog(A="x"))
        self.assertEqual(code, 0)
        self.assertEqual(merged["strings"], catalog(A="x")["strings"])

    def test_one_key_changed_differently_is_a_conflict(self):
        base = catalog(A="a")
        code, _, text = merge(base, catalog(A="x"), catalog(A="y"))
        self.assertEqual(code, 1)
        self.assertEqual(text, dump(catalog(A="x")))

    def test_a_key_changed_on_one_side_and_removed_on_the_other_is_a_conflict(self):
        base = catalog(A="a", B="b")
        self.assertEqual(merge(base, catalog(B="b"), catalog(A="x", B="b"))[0], 1)
        self.assertEqual(merge(base, catalog(A="x", B="b"), catalog(B="b"))[0], 1)

    def test_a_key_added_differently_on_both_sides_is_a_conflict(self):
        base = catalog(A="a")
        self.assertEqual(merge(base, catalog(A="a", B="x"), catalog(A="a", B="y"))[0], 1)


if __name__ == "__main__":
    unittest.main()
