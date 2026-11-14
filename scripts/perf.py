#!/usr/bin/env python3
"""The performance suite: bae's timed measurements, compared with baselines.

    scripts/perf.sh list
    scripts/perf.sh run [--repeat N] [--against NAME] [scenario...]
    scripts/perf.sh accept [--as NAME] [scenario...]

A scenario is one of the repo's ignored measurements (the `SCENARIOS` table:
its cargo command and how its numbers are read). They run on a person's
machine on demand, in the release profile, on an otherwise idle machine; no
runner holds them and GitHub never does.

`run` runs each scenario's command `--repeat` times (3 by default). A number's
value is the median over those runs, its spread is (largest - smallest) /
median. Every number is a cost, so lower is better. It is compared with the
baseline in `perf/baselines/<machine>.json`, committed per machine, so
accepting a slower number takes a commit:

  better        below the baseline by more than the tolerance
  within        inside the tolerance
  slower        above the baseline by more than the tolerance (exit status 1)
  no baseline   the scenario's number has none yet (exit status 0)
  not compared  the machine's one-minute load was over half its cores when the
                scenario started (after waiting up to ten minutes for it to
                fall) or at any minute while it ran; the numbers are shown
                and recorded with when the load went over and how high, and
                `accept` refuses them

The tolerance is 25%, widened by the baseline's own spread. The last run
is kept in `target/perf/last-run.json` (build output, not the repo) with each
scenario's raw output beside it. `accept` copies the last run's medians and
spreads into the machine's baseline file for the person to commit.

`accept --as NAME` writes `perf/baselines/<machine>.<NAME>.json` instead, a
named baseline that stays put while the machine's own moves with every accept;
`run --against NAME` compares with it.

A number may carry its bar, the cost the screen it serves needs: `bar=` on a
`COST` line, `bar_ms=` on a `WINDOW_COST` line. Each reported number says
`meets bar` or `over bar (N unit)` beside its verdict, and the last run and
the baselines record the bar. A number over its bar does not change the exit
status: the bar judges the work done after the baseline, `slower` judges the
baseline.

A measurement may also print the plans of the statements it ran, one line per
plan step: `PLAN metric= n= sql= step=` (`n` numbers the metric's statements,
`sql` is the statement's first 200 characters, `step` an EXPLAIN QUERY PLAN
detail). They are recorded per metric in the last run and the baselines, and
`run` prints each step that scans a table of the seeds' big ones
(`BIG_TABLES`) or sorts with a temporary b-tree. SQLite names a scan by the
statement's alias, so only an unaliased big table is recognised.

The numbers are read from three kinds of line the measurements print:
`WINDOW_COST ... median_ms=` (bae-store's `window_cost::report`),
`COST metric= value= unit=` (its `report_cost`) and the playback queue bench's
`<label>: fastest S s, median S s (runs ...)`.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINES = ROOT / "perf" / "baselines"
LAST_RUN = ROOT / "target" / "perf" / "last-run.json"

TOLERANCE = 0.25
LOAD_SHARE = 0.5
DEFAULT_REPEAT = 3
QUIET_WAIT = 600
# Seconds between the load readings of a run.
LOAD_CHECK = 60

BETTER, WITHIN, SLOWER, NO_BASELINE, NOT_COMPARED = (
    "better",
    "within",
    "slower",
    "no baseline",
    "not compared",
)

WINDOW_COST = re.compile(r"^WINDOW_COST (list=.*)$")
COST = re.compile(r"^COST metric=(\S+) value=(\S+) unit=(\S+?)(?: bar=(\S+))?$")
PLAN = re.compile(r"^PLAN metric=(\S+) n=(\d+) sql=(.*?) step=(.*)$")
SCAN_STEP = re.compile(r"^SCAN (\w+)")
TEMP_SORT = "USE TEMP B-TREE FOR ORDER BY"
# The tables the shaped seeds make big, whose scan a plan step reports.
BIG_TABLES = frozenset(
    (
        "albums", "releases", "release_media", "tracks", "works", "track_works",
        "album_artists", "artists", "release_files", "release_storage",
        "release_artist_roles", "track_artist_roles", "release_records",
        "playlist_entries", "ratings", "download_queue", "playback_context",
        "scan_candidate", "import_list_placement", "import_list_item",
        "album_listing", "release_listing", "composer_work", "composer_release",
        "composer_listing", "composer_detail_row",
    )
)  # fmt: skip
QUEUE_BENCH = re.compile(r"^(.+): fastest [\d.]+ s, median ([\d.]+) s \(runs ")


@dataclass(frozen=True)
class Scenario:
    name: str
    # The arguments to scripts/cargo.sh.
    cargo: tuple[str, ...]
    summary: str
    # Environment variables the measurement reads, set for its cargo command.
    env: tuple[tuple[str, str], ...] = ()


def test_command(package: str, target: tuple[str, ...], filter_: tuple[str, ...]) -> tuple[str, ...]:
    return (
        "test", "--release", "-p", package, "--features", "test-utils", *target,
        "--", *filter_, "--ignored", "--nocapture", "--test-threads", "1",
    )  # fmt: skip


SCENARIOS: tuple[Scenario, ...] = (
    Scenario(
        "import-list-windows",
        test_command("bae-import", ("--lib",), ("import_list_window_cost",)),
        "reading one window of the import list, by jump and by scroll",
    ),
    Scenario(
        "import-list-search",
        test_command("bae-import", ("--lib",), ("import_list_search_window_cost",)),
        "a text-filtered import list's windows, first jumps and typing, each size in its own store",
    ),
    Scenario(
        "library-list-windows",
        test_command("bae-views", ("--lib",), ("window_cost",)),
        "reading one window of each library list, and the playback queue's",
    ),
    Scenario(
        "playback-queue-fill",
        test_command("bae-playback", ("--lib",), ("playback_queue_bench",)),
        "a library-sized play through the queue tables, against plain SQLite",
    ),
    Scenario(
        "scan",
        test_command("bae-app", ("--test", "integration"), ("test_scan_cost::",)),
        "a first scan of 1,000 and 5,000 folders, flat, in one tree and with the list open",
        env=(("SCAN_SIZES", "1000,5000"),),
    ),
    Scenario(
        "scan-10k",
        test_command("bae-app", ("--test", "integration"), ("test_scan_cost::flat_folders",)),
        "a first scan of 10,000 flat folders, without and with the import list open",
        env=(("SCAN_SIZES", "10000"),),
    ),
    Scenario(
        "import-live-query-reruns",
        test_command("bae-app", ("--test", "integration"), ("test_import_live_query_reruns::",)),
        "how often the import screen's live queries rerun during a scan",
    ),
    Scenario(
        "screen-library",
        test_command(
            "bae-app", ("--test", "integration"),
            ("test_screen_cost::library::", "test_screen_cost::detail::"),
        ),
        "the album grid, artists, composers, detail pages and search as plain queries, at S, M and L",
    ),
    Scenario(
        "screen-storage",
        test_command("bae-app", ("--test", "integration"), ("test_screen_cost::storage::",)),
        "the Storage Manager's windows, totals, search, selection and inspector as plain queries",
    ),
    Scenario(
        "screen-playback",
        test_command("bae-app", ("--test", "integration"), ("test_screen_cost::playback::",)),
        "now playing, the queue head and the upcoming window, plain and as they read today",
    ),
    Scenario(
        "screen-people",
        test_command("bae-app", ("--test", "integration"), ("test_screen_cost::playlists::",)),
        "the playlists' listing and detail as plain queries",
    ),
    Scenario(
        "screen-activity",
        test_command("bae-app", ("--test", "integration"), ("test_screen_cost::transfers::",)),
        "the transfer queues and the bytes waiting to upload as plain queries",
    ),
    Scenario(
        "library-create",
        test_command("bae-library", ("--test", "integration"), ("test_library_create_cost::",)),
        "creating a library",
    ),
    Scenario(
        "library-restore",
        test_command("bae-library", ("--test", "integration"), ("test_restore_cost::",)),
        "restoring a library on a new device",
    ),
    Scenario(
        "playback-cpu",
        test_command("bae-library", ("--test", "test_playback_cpu"), ()),
        "steady-state playback CPU",
    ),
)


def parse_numbers(output: str) -> dict[str, tuple[float, str]]:
    """Each number a measurement printed: metric -> (value, unit).

    A metric printed twice keeps its last value."""
    numbers: dict[str, tuple[float, str]] = {}
    for line in output.splitlines():
        line = line.strip()
        if match := WINDOW_COST.match(line):
            # A value may hold spaces (a Debug-printed order), so a field ends
            # where the next `key=` begins.
            fields = dict(re.findall(r"(\w+)=(.*?)(?= \w+=|$)", match.group(1)))
            metric = "/".join(
                fields[key] for key in ("list", "order", "reach", "n", "at")
            )
            numbers[metric] = (float(fields["median_ms"]), "ms")
        elif match := COST.match(line):
            numbers[match.group(1)] = (float(match.group(2)), match.group(3))
        elif match := QUEUE_BENCH.match(line):
            numbers[match.group(1)] = (float(match.group(2)) * 1000.0, "ms")
    return numbers


def parse_bars(output: str) -> dict[str, float]:
    """The bar each number printed, in the number's unit: metric -> bar."""
    bars: dict[str, float] = {}
    for line in output.splitlines():
        line = line.strip()
        if match := WINDOW_COST.match(line):
            fields = dict(re.findall(r"(\w+)=(.*?)(?= \w+=|$)", match.group(1)))
            if "bar_ms" in fields:
                metric = "/".join(
                    fields[key] for key in ("list", "order", "reach", "n", "at")
                )
                bars[metric] = float(fields["bar_ms"])
        elif (match := COST.match(line)) and match.group(4) is not None:
            bars[match.group(1)] = float(match.group(4))
    return bars


