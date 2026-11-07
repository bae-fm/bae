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


class SummaryTests(unittest.TestCase):
    def test_median_and_spread_over_runs(self) -> None:
        runs = [{"a": (90.0, "ms")}, {"a": (100.0, "ms")}, {"a": (120.0, "ms")}]
        summary = PERF.summarize(runs)["a"]
        self.assertEqual(summary["median"], 100.0)
        self.assertAlmostEqual(summary["spread"], 0.3)
        self.assertEqual(summary["unit"], "ms")


if __name__ == "__main__":
    unittest.main()
