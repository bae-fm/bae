#!/bin/bash

staged_file_in_list() {
    local file="$1"
    grep -qxF "$file" <<< "$CHANGED_FILES"
}

staged_path_under() {
    local directory="${1%/}"
    while IFS= read -r file; do
        case "$file" in
            "$directory"/*) return 0 ;;
        esac
    done <<< "$CHANGED_FILES"
    return 1
}

staged_files_with_extension() {
    local extension="$1"
    while IFS= read -r file; do
        case "$file" in
            *."$extension") printf '%s\n' "$file" ;;
        esac
    done <<< "$CHANGED_FILES"
}

format_dotnet_whitespace() {
    local project="$1"
    shift
    dotnet format whitespace "$project" --include "$@"
}

format_ktlint() {
    ktlint -F "$@"
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
# - "mobile <crate>" for each covered crate the mobile apps build: bae-library
#   and the workspace crates it depends on.
crate_check_plan() {
    python3 - "$@" <<'PLAN'
import json, subprocess, sys
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--offline"]))
members = set(metadata["workspace_members"])
packages = {package["id"]: package for package in metadata["packages"]}
by_name = {packages[member]["name"]: member for member in members}
nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
mobile, pending = set(), [by_name["bae-library"]]
while pending:
    member = pending.pop()
    if member in mobile:
        continue
    mobile.add(member)
    for dependency in nodes[member]["deps"]:
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
    if member in mobile:
        print("mobile", name)
PLAN
}

# The packages, of the "<package> <directory>" lines on stdin, with a staged
# path under their directory.
changed_workspace_crates() {
    local package directory
    while read -r package directory; do
        if staged_path_under "$directory"; then
            printf '%s\n' "$package"
        fi
    done
}

record_staged_worktree_state() {
    local state_file="$1"
    : > "$state_file"
    while IFS= read -r file; do
        [ -z "$file" ] && continue
        local had_unstaged=0
        if ! git diff --quiet -- "$file"; then
            had_unstaged=1
        fi
        printf '%s\t%s\t%s\n' \
            "$(worktree_file_hash "$file")" \
            "$had_unstaged" \
            "$file" >> "$state_file"
    done <<< "$CHANGED_FILES"
}

stage_formatter_changes() {
    local state_file="$1"
    local scope="${2:-}"
    local restaged=0
    local partial_changed=0
    while IFS=$'\t' read -r before_hash had_unstaged file; do
        [ -z "$file" ] && continue
        if [ -n "$scope" ]; then
            case "$file" in
                "$scope"*) ;;
                *) continue ;;
            esac
        fi
        if [ "$(worktree_file_hash "$file")" != "$before_hash" ]; then
            if [ "$had_unstaged" -eq 1 ]; then
                partial_changed=1
            else
                git add -- "$file"
                restaged=1
            fi
        fi
    done < "$state_file"
    if [ "$partial_changed" -eq 1 ]; then
        return 2
    fi
    if [ "$restaged" -eq 1 ]; then
        return 1
    fi
    return 0
}

worktree_file_hash() {
    local file="$1"
    if [ -e "$file" ] || [ -L "$file" ]; then
        git hash-object -- "$file"
    else
        printf 'missing'
    fi
}