def parse_plans(output: str) -> dict[str, list[list]]:
    """The plans printed, per metric: [[statement, [step, ...]], ...] in the
    order of the statements' numbers."""
    statements: dict[str, dict[int, list]] = {}
    for line in output.splitlines():
        if match := PLAN.match(line.strip()):
            metric, n, sql, step = match.groups()
            entry = statements.setdefault(metric, {}).setdefault(int(n), [sql, []])
            entry[1].append(step)
    return {
        metric: [entry for _, entry in sorted(by_number.items())]
        for metric, by_number in statements.items()
    }


def costly_steps(plans: dict[str, list[list]]) -> dict[str, list[str]]:
    """Each metric's plan steps that scan a big table or sort in a
    temporary b-tree."""
    costly: dict[str, list[str]] = {}
    for metric, statements in plans.items():
        for _, steps in statements:
            for step in steps:
                scan = SCAN_STEP.match(step)
                if step == TEMP_SORT or (scan and scan.group(1) in BIG_TABLES):
                    costly.setdefault(metric, []).append(step)
    return costly


def bar_verdict(value: float, bar: float | None, unit: str) -> str:
    """Whether a number meets the bar it printed with, if it printed one."""
    if bar is None:
        return ""
    return "meets bar" if value <= bar else f"over bar ({bar:g} {unit})"


