#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/.."

matches=$(
    git ls-files -co --exclude-standard -z -- '*.rs' \
        | xargs -0 grep -nHE 'pub[[:space:]]*\([[:space:]]*in([[:space:]]|::)' ||
        true
)

if [ -n "$matches" ]; then
    printf '%s\n' "$matches"
    printf '%s\n' \
        'restricted-path visibility is forbidden; move the item under its owner and use private or pub(super) access' >&2
    exit 1
fi
