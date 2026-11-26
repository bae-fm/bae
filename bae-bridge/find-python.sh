#!/bin/bash
# Prints the path of a Python of 3.11 or later, which the bridge's Python tools
# need for tomllib. Xcode puts its own python3 (3.9) first on the PATH of a
# scheme pre-action, so the first python3 on PATH is not always the one to run.
#
# FIND_PYTHON_EXTRA_FOLDERS replaces the Homebrew folders, for the test.
#
# Looks at the python3 of each PATH folder and of Homebrew's folders, then at
# each folder's python3.N, the newest first. Exits 1 and names each Python it
# found when none is 3.11 or later.
set -euo pipefail

folders=()
IFS=: read -r -a path_folders <<<"${PATH:-}"
IFS=: read -r -a extra_folders <<<"${FIND_PYTHON_EXTRA_FOLDERS-/opt/homebrew/bin:/usr/local/bin}"
folders+=("${path_folders[@]}" ${extra_folders[@]+"${extra_folders[@]}"})

found=()
candidates=()
for folder in "${folders[@]}"; do
    [[ -n "$folder" ]] && candidates+=("$folder/python3")
done
for minor in 20 19 18 17 16 15 14 13 12 11; do
    for folder in "${folders[@]}"; do
        [[ -n "$folder" ]] && candidates+=("$folder/python3.$minor")
    done
done

for candidate in "${candidates[@]}"; do
    [[ -x "$candidate" && ! -d "$candidate" ]] || continue
    version="$("$candidate" -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>/dev/null)" || continue
    if "$candidate" -c 'import sys; sys.exit(sys.version_info < (3, 11))' 2>/dev/null; then
        echo "$candidate"
        exit 0
    fi
    found+=("$candidate ($version)")
done

{
    echo "bae-bridge needs Python 3.11 or later for tomllib, and none exists."
    if [[ ${#found[@]} -eq 0 ]]; then
        echo "No python3 was found on PATH or in /opt/homebrew/bin or /usr/local/bin."
    else
        echo "It found only: ${found[*]}"
    fi
    echo "Install a current Python, for example with: brew install python"
} >&2
exit 1
