#!/usr/bin/env python3
"""Exercise the performance suite's comparison and its readings of the numbers
the measurements print, with fixture numbers."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts" / "perf.py"
SPEC = importlib.util.spec_from_file_location("perf", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
PERF = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PERF
SPEC.loader.exec_module(PERF)

QUIET = 1.0
CORES = 8


def compare(value: float, median: float | None, spread: float = 0.0, load: float = QUIET) -> str:
    baseline = None if median is None else {"median": median, "spread": spread}
    return PERF.compare(value, baseline, load, CORES)


class CompareTests(unittest.TestCase):
    def test_inside_the_tolerance_is_within(self) -> None:
        self.assertEqual(compare(124.0, 100.0), "within")
        self.assertEqual(compare(76.0, 100.0), "within")

    def test_past_the_tolerance_is_slower_or_better(self) -> None:
        self.assertEqual(compare(126.0, 100.0), "slower")
        self.assertEqual(compare(74.0, 100.0), "better")

    def test_the_baselines_spread_widens_the_tolerance(self) -> None:
        self.assertEqual(compare(140.0, 100.0, spread=0.0), "slower")
        self.assertEqual(compare(140.0, 100.0, spread=0.2), "within")
        self.assertEqual(compare(150.0, 100.0, spread=0.2), "slower")
        self.assertEqual(compare(50.0, 100.0, spread=0.2), "better")

    def test_load_over_half_the_cores_is_not_compared(self) -> None:
        self.assertEqual(compare(500.0, 100.0, load=4.01), "not compared")
        self.assertEqual(compare(500.0, 100.0, load=4.0), "slower")
        self.assertEqual(compare(100.0, None, load=5.0), "not compared")

    def test_no_baseline(self) -> None:
        self.assertEqual(compare(100.0, None), "no baseline")

    def test_a_baseline_of_zero_allows_nothing(self) -> None:
        self.assertEqual(compare(0.0, 0.0), "within")
        self.assertEqual(compare(1.0, 0.0), "slower")


class ParseTests(unittest.TestCase):
    def test_a_window_line(self) -> None:
        line = (
            "WINDOW_COST list=import order=added reach=jump n=20000 at=middle "
            "offset=10000 rows=50 median_ms=1.250"
        )
        self.assertEqual(
            PERF.parse_numbers(line), {"import/added/jump/20000/middle": (1.25, "ms")}
        )

    def test_a_window_line_whose_order_holds_spaces(self) -> None:
        line = (
            "WINDOW_COST list=import_list order=Added { newest: true } reach=scroll "
            "n=50000 at=end offset=49950 rows=50 median_ms=0.500"
        )
        self.assertEqual(
            PERF.parse_numbers(line),
            {"import_list/Added { newest: true }/scroll/50000/end": (0.5, "ms")},
        )

    def test_a_cost_line(self) -> None:
        line = "COST metric=create_library value=84.5000 unit=ms"
        self.assertEqual(PERF.parse_numbers(line), {"create_library": (84.5, "ms")})

    def test_a_queue_bench_line_in_milliseconds(self) -> None:
        line = (
            "bae, no live queries: play into an empty lane: fastest 1.200 s, "
            "median 1.500 s (runs [1.200, 1.500, 1.900])"
        )
        self.assertEqual(
            PERF.parse_numbers(line),
            {"bae, no live queries: play into an empty lane": (1500.0, "ms")},
        )

    def test_other_lines_are_ignored(self) -> None:
        self.assertEqual(PERF.parse_numbers("created a library in 80ms\ntest ok"), {})


class BarTests(unittest.TestCase):
    def test_a_cost_line_may_carry_its_bar(self) -> None:
        line = "COST metric=album_page value=12.5 unit=ms bar=50"
        self.assertEqual(PERF.parse_numbers(line), {"album_page": (12.5, "ms")})
        self.assertEqual(PERF.parse_bars(line), {"album_page": 50.0})

    def test_a_window_line_may_carry_its_bar(self) -> None:
        line = (
            "WINDOW_COST list=albums order=title reach=jump n=20000 at=end "
            "offset=19950 rows=50 median_ms=3.000 bar_ms=16"
        )
        self.assertEqual(PERF.parse_bars(line), {"albums/title/jump/20000/end": 16.0})

    def test_a_line_without_a_bar_has_none(self) -> None:
        self.assertEqual(PERF.parse_bars("COST metric=a value=1 unit=ms"), {})

    def test_the_verdict_beside_the_bar(self) -> None:
        self.assertEqual(PERF.bar_verdict(8.0, 8.0, "ms"), "meets bar")
        self.assertEqual(PERF.bar_verdict(8.1, 8.0, "ms"), "over bar (8 ms)")
        self.assertEqual(PERF.bar_verdict(8.1, None, "ms"), "")

    def test_the_summary_records_the_bar(self) -> None:
        runs = [{"a": (1.0, "ms"), "b": (2.0, "ms")}]
        summary = PERF.summarize(runs, {"a": 8.0})
        self.assertEqual(summary["a"]["bar"], 8.0)
        self.assertNotIn("bar", summary["b"])


class PlanTests(unittest.TestCase):
    LINES = "\n".join(
        (
            "PLAN metric=grid n=0 sql=SELECT a.id FROM albums a step=SCAN albums",
            "PLAN metric=grid n=0 sql=SELECT a.id FROM albums a step=USE TEMP B-TREE FOR ORDER BY",
            "PLAN metric=grid n=1 sql=SELECT COUNT(*) FROM albums step=SEARCH albums USING INTEGER PRIMARY KEY (rowid=?)",
            "PLAN metric=page n=0 sql=SELECT 1 step=SCAN CONSTANT ROW",
            "created a library in 80ms",
        )
    )

    def test_plans_are_collected_per_metric_in_statement_order(self) -> None:
        self.assertEqual(
            PERF.parse_plans(self.LINES),
            {
                "grid": [
                    ["SELECT a.id FROM albums a", ["SCAN albums", "USE TEMP B-TREE FOR ORDER BY"]],
                    [
                        "SELECT COUNT(*) FROM albums",
                        ["SEARCH albums USING INTEGER PRIMARY KEY (rowid=?)"],
                    ],
                ],
                "page": [["SELECT 1", ["SCAN CONSTANT ROW"]]],
            },
        )

    def test_costly_steps_are_big_table_scans_and_temporary_sorts(self) -> None:
        self.assertEqual(
            PERF.costly_steps(PERF.parse_plans(self.LINES)),
            {"grid": ["SCAN albums", "USE TEMP B-TREE FOR ORDER BY"]},
        )

    def test_a_scan_of_a_small_table_is_not_costly(self) -> None:
        plans = {"m": [["SELECT 1", ["SCAN settings"]]]}
        self.assertEqual(PERF.costly_steps(plans), {})


class NamedBaselineTests(unittest.TestCase):
    def test_a_named_baseline_has_its_own_file(self) -> None:
        self.assertEqual(
            PERF.baseline_path("M1", None), PERF.BASELINES / "M1.json"
        )
        self.assertEqual(
            PERF.baseline_path("M1", "before"), PERF.BASELINES / "M1.before.json"
        )


class SummaryTests(unittest.TestCase):
    def test_median_and_spread_over_runs(self) -> None:
        runs = [{"a": (90.0, "ms")}, {"a": (100.0, "ms")}, {"a": (120.0, "ms")}]
        summary = PERF.summarize(runs)["a"]
        self.assertEqual(summary["median"], 100.0)
        self.assertAlmostEqual(summary["spread"], 0.3)
        self.assertEqual(summary["unit"], "ms")


class RunOnceTests(unittest.TestCase):
    def test_the_largest_load_read_while_the_command_runs(self) -> None:
        loads = iter([1.0, 3.0, 9.5, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0])
        run = PERF.run_once(
            [sys.executable, "-c", "import time; time.sleep(0.3); print('done')"],
            dict(),
            read_load=lambda: next(loads),
            wait=0.05,
        )
        self.assertEqual(run.status, 0)
        self.assertEqual(run.output.strip(), "done")
        self.assertEqual(run.peak_load, 9.5)
        self.assertGreater(run.peak_at, 0.0)

    def test_the_load_at_the_start_counts(self) -> None:
        run = PERF.run_once(
            [sys.executable, "-c", "pass"], dict(), read_load=lambda: 7.0, wait=0.05
        )
        self.assertEqual((run.peak_load, run.peak_at), (7.0, 0.0))


if __name__ == "__main__":
    unittest.main()
