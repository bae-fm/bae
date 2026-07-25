#!/usr/bin/env python3
"""Enforce the desktop session/bridge boundary: NativeBae stays fenced.

The AppService port moves views and stores off `NativeBae` and the raw
`LibraryHandle` onto the narrow domain services bundled in `AppService`. The C#
compiler can't see dependency *direction* — a view that reaches back for
`NativeBae` compiles fine — so this gate makes the boundary fail loud instead of
aspirational: it greps every `NativeBae.` reference in each desktop app and fails
if one lives outside that app's sanctioned set below.

Two desktop apps are scanned: `bae-windows` (the WinUI app, shipping until the
Avalonia app reaches parity) and `bae-avalonia` (its cross-platform replacement).
Each has its own `ALLOWED` set because the two are at different points in the
same port — bae-avalonia's set grows as flows migrate onto it.

The sanctioned set is:

- The composition boundary itself — the bridge wrapper, the handle primitive, the
  session that owns the handle lifecycle, and the domain services that wrap
  NativeBae (that wrapping is the whole point).
- Process/session infrastructure that has no library-domain UI.
- The flows not yet migrated onto services. Each is fenced here, grouped under
  the future story that migrates it, so its NativeBae use can't spread further
  while it waits its turn.

Comments are stripped before matching, so a doc-comment mention of `NativeBae.X`
never trips the gate. The unit-test project is not scanned (it isn't production
dependency direction).

Second mode, `--unconsumed`: for every delegate property declared on the service
classes, list the ones with no consumer outside `Services/`. This is report-only
and never fails — caller-less delegates are the deliberate state of the
full-surface port (every BaeKit delegate is present whether or not a desktop
consumer exists yet), and the C# compiler has no signal for them (IDE0051 only
fires on unused *private* members). The default gate run prints the count so the
parity backlog is visible on every CI pass without failing anything.
"""
import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TESTS_SUFFIX = ".Tests"
SERVICES_DIR = "Services"

# Per-app sanctioned sets. A trailing "/" matches a directory prefix; otherwise
# it's an exact file (relative to the app directory).
COMPOSITION_BOUNDARY = [
    # --- The composition boundary: where the handle and the raw bridge live. ---
    "NativeBae.cs",            # the bridge wrapper itself
    "LibraryHandle.cs",        # the handle primitive (open / shutdown / free)
    "Stores/SessionStore.cs",  # owns the handle lifecycle + event subscription
    "Services/",               # the domain services wrap NativeBae — the point
    # --- Process / session infrastructure (no library-domain UI). ---
    "Program.cs",
    "BaeLogger.cs",
    "BaeCrashReporting.cs",     # edition-gated crash reporter
    "CrashCapture.cs",
    "UpdateService.cs",
    "LibraryDiscovery.cs",
    "OAuthCreds.cs",            # cloud sign-in credential registration
    "BridgeDisplay.cs",         # handle-less bridge value -> localization key
    "Views/Welcome/",           # first-run / join / restore-from-cloud flows
    "Views/Debug/",             # component gallery + shot capture
]

APPS = {
    # The WinUI app: fully ported surface, with the unmigrated dialogs fenced
    # under their future stories.
    "bae-windows": COMPOSITION_BOUNDARY
    + [
        "App.xaml.cs",
        "App.Session.cs",
        "ProtocolRegistration.cs",  # bae:// scheme registration
        # --- Dialogs not yet migrated onto services. The stores behind these ---
        # --- flows read through services now; the standalone dialogs still    ---
        # --- hold direct session/NativeBae calls and migrate with their story.---
        # future: settings dialogs (members, restore code, rename/lock, cloud
        # sign-in/disconnect, discogs/mcp/subsonic config, save-preset writes)
        "Views/Settings/",
        # future: storage dialog (per-tab paging, outbox/config reads, transfer keys)
        "Views/Storage/",
        # future: import dialogs (search/prefetch/confirm, source-identity choice)
        "Views/Import/",
        # future: album-detail service (inline album expansion + per-release actions)
        "Views/Library/AlbumExpansionPanel.cs",
        "Views/Library/AlbumExpansionPanel.TrackActions.cs",
        "Views/AlbumDetail/",
        # future: with the join / restore / unlock story (library switch + unlock)
        "Views/Library/LibrariesDialog.cs",
        "Views/Library/UnlockDialog.cs",
    ],
    # The Avalonia app: the composition boundary and the flows ported so far. The
    # parity port grows this set as it moves each area onto the service layer.
    "bae-avalonia": COMPOSITION_BOUNDARY
    + [
        "App.axaml.cs",
        # The unlock dialog holds its own NativeBae.UnlockLibrary call, fenced
        # under the join / restore / unlock flow (as in the WinUI app).
        "Views/Library/UnlockDialog.cs",
    ],
}

NATIVE_REF = re.compile(r"\bNativeBae\.")
LINE_COMMENT = re.compile(r"//.*$", re.MULTILINE)
BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)
# A service delegate property: `public Func<...> Name { get; init; }` /
# `public Action<...> Name { get; init; }`.
DELEGATE_DECL = re.compile(
    r"public\s+(?:Func|Action)<.*>\s+(\w+)\s*\{\s*get;\s*init;\s*\}"
)


