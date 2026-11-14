#!/usr/bin/env python3
"""Exercise the app row-work pass: each operation in Swift and Kotlin, what it
leaves alone, the exact counts of the two lists, and the real TOML."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import ui_row_work as WORK  # noqa: E402

SWIFT = "bae-macos/bae/bae/Views/Fixture.swift"
KOTLIN = "bae-android/app/src/main/java/fm/bae/app/ui/Fixture.kt"


def operations(path: str, text: str) -> list[str]:
    return [found.operation for found in WORK.findings({path: text})]


class OperationTests(unittest.TestCase):
    def test_swift_operations(self) -> None:
        cases = {
            "sorted": "let a = rows.sorted { $0.n < $1.n }",
            "sort": "rows.sort()",
            "filter": "let a = rows.filter { $0.ok }",
            "Dictionary(grouping:": "let a = Dictionary(grouping: rows, by: \\.k)",
            "Dictionary(uniqueKeysWithValues:": "let a = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })",
            "reduce": "let a = rows.reduce(0) { $0 + $1.n }",
            "count(where:": "let a = rows.count(where: { $0.ok })",
            "max": "let a = rows.max()",
            "min": "let a = rows.min(by: { $0 < $1 })",
            "uniqued": "let a = rows.uniqued()",
        }
        for operation, source in cases.items():
            with self.subTest(operation):
                found = operations(SWIFT, source)
                self.assertIn(operation, found)

    def test_kotlin_operations(self) -> None:
        cases = {
            "sorted": "val a = rows.sorted()",
            "sortedBy": "val a = rows.sortedBy { it.n }",
            "sortedByDescending": "val a = rows.sortedByDescending { it.n }",
            "sortedWith": "val a = rows.sortedWith(cmp)",
            "sortedDescending": "val a = rows.sortedDescending()",
            "sort": "rows.sort()",
            "sortBy": "rows.sortBy { it.n }",
            "sortWith": "rows.sortWith(cmp)",
            "filter": "val a = rows.filter { it.ok }",
            "filterNot": "val a = rows.filterNot { it.ok }",
            "filterIsInstance": "val a = rows.filterIsInstance<Row>()",
            "partition": "val a = rows.partition { it.ok }",
            "groupBy": "val a = rows.groupBy { it.k }",
            "groupingBy": "val a = rows.groupingBy { it.k }",
            "associateBy": "val a = rows.associateBy { it.id }",
            "associate": "val a = rows.associate { it.id to it }",
            "associateWith": "val a = rows.associateWith { it.n }",
            "sumOf": "val a = rows.sumOf { it.n }",
            "sum": "val a = counts.sum()",
            "count {": "val a = rows.count { it.ok }",
            "maxOf": "val a = rows.maxOf { it.n }",
            "minOf": "val a = rows.minOf { it.n }",
            "maxBy": "val a = rows.maxBy { it.n }",
            "minBy": "val a = rows.minBy { it.n }",
            "fold": "val a = rows.fold(0) { s, r -> s + r.n }",
            "reduce": "val a = rows.reduce { s, r -> s }",
            "distinct": "val a = rows.distinct()",
            "distinctBy": "val a = rows.distinctBy { it.id }",
        }
        for operation, source in cases.items():
            with self.subTest(operation):
                self.assertIn(operation, operations(KOTLIN, source))

    def test_safe_call_chains_are_found(self) -> None:
        self.assertEqual(operations(KOTLIN, "val a = rows?.filter { it.ok }"), ["filter"])
        self.assertEqual(operations(SWIFT, "let a = rows?.filter { $0.ok }"), ["filter"])

    def test_swift_names_do_not_cross_into_kotlin_and_back(self) -> None:
        self.assertEqual(operations(KOTLIN, "val a = rows.uniqued()"), [])
        self.assertEqual(operations(SWIFT, "let a = rows.sumOf { $0 }"), [])
        self.assertEqual(operations(SWIFT, "let a = rows.count { $0.ok }"), [])
        self.assertEqual(operations(KOTLIN, "val a = rows.count(where: x)"), [])

    def test_names_in_comments_and_strings_are_not_found(self) -> None:
        self.assertEqual(
            operations(SWIFT, '// rows.filter { }\nlet a = "rows.sorted()"\n/* rows.reduce(0) */'),
            [],
        )
        self.assertEqual(
            operations(KOTLIN, '// rows.filter { }\nval a = "rows.sumOf { }"\n/* rows.groupBy { } */'),
            [],
        )

    def test_global_functions_and_constants_are_not_row_work(self) -> None:
        self.assertEqual(operations(SWIFT, "let a = max(w, 10)\nlet b = Int.max\nlet c = min(a, b)"), [])
        self.assertEqual(operations(KOTLIN, "val a = maxOf(w, 10)\nval b = Int.MAX_VALUE\nval c = minOf(a, b)"), [])

    def test_swift_count_for_layout_is_not_shown_count(self) -> None:
        self.assertEqual(operations(SWIFT, "let n = rows.count\nif rows.count > 3 { }"), [])
        self.assertEqual(operations(KOTLIN, "val n = rows.size\nval m = rows.count()"), [])

    def test_interpolated_count_is_shown_count(self) -> None:
        self.assertEqual(
            operations(SWIFT, 'let a = Text("\\(members.count) people")'), ["shown-count"]
        )
        self.assertEqual(
            operations(KOTLIN, 'val a = "${members.size} people"'), ["shown-count"]
        )
        self.assertEqual(
            operations(KOTLIN, 'val a = "$name has ${rows.count()} rows"'), ["shown-count"]
        )

    def test_second_interpolation_of_one_string_is_read(self) -> None:
        self.assertEqual(
            operations(SWIFT, 'let a = "\\(name) has \\(rows.count) rows"'), ["shown-count"]
        )

    def test_interpolated_value_that_is_not_a_count_is_not_found(self) -> None:
        self.assertEqual(operations(SWIFT, 'let a = "\\(name) \\(total)"'), [])
        self.assertEqual(operations(KOTLIN, 'val a = "$name ${rows.first()}"'), [])

    def test_plural_argument_is_shown_count(self) -> None:
        self.assertEqual(
            operations(
                KOTLIN,
                "val a = pluralStringResource(R.plurals.people, loaded.size, loaded.size)",
            ),
            ["shown-count", "shown-count"],
        )
        self.assertEqual(
            operations(SWIFT, 'let a = String(localized: "people \\(loaded.count)")'),
            ["shown-count"],
        )

    def test_one_count_in_two_shapes_is_one_finding(self) -> None:
        self.assertEqual(
            operations(SWIFT, 'let a = String(localized: "people \\(loaded.count)")'),
            ["shown-count"],
        )

    def test_finding_carries_path_and_line(self) -> None:
        found = WORK.findings({SWIFT: "let a = 1\nlet b = rows.filter { $0 }\n"})
        self.assertEqual([(f.path, f.line, f.operation) for f in found], [(SWIFT, 2, "filter")])


class ListTests(unittest.TestCase):
    files = {SWIFT: "let a = rows.filter { $0 }\nlet b = rows.filter { $1 }\n"}

    def lists(self, frozen=(), allowed=()) -> WORK.Lists:
        return WORK.Lists(
            tuple(WORK.Row(*row) for row in frozen),
            tuple(WORK.Row(*row) for row in allowed),
        )

    def test_unlisted_findings_are_refused_with_remedy_inputs(self) -> None:
        problems = WORK.violations(self.files, self.lists())
        self.assertEqual(len(problems), 2)
        self.assertIn(f"{SWIFT}:1:", problems[0])
        self.assertIn("(filter)", problems[0])

    def test_frozen_count_exact_passes(self) -> None:
        lists = self.lists(frozen=[(SWIFT, "filter", 2, "why")])
        self.assertEqual(WORK.violations(self.files, lists), [])

    def test_allowed_count_exact_passes(self) -> None:
        lists = self.lists(allowed=[(SWIFT, "filter", 2, "why")])
        self.assertEqual(WORK.violations(self.files, lists), [])

    def test_lower_found_than_counted_says_lower_the_count(self) -> None:
        for kind in ("frozen", "allowed"):
            with self.subTest(kind):
                lists = self.lists(**{kind: [(SWIFT, "filter", 3, "why")]})
                problems = WORK.violations(self.files, lists)
                self.assertEqual(len(problems), 1)
                self.assertIn("lower the count to 2", problems[0])

    def test_more_found_than_counted_lists_them_all(self) -> None:
        for kind in ("frozen", "allowed"):
            with self.subTest(kind):
                lists = self.lists(**{kind: [(SWIFT, "filter", 1, "why")]})
                problems = WORK.violations(self.files, lists)
                self.assertEqual(len(problems), 2)
                self.assertIn(f"{SWIFT}:1:", problems[0])
                self.assertIn(f"{SWIFT}:2:", problems[1])

    def test_row_naming_missing_file_is_stale(self) -> None:
        lists = self.lists(frozen=[("bae-macos/bae/bae/Gone.swift", "filter", 1, "why")])
        problems = WORK.violations(self.files, lists)
        self.assertTrue(any("Gone.swift" in p and "stale" in p for p in problems))

    def test_row_whose_operation_is_gone_is_stale(self) -> None:
        lists = self.lists(frozen=[(SWIFT, "sorted", 1, "why")])
        problems = WORK.violations(self.files, lists)
        self.assertTrue(any("sorted" in p and "stale" in p for p in problems))

    def test_unknown_operation_is_refused(self) -> None:
        lists = self.lists(frozen=[(SWIFT, "mapNotNull", 1, "why")])
        self.assertTrue(any("unknown operation" in p for p in WORK.violations(self.files, lists)))

    def test_count_below_one_is_refused(self) -> None:
        lists = self.lists(frozen=[(SWIFT, "filter", 0, "why")])
        self.assertTrue(any("at least 1" in p for p in WORK.violations(self.files, lists)))

    def test_key_in_both_lists_is_refused(self) -> None:
        lists = self.lists(
            frozen=[(SWIFT, "filter", 1, "why")], allowed=[(SWIFT, "filter", 1, "why")]
        )
        self.assertTrue(any("both" in p for p in WORK.violations(self.files, lists)))

    def test_duplicate_key_in_one_list_is_refused(self) -> None:
        lists = self.lists(frozen=[(SWIFT, "filter", 1, "why"), (SWIFT, "filter", 1, "again")])
        self.assertTrue(any("twice" in p for p in WORK.violations(self.files, lists)))


class ProductionTests(unittest.TestCase):
    def test_tracked_sources_pass_with_the_real_lists(self) -> None:
        self.assertEqual(WORK.check(ROOT), [])

    def test_real_lists_have_a_reason_on_every_row(self) -> None:
        lists = WORK.load_lists(ROOT / WORK.LISTS_FILE)
        for row in (*lists.frozen, *lists.allowed):
            self.assertTrue(row.note.strip(), row)

    def test_untracked_files_are_not_read(self) -> None:
        paths = WORK.tracked_sources(ROOT)
        self.assertTrue(paths)
        self.assertFalse([p for p in paths if "swift-bindings" in p or "bae_bridge.swift" in p])


if __name__ == "__main__":
    unittest.main()
