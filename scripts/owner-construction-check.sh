#!/usr/bin/env bash

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# coven's structural checker, pinned to a coven revision. It reads the rules
# it runs, and what it holds them to, from this workspace's
# owner-construction-check.toml. This tool pin is independent of the runtime
# dependency in Cargo.toml.
checker_revision="41a35f50d7411b4fc5f8fdc54c84b61acbb96500"
# Every checkout on this machine runs the one checker installed for its
# revision. The first checkout that needs a revision builds the checker from
# scratch, in a target folder of its own, puts it into a new folder beside
# the shared ones, and renames that folder into place, so no checkout runs a
# partly written checker. The folder's `.crates.toml` records the source cargo
# built, and a folder whose record does not name this revision is built again,
# so a checker never runs that came from other source than the revision it
# names.
checker_directory="${XDG_CACHE_HOME:-$HOME/.cache}/owner-construction-check"
checker_root="$checker_directory/$checker_revision"
built_from_revision() {
    grep -q "bae-fm/coven?rev=$checker_revision#$checker_revision" "$1/.crates.toml" 2>/dev/null &&
        [ -x "$1/bin/owner-construction-check" ]
}
if ! built_from_revision "$checker_root"; then
    mkdir -p "$checker_directory"
    install_root="$(mktemp -d "$checker_directory/.install.XXXXXX")"
    trap 'rm -rf "$install_root"' EXIT
    scripts/cargo.sh install --locked --git https://github.com/bae-fm/coven \
        --rev "$checker_revision" --root "$install_root" \
        --target-dir "$install_root/target" owner-construction-check
    built_from_revision "$install_root" || {
        echo "owner-construction-check: the installed checker does not name revision $checker_revision" >&2
        exit 1
    }
    rm -rf "$install_root/target" "$checker_root"
    mv "$install_root" "$checker_root"
fi

"$checker_root/bin/owner-construction-check"