def rel(app_dir, path):
    return path.relative_to(app_dir).as_posix()


def is_allowed(allowed, relpath):
    for entry in allowed:
        if entry.endswith("/"):
            if relpath.startswith(entry):
                return True
        elif relpath == entry:
            return True
    return False


def strip_comments(text):
    return LINE_COMMENT.sub("", BLOCK_COMMENT.sub("", text))


def production_cs_files(app_dir):
    for path in sorted(app_dir.rglob("*.cs")):
        if rel(app_dir, path).startswith(app_dir.name + TESTS_SUFFIX + "/"):
            continue
        yield path


def check_boundary(app_dir, allowed):
    """Fail if any NativeBae reference lives outside the sanctioned set."""
    violations = []
    for path in production_cs_files(app_dir):
        relpath = rel(app_dir, path)
        if is_allowed(allowed, relpath):
            continue
        code = strip_comments(path.read_text(errors="ignore"))
        if NATIVE_REF.search(code):
            lines = [
                i
                for i, line in enumerate(
                    strip_comments(path.read_text(errors="ignore")).splitlines(), 1
                )
                if NATIVE_REF.search(line)
            ]
            violations.append((relpath, lines))
    return violations


def service_delegates(app_dir):
    """Every delegate property name declared on the service classes."""
    names = []
    for path in sorted((app_dir / SERVICES_DIR).glob("*.cs")):
        for match in DELEGATE_DECL.finditer(path.read_text(errors="ignore")):
            names.append((path.stem, match.group(1)))
    return names


def unconsumed_delegates(app_dir):
    """Service delegates with no consumer outside Services/ (and outside tests).

    A consumer accesses the delegate on a service instance — `appService.Playback.Pause`.
    Every delegate name mirrors its `NativeBae` method name (that's the wiring:
    `Pause = () => session.WithCurrentHandle(NativeBae.Pause)`), so a raw
    `NativeBae.Pause` call in an unmigrated flow also reads as `.Pause`; those are
    subtracted so only genuine service-delegate access counts as a consumer.
    """
    # One blob of every consumer-candidate source (a view or store), comments
    # stripped so a doc-comment mention of a delegate name doesn't read as a
    # consumer. The composition boundary is excluded: the Services/ wiring itself,
    # and NativeBae/LibraryHandle whose `handle.<Name>(...)` wrapper bodies mirror
    # every delegate name (that plumbing is what the services wrap, not a consumer
    # of them).
    boundary = {"NativeBae.cs", "LibraryHandle.cs", "Stores/SessionStore.cs"}
    blobs = []
    for path in production_cs_files(app_dir):
        relpath = rel(app_dir, path)
        if relpath.startswith(SERVICES_DIR + "/") or relpath in boundary:
            continue
        blobs.append(strip_comments(path.read_text(errors="ignore")))
    consumers = "\n".join(blobs)
    unconsumed = []
    for service, name in service_delegates(app_dir):
        dotted = len(re.findall(r"\." + re.escape(name) + r"\b", consumers))
        native = len(re.findall(r"\bNativeBae\." + re.escape(name) + r"\b", consumers))
        if dotted - native <= 0:
            unconsumed.append((service, name))
    return unconsumed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--app",
        choices=sorted(APPS),
        help="scan only this desktop app (default: every app that exists)",
    )
    parser.add_argument(
        "--unconsumed",
        action="store_true",
        help="report service delegates with no consumer outside Services/ (never fails)",
    )
    args = parser.parse_args()

    apps = [args.app] if args.app else sorted(APPS)
    failed = False
    for name in apps:
        app_dir = ROOT / name
        if not app_dir.is_dir():
            continue
        allowed = APPS[name]
        unconsumed = unconsumed_delegates(app_dir)

        if args.unconsumed:
            print(f"=== {name} service delegates with no consumer outside Services/ ===")
            for service, delegate in unconsumed:
                print(f"  {service}.{delegate}")
            print(
                f"\n{len(unconsumed)} unconsumed delegate(s) of "
                f"{len(service_delegates(app_dir))} total.\n"
            )
            continue

        violations = check_boundary(app_dir, allowed)
        print(
            f"=== {name} bridge boundary: {len(violations)} out-of-boundary "
            f"file(s); {len(unconsumed)} unconsumed service delegate(s) "
            f"(run with --unconsumed to list) ==="
        )
        for relpath, lines in violations:
            print(f"  {relpath}: NativeBae. at line(s) {', '.join(map(str, lines))}")
        if violations:
            failed = True
            print(
                f"\nA view or store outside {name}'s sanctioned boundary reached "
                "for NativeBae. Route it through a domain service on AppService, "
                "or — if it belongs to an unmigrated flow — fence it by adding the "
                f"file to APPS['{name}'] in {pathlib.Path(__file__).name} under "
                "its future story.",
                file=sys.stderr,
            )

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
