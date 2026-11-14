#!/bin/bash
# Which workspace crates a set of changed files reaches, and the cargo checks
# those crates need. Sourced by .land-compile, which sets CHANGED_FILES to the
# changed paths, one per line.

changed_path_under() {
    local directory="${1%/}"
    while IFS= read -r file; do
        case "$file" in
            "$directory"/*) return 0 ;;
        esac
    done <<< "$CHANGED_FILES"
    return 1
}

# The workspace's crates as Cargo reads them, one "<package> <directory>" line
# each, the directory relative to the workspace root. Fails when Cargo can't
# read the workspace.
workspace_crates() {
    python3 -c '
import json, os, subprocess
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"]))
root = metadata["workspace_root"]
for package in metadata["packages"]:
    directory = os.path.relpath(os.path.dirname(package["manifest_path"]), root)
    print(package["name"], directory)
'
}

# What the checks of the directly changed crates named as arguments cover,
# one line each:
# - "crate <crate>" for each crate a change to theirs can break: the changed
#   crates, each crate whose library depends on one of them (a normal or build
#   dependency, through any chain of such dependencies), and each crate whose
#   tests use one of those (a dev-dependency). A changed public path or
#   signature reaches all of their libraries and tests, integration tests
#   included; the libraries of a crate that only tests with one are unchanged,
#   so the crates above it are not covered through it.
# - "feature <crate>/<feature>" for each test feature a covered crate declares
#   (`test-utils`, `test-instrumentation`).
# - "feature bae-bridge/<feature>" for every feature bae-bridge declares, the
#   set the macOS Debug app builds it with (bae-bridge/build-macos.sh), so the
#   hook builds its desktop code. Other crates' app features reach them through
#   bae-bridge's in the same clippy run (one feature resolution).
# - "app-feature bae-bridge/<feature>" for each bridge feature a shipping app
#   build turns on that is not test support (`test-keyring` is), the set the
#   check of the libraries without test features builds with.
# - "mobile <crate>" for each covered crate the mobile apps build: bae-library,
#   bae-app and bae-bridge, and the workspace crates they depend on for iOS.
crate_check_plan() {
    python3 - "$@" <<'PLAN'
import json, subprocess, sys
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--offline"]))
members = set(metadata["workspace_members"])
packages = {package["id"]: package for package in metadata["packages"]}
by_name = {packages[member]["name"]: member for member in members}
# What the mobile apps build: the three crates they link, and the workspace
# crates those depend on for an iOS target, which leaves out the dependencies
# only the desktop builds declare.
ios = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--offline",
     "--filter-platform", "aarch64-apple-ios"]))
ios_nodes = {node["id"]: node for node in ios["resolve"]["nodes"]}
mobile = set()
pending = [by_name[name] for name in ("bae-library", "bae-app", "bae-bridge")]
while pending:
    member = pending.pop()
    if member in mobile:
        continue
    mobile.add(member)
    for dependency in ios_nodes[member]["deps"]:
        normal = any(kind["kind"] is None for kind in dependency["dep_kinds"])
        if dependency["pkg"] in members and normal:
            pending.append(dependency["pkg"])
# Each member's dependents among the members, from the dependencies their
# manifests declare, optional ones included: (dependent, whether its library
# uses it).
dependents = {member: [] for member in members}
for member in members:
    for declared in packages[member]["dependencies"]:
        if declared["name"] in by_name and declared.get("path"):
            dependents[by_name[declared["name"]]].append(
                (member, declared["kind"] != "dev"))
libraries, pending = set(), [by_name[name] for name in sys.argv[1:]]
while pending:
    member = pending.pop()
    if member in libraries:
        continue
    libraries.add(member)
    pending.extend(dependent for dependent, library in dependents[member] if library)
covered = libraries | {
    dependent for member in libraries for dependent, library in dependents[member]
    if not library}
for member in sorted(covered, key=lambda member: packages[member]["name"]):
    name = packages[member]["name"]
    print("crate", name)
    for feature in ("test-utils", "test-instrumentation"):
        if feature in packages[member]["features"]:
            print("feature", f"{name}/{feature}")
    if name == "bae-bridge":
        for feature in sorted(packages[member]["features"]):
            if feature != "default":
                print("feature", f"{name}/{feature}")
            if feature not in ("default", "test-keyring"):
                print("app-feature", f"{name}/{feature}")
    if member in mobile:
        print("mobile", name)
PLAN
}

# The cargo arguments (one per line) that check the libraries and binaries of
# the crates of a crate_check_plan on stdin without the test features the
# clippy over --all-targets turns on. The bridges and apps build the crates
# that way, and a `pub use` of a test-only item without its `#[cfg]` gate
# compiles in a build with `test-utils` and fails in theirs. A check with no
# target named builds each crate's library and binaries, and a crate with only
# binaries (bae-shots) is refused by `--lib`.
library_check_args() {
    echo check
    local kind value
    while read -r kind value; do
        case "$kind" in
            crate) printf '%s\n' -p "$value" ;;
            app-feature) printf '%s\n' --features "$value" ;;
        esac
    done
}

# The packages, of the "<package> <directory>" lines on stdin, with a staged
# path under their directory.
changed_workspace_crates() {
    local package directory
    while read -r package directory; do
        if changed_path_under "$directory"; then
            printf '%s\n' "$package"
        fi
    done
}
