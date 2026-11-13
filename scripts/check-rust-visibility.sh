#!/usr/bin/env sh
# Fails on restricted-path visibility (`pub(in path)`) in Rust files: the files
# named as arguments, or every Rust file in the repository with none.
set -eu

cd "$(dirname "$0")/.."

rust_files() {
    if [ "$#" -gt 0 ]; then
        printf '%s\0' "$@"
    else
        git ls-files -co --exclude-standard -z -- '*.rs'
    fi
}

matches=$(
    rust_files "$@" \
        | xargs -0 grep -nHE 'pub[[:space:]]*\([[:space:]]*in([[:space:]]|::)' ||
        true
)

if [ -n "$matches" ]; then
    printf '%s\n' "$matches"
    printf '%s\n' \
        'restricted-path visibility is forbidden; move the item under its owner and use private or pub(super) access' >&2
    exit 1
fi
