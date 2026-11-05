#!/bin/bash

set -e

source "$(dirname "$0")/../lib/file-list.sh"

CHANGED_FILES=$'bae-library/Cargo.toml\nbae-library/src/lib.rs'

if ! staged_file_in_list "bae-library/Cargo.toml"; then
    echo "expected the exact staged path to match"
    exit 1
fi

if staged_file_in_list "Cargo.toml"; then
    echo "a path suffix matched a different staged file"
    exit 1
fi

if ! staged_path_under "bae-library"; then
    echo "expected a staged path below bae-library to match"
    exit 1
fi

if staged_path_under "bae"; then
    echo "a directory prefix matched a different staged directory"
    exit 1
fi

# Cargo's members, every one of them, each in its own directory.
workspace=$(workspace_crates)
members=$(python3 -c '
import tomllib
print("\n".join(sorted(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["members"])))
')
listed=$(cut -d" " -f1 <<< "$workspace" | sort)
if [ "$listed" != "$members" ]; then
    echo "the hook's workspace crates differ from Cargo.toml's members:"
    diff <(echo "$listed") <(echo "$members") || true
    exit 1
fi

CHANGED_FILES=$'bae-library/src/lib.rs\nbae-mirror/src/lib.rs\nnotes/example.md'
changed_crates=$(changed_workspace_crates <<< "$workspace" | sort)
if [ "$changed_crates" != $'bae-library\nbae-mirror' ]; then
    echo "directly changed workspace crates were routed incorrectly:"
    echo "$changed_crates"
    exit 1
fi

CHANGED_FILES=$'bae-ios/bae/bae/baeApp.swift\nBaeKit/Package.swift\nbae-library/src/lib.rs'
swift_files=$(staged_files_with_extension swift)
if [ "$swift_files" != $'bae-ios/bae/bae/baeApp.swift\nBaeKit/Package.swift' ]; then
    echo "staged Swift files were routed incorrectly:"
    echo "$swift_files"
    exit 1
fi

CHANGED_FILES=$'bae-avalonia/App.axaml.cs\nbae-avalonia/App.axaml\nnotes/example.cs'
csharp_files=$(staged_files_with_extension cs)
if [ "$csharp_files" != $'bae-avalonia/App.axaml.cs\nnotes/example.cs' ]; then
    echo "staged C# files were routed incorrectly:"
    echo "$csharp_files"
    exit 1
fi

CHANGED_FILES=$'bae-android/app/src/main/java/fm/bae/ImportPane.kt\nbae-android/app/src/main/res/layout.xml\nnotes/example.kt'
kotlin_files=$(staged_files_with_extension kt)
if [ "$kotlin_files" != $'bae-android/app/src/main/java/fm/bae/ImportPane.kt\nnotes/example.kt' ]; then
    echo "staged Kotlin files were routed incorrectly:"
    echo "$kotlin_files"
    exit 1
fi

# A change reaches the crates whose libraries depend on the changed crate,
# through any chain, and the tests of the crates that test with one of those;
# not the crates above a crate that only tests with one, nor its dependencies.
plan_workspace=$(mktemp -d)
mkdir -p "$plan_workspace"/{base,bae-bridge,bae-library,tester,above-tester,unrelated}/src
printf '[workspace]\nmembers = ["base", "bae-bridge", "bae-library", "tester", "above-tester", "unrelated"]\nresolver = "2"\n' \
    > "$plan_workspace/Cargo.toml"
plan_crate() {
    local name="$1" dependencies="$2" rest="${3:-}"
    printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2024"\n\n%s\n%s\n' \
        "$name" "$dependencies" "$rest" > "$plan_workspace/$name/Cargo.toml"
    : > "$plan_workspace/$name/src/lib.rs"
}
plan_crate base "" $'[features]\ntest-utils = []'
plan_crate bae-bridge $'[dependencies]\nbae-library = { path = "../bae-library" }' $'[features]\ncast = []\ncloudkit = []'
plan_crate bae-library $'[dependencies]\nbase = { path = "../base" }'
plan_crate tester $'[dev-dependencies]\nbae-library = { path = "../bae-library" }'
plan_crate above-tester $'[dependencies]\ntester = { path = "../tester" }'
plan_crate unrelated ""
if ! plan=$(cd "$plan_workspace" && crate_check_plan base); then
    rm -rf "$plan_workspace"
    echo "the check plan of a planted workspace failed"
    exit 1
fi
rm -rf "$plan_workspace"
expected_plan=$'crate bae-bridge\nfeature bae-bridge/cast\nfeature bae-bridge/cloudkit\ncrate bae-library\nmobile bae-library\ncrate base\nfeature base/test-utils\nmobile base\ncrate tester'
if [ "$plan" != "$expected_plan" ]; then
    echo "the crates a change to base reaches were planned incorrectly:"
    echo "$plan"
    exit 1
fi

repo=$(mktemp -d)
trap 'rm -rf "$repo"' EXIT
git -C "$repo" init -q
git -C "$repo" config user.email "test@example.invalid"
git -C "$repo" config user.name "Hook Test"
printf 'base\n' > "$repo/partial.txt"
printf 'base\n' > "$repo/clean.txt"
git -C "$repo" add partial.txt clean.txt
git -C "$repo" commit -qm base

run_stage_formatter_changes() {
    local state_file="$1"
    local expected_status="$2"
    set +e
    stage_formatter_changes "$state_file"
    local status=$?
    set -e
    if [ "$status" -ne "$expected_status" ]; then
        echo "formatter staging returned $status, expected $expected_status"
        exit 1
    fi
}

cd "$repo"

mkdir "$repo/bin"
dotnet_args="$repo/dotnet-args"
printf '%s\n' \
    '#!/bin/bash' \
    'printf '\''%s\n'\'' "$@" > "$DOTNET_ARGS"' \
    > "$repo/bin/dotnet"
chmod +x "$repo/bin/dotnet"
export DOTNET_ARGS="$dotnet_args"
PATH="$repo/bin:$PATH" format_dotnet_whitespace \
    bae-avalonia/bae-avalonia.csproj \
    bae-avalonia/Program.cs \
    bae-avalonia/MainWindow.cs
expected_dotnet_args=$'format\nwhitespace\nbae-avalonia/bae-avalonia.csproj\n--include\nbae-avalonia/Program.cs\nbae-avalonia/MainWindow.cs'
if [ "$(cat "$dotnet_args")" != "$expected_dotnet_args" ]; then
    echo "dotnet whitespace formatter received the wrong arguments:"
    cat "$dotnet_args"
    exit 1
fi

ktlint_args="$repo/ktlint-args"
printf '%s\n' \
    '#!/bin/bash' \
    'printf '\''%s\n'\'' "$@" > "$KTLINT_ARGS"' \
    > "$repo/bin/ktlint"
chmod +x "$repo/bin/ktlint"
export KTLINT_ARGS="$ktlint_args"
PATH="$repo/bin:$PATH" format_ktlint \
    bae-android/app/src/main/java/fm/bae/First.kt \
    bae-android/app/src/main/java/fm/bae/Second.kt
expected_ktlint_args=$'-F\nbae-android/app/src/main/java/fm/bae/First.kt\nbae-android/app/src/main/java/fm/bae/Second.kt'
if [ "$(cat "$ktlint_args")" != "$expected_ktlint_args" ]; then
    echo "Kotlin formatter received the wrong arguments:"
    cat "$ktlint_args"
    exit 1
fi

# An unstaged hunk that existed before formatting is not formatter output.
printf 'staged\n' > partial.txt
git add partial.txt
printf 'staged\npreexisting\n' > partial.txt
CHANGED_FILES=partial.txt
# Kept in the planted repository's git dir, which the EXIT trap removes.
partial_state=$(mktemp "$repo/.git/partial-state.XXXXXX")
record_staged_worktree_state "$partial_state"
partial_index_before=$(git rev-parse :partial.txt)
run_stage_formatter_changes "$partial_state" 0
if [ "$(git rev-parse :partial.txt)" != "$partial_index_before" ]; then
    echo "preexisting unstaged content entered the index"
    exit 1
fi

# Formatter output on an otherwise fully staged file is staged, then reported.
printf 'staged\n' > clean.txt
git add clean.txt
CHANGED_FILES=clean.txt
clean_state=$(mktemp "$repo/.git/clean-state.XXXXXX")
record_staged_worktree_state "$clean_state"
printf 'formatted\n' > clean.txt
run_stage_formatter_changes "$clean_state" 1
if ! git diff --quiet -- clean.txt; then
    echo "formatter output on a fully staged file was not staged"
    exit 1
fi

# Formatter output on a partially staged file fails without changing its index.
CHANGED_FILES=partial.txt
record_staged_worktree_state "$partial_state"
partial_index_before=$(git rev-parse :partial.txt)
printf 'formatted\npreexisting\n' > partial.txt
run_stage_formatter_changes "$partial_state" 2
if [ "$(git rev-parse :partial.txt)" != "$partial_index_before" ]; then
    echo "formatter output swept a partial file into the index"
    exit 1
fi