def summarize(
    runs: list[dict[str, tuple[float, str]]], bars: dict[str, float] | None = None
) -> dict[str, dict]:
    """Median and spread of each metric over the repeated runs of a scenario,
    with its bar when it printed one."""
    summary: dict[str, dict] = {}
    for metric in sorted(set().union(*runs)):
        values = [run[metric][0] for run in runs if metric in run]
        unit = next(run[metric][1] for run in runs if metric in run)
        median = statistics.median(values)
        spread = (max(values) - min(values)) / median if median else 0.0
        summary[metric] = {"median": median, "spread": spread, "unit": unit}
        if bars and metric in bars:
            summary[metric]["bar"] = bars[metric]
    return summary


def compare(
    value: float, baseline: dict | None, load: float, cores: int
) -> str:
    """How a number stands against its baseline entry (median, spread)."""
    if load > cores * LOAD_SHARE:
        return NOT_COMPARED
    if baseline is None:
        return NO_BASELINE
    tolerance = TOLERANCE + baseline["spread"]
    if value > baseline["median"] * (1 + tolerance):
        return SLOWER
    if value < baseline["median"] * (1 - tolerance):
        return BETTER
    return WITHIN


def machine_name() -> str:
    """A stable name for this kind of machine: its model and core count."""
    model = ""
    if sys.platform == "darwin":
        model = subprocess.run(
            ["sysctl", "-n", "hw.model"], capture_output=True, text=True, check=False
        ).stdout.strip()
    model = model or f"{platform.system()}-{platform.machine()}"
    return re.sub(r"[^A-Za-z0-9._]+", "-", f"{model}-{os.cpu_count()}cores")


