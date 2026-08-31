#!/bin/bash

set -e

source "$(dirname "$0")/../lib/file-list.sh"

CHANGED_FILES=$'bae-core/Cargo.toml\nbae-core/src/lib.rs'

if ! staged_file_in_list "bae-core/Cargo.toml"; then
    echo "expected the exact staged path to match"
    exit 1
fi

if staged_file_in_list "Cargo.toml"; then
    echo "a path suffix matched a different staged file"
    exit 1
fi

if ! staged_path_under "bae-core"; then
    echo "expected a staged path below bae-core to match"
    exit 1
fi

if staged_path_under "bae"; then
    echo "a directory prefix matched a different staged directory"
    exit 1
fi

CHANGED_FILES=$'bae-core/src/lib.rs\nbae-bridge/src/lib.rs\nnotes/example.md'
changed_crates=$(changed_workspace_crates \
    bae-automation bae-bridge bae-cast bae-core bae-desktop \
    bae-loc bae-mcp bae-subsonic bae-test-support)
if [ "$changed_crates" != $'bae-bridge\nbae-core' ]; then
    echo "directly changed workspace crates were routed incorrectly:"
    echo "$changed_crates"
    exit 1
fi

CHANGED_FILES=$'bae-ios/bae/bae/AppService.swift\nBaeKit/Package.swift\nbae-core/src/lib.rs'
swift_files=$(staged_files_with_extension swift)
if [ "$swift_files" != $'bae-ios/bae/bae/AppService.swift\nBaeKit/Package.swift' ]; then
    echo "staged Swift files were routed incorrectly:"
    echo "$swift_files"
    exit 1
fi

CHANGED_FILES=$'bae-avalonia/Views/ImportPane.cs\nbae-avalonia/Views/ImportPane.axaml\nnotes/example.cs'
csharp_files=$(staged_files_with_extension cs)
if [ "$csharp_files" != $'bae-avalonia/Views/ImportPane.cs\nnotes/example.cs' ]; then
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
    bae-avalonia/Views/First.cs \
    bae-avalonia/Views/Second.cs
expected_dotnet_args=$'format\nwhitespace\nbae-avalonia/bae-avalonia.csproj\n--include\nbae-avalonia/Views/First.cs\nbae-avalonia/Views/Second.cs'
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
partial_state=$(mktemp)
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
clean_state=$(mktemp)
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
