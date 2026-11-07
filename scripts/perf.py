#!/usr/bin/env python3
"""The performance suite: bae's timed measurements, compared with baselines.

    scripts/perf.sh list
    scripts/perf.sh run [--repeat N] [scenario...]
    scripts/perf.sh accept [scenario...]

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
                scenario started, after waiting up to ten minutes for it to
                fall; the numbers are shown and recorded, and
                `accept` refuses them

The tolerance is 25%, widened by the baseline's own spread. The last run
is kept in `target/perf/last-run.json` (build output, not the repo) with each
scenario's raw output beside it. `accept` copies the last run's medians and
spreads into the machine's baseline file for the person to commit.

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
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINES = ROOT / "perf" / "baselines"
LAST_RUN = ROOT / "target" / "perf" / "last-run.json"

TOLERANCE = 0.25
LOAD_SHARE = 0.5
DEFAULT_REPEAT = 3
QUIET_WAIT = 600

BETTER, WITHIN, SLOWER, NO_BASELINE, NOT_COMPARED = (
    "better",
    "within",
    "slower",
    "no baseline",
    "not compared",
)

WINDOW_COST = re.compile(r"^WINDOW_COST (list=.*)$")
COST = re.compile(r"^COST metric=(\S+) value=(\S+) unit=(\S+)$")
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


def summarize(runs: list[dict[str, tuple[float, str]]]) -> dict[str, dict]:
    """Median and spread of each metric over the repeated runs of a scenario."""
    summary: dict[str, dict] = {}
    for metric in sorted(set().union(*runs)):
        values = [run[metric][0] for run in runs if metric in run]
        unit = next(run[metric][1] for run in runs if metric in run)
        median = statistics.median(values)
        spread = (max(values) - min(values)) / median if median else 0.0
        summary[metric] = {"median": median, "spread": spread, "unit": unit}
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


def load_json(path: Path) -> dict:
    return json.loads(path.read_text()) if path.exists() else {}


def chosen(names: list[str]) -> list[Scenario]:
    known = {scenario.name: scenario for scenario in SCENARIOS}
    unknown = [name for name in names if name not in known]
    if unknown:
        sys.exit(f"unknown scenario {', '.join(unknown)}; `scripts/perf.sh list` names them")
    return [known[name] for name in names] if names else list(SCENARIOS)


def run_once(scenario: Scenario) -> tuple[int, str]:
    process = subprocess.run(
        [str(ROOT / "scripts" / "cargo.sh"), *scenario.cargo],
        env={**os.environ, **dict(scenario.env)},
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
        check=False,
    )
    return process.returncode, process.stdout


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
    baselines = load_json(BASELINES / f"{machine}.json").get("scenarios", {})
    cores = os.cpu_count() or 1
    last = {"machine": machine, "scenarios": {}}
    slower = 0
    print(f"machine {machine}")
    for scenario in chosen(args.scenarios):
        wait_for_quiet(cores)
        load = os.getloadavg()[0]
        print(f"\n{scenario.name} (load {load:.2f} of {cores} cores)")
        runs, logs = [], []
        for _ in range(args.repeat):
            status, output = run_once(scenario)
            logs.append(output)
            if status != 0:
                sys.stderr.write(output[-4000:])
                sys.exit(f"{scenario.name}: cargo exited {status}")
            runs.append(parse_numbers(output))
        LAST_RUN.parent.mkdir(parents=True, exist_ok=True)
        (LAST_RUN.parent / f"{scenario.name}.log").write_text("\n".join(logs))
        if not any(runs):
            sys.exit(f"{scenario.name}: the run printed no numbers")
        numbers = summarize(runs)
        recorded = {}
        for metric, number in numbers.items():
            baseline = baselines.get(scenario.name, {}).get(metric)
            verdict = compare(number["median"], baseline, load, cores)
            slower += verdict == SLOWER
            against = (
                f" (baseline {baseline['median']:.4g})" if baseline else ""
            )
            print(f"  {verdict:12} {metric}: {number['median']:.4g} {number['unit']}{against}")
            recorded[metric] = number
        last["scenarios"][scenario.name] = {
            "load": load,
            "cores": cores,
            "compared": load <= cores * LOAD_SHARE,
            "numbers": recorded,
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
    path = BASELINES / f"{last['machine']}.json"
    baseline = load_json(path) or {"machine": last["machine"], "scenarios": {}}
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
    run.add_argument("scenarios", nargs="*")
    run.set_defaults(run=command_run)
    accept = commands.add_parser("accept", help="write the last run's numbers as the baseline")
    accept.add_argument("scenarios", nargs="*")
    accept.set_defaults(run=command_accept)
    args = parser.parse_args()
    return args.run(args)


if __name__ == "__main__":
    sys.exit(main())