def baseline_path(machine: str, name: str | None) -> Path:
    """The machine's moving baseline, or its baseline named `name`."""
    return BASELINES / (f"{machine}.{name}.json" if name else f"{machine}.json")


def load_json(path: Path) -> dict:
    return json.loads(path.read_text()) if path.exists() else {}


def chosen(names: list[str]) -> list[Scenario]:
    known = {scenario.name: scenario for scenario in SCENARIOS}
    unknown = [name for name in names if name not in known]
    if unknown:
        sys.exit(f"unknown scenario {', '.join(unknown)}; `scripts/perf.sh list` names them")
    return [known[name] for name in names] if names else list(SCENARIOS)


@dataclass(frozen=True)
class Run:
    status: int
    output: str
    # The largest one-minute load read: at the start and after each wait
    # during the run, and how many seconds into the run it was read.
    peak_load: float
    peak_at: float


def run_once(
    command: list[str],
    env: dict[str, str],
    read_load: Callable[[], float] = lambda: os.getloadavg()[0],
    wait: float = LOAD_CHECK,
) -> Run:
    """Run a command, reading the load at its start and after each `wait`
    seconds until it exits, so a load that rises mid-run is seen."""
    started = time.monotonic()
    peak, peak_at = read_load(), 0.0
    process = subprocess.Popen(
        command,
        env=env,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
    )
    while True:
        try:
            output, _ = process.communicate(timeout=wait)
            break
        except subprocess.TimeoutExpired:
            load = read_load()
            if load > peak:
                peak, peak_at = load, time.monotonic() - started
    return Run(process.returncode, output, peak, peak_at)


def run_scenario(scenario: Scenario) -> Run:
    return run_once(
        [str(ROOT / "scripts" / "cargo.sh"), *scenario.cargo],
        {**os.environ, **dict(scenario.env)},
    )


def command_list(_: argparse.Namespace) -> int:
    for scenario in SCENARIOS:
        print(f"{scenario.name:26} {scenario.summary}")
        print(f"{'':26} scripts/cargo.sh {' '.join(scenario.cargo)}")
    return 0


def wait_for_quiet(cores: int) -> None:
    """Wait, up to ten minutes, for the one-minute load to fall to its share."""
    waited = 0
    while os.getloadavg()[0] > cores * LOAD_SHARE and waited < QUIET_WAIT:
        if waited == 0:
            print(f"  waiting for load {os.getloadavg()[0]:.2f} to fall under {cores * LOAD_SHARE:.0f}")
        time.sleep(10)
        waited += 10


def command_run(args: argparse.Namespace) -> int:
    machine = machine_name()
    path = baseline_path(machine, args.against)
    if args.against and not path.exists():
        sys.exit(f"no baseline named {args.against}: {path.relative_to(ROOT)} does not exist")
    baselines = load_json(path).get("scenarios", {})
    cores = os.cpu_count() or 1
    last = {"machine": machine, "scenarios": {}}
    slower = 0
    print(f"machine {machine}")
    for scenario in chosen(args.scenarios):
        wait_for_quiet(cores)
        load = os.getloadavg()[0]
        print(f"\n{scenario.name} (load {load:.2f} of {cores} cores)")
        runs, logs, bars, plans = [], [], {}, {}
        peak = None  # the highest load over the share: (load, seconds into the run, repeat)
        for repeat in range(1, args.repeat + 1):
            run = run_scenario(scenario)
            logs.append(run.output)
            if run.status != 0:
                sys.stderr.write(run.output[-4000:])
                sys.exit(f"{scenario.name}: cargo exited {run.status}")
            runs.append(parse_numbers(run.output))
            bars.update(parse_bars(run.output))
            plans.update(parse_plans(run.output))
            load = max(load, run.peak_load)
            if run.peak_load > cores * LOAD_SHARE and (peak is None or run.peak_load > peak[0]):
                peak = (run.peak_load, run.peak_at, repeat)
        if peak:
            print(
                f"  load went over {cores * LOAD_SHARE:g}: {peak[0]:.2f} at "
                f"{peak[1] / 60:.0f} min into run {peak[2]} of {args.repeat}; not compared"
            )
        LAST_RUN.parent.mkdir(parents=True, exist_ok=True)
        (LAST_RUN.parent / f"{scenario.name}.log").write_text("\n".join(logs))
        if not any(runs):
            sys.exit(f"{scenario.name}: the run printed no numbers")
        numbers = summarize(runs, bars)
        recorded = {}
        for metric, number in numbers.items():
            baseline = baselines.get(scenario.name, {}).get(metric)
            verdict = compare(number["median"], baseline, load, cores)
            slower += verdict == SLOWER
            against = (
                f" (baseline {baseline['median']:.4g})" if baseline else ""
            )
            bar = bar_verdict(number["median"], number.get("bar"), number["unit"])
            print(
                f"  {verdict:12} {metric}: {number['median']:.4g} {number['unit']}{against}"
                + (f"  {bar}" if bar else "")
            )
            recorded[metric] = number
        for metric, steps in costly_steps(plans).items():
            for step in steps:
                print(f"  plan         {metric}: {step}")
        last["scenarios"][scenario.name] = {
            "load": load,
            "load_over_at": None if peak is None else {"load": peak[0], "seconds": peak[1], "run": peak[2]},
            "cores": cores,
            "compared": load <= cores * LOAD_SHARE,
            "numbers": recorded,
            "plans": plans,
        }
        LAST_RUN.write_text(json.dumps(last, indent=2) + "\n")
    return 1 if slower else 0


def command_accept(args: argparse.Namespace) -> int:
    last = load_json(LAST_RUN)
    if not last:
        sys.exit("no last run; `scripts/perf.sh run` first")
    if last["machine"] != machine_name():
        sys.exit(f"the last run was on {last['machine']}, not this machine")
    names = [scenario.name for scenario in chosen(args.scenarios)] if args.scenarios else list(
        last["scenarios"]
    )
    path = baseline_path(last["machine"], args.name)
    baseline = load_json(path) or {"machine": last["machine"], "scenarios": {}}
    baseline.setdefault("plans", {})
    for name in names:
        ran = last["scenarios"].get(name)
        if ran is None:
            sys.exit(f"{name}: not in the last run")
        if not ran["compared"]:
            sys.exit(
                f"{name}: the last run was recorded as not compared "
                f"(load {ran['load']:.2f} of {ran['cores']} cores); run it again on a quiet machine"
            )
    for name in names:
        baseline["scenarios"][name] = last["scenarios"][name]["numbers"]
        baseline["plans"][name] = last["scenarios"][name].get("plans", {})
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
    print(f"wrote {', '.join(names)} to {path.relative_to(ROOT)}; commit it")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("list", help="the scenarios").set_defaults(run=command_list)
    run = commands.add_parser("run", help="run scenarios and compare with the baseline")
    run.add_argument("--repeat", type=int, default=DEFAULT_REPEAT, help="runs per scenario")
    run.add_argument(
        "--against", metavar="NAME", help="compare with the baseline accepted --as NAME"
    )
    run.add_argument("scenarios", nargs="*")
    run.set_defaults(run=command_run)
    accept = commands.add_parser("accept", help="write the last run's numbers as the baseline")
    accept.add_argument(
        "--as", dest="name", metavar="NAME", help="write a baseline named NAME, not the machine's own"
    )
    accept.add_argument("scenarios", nargs="*")
    accept.set_defaults(run=command_accept)
    args = parser.parse_args()
    return args.run(args)


if __name__ == "__main__":
    sys.exit(main())
