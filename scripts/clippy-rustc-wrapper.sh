#!/bin/bash
# cargo's RUSTC_WRAPPER for scripts/clippy-crates.sh. cargo runs a workspace
# crate as `<wrapper> <lints> rustc <args>`, where <lints> is the typed
# lints' wrapper (tools/typed-lints/built/typed-lints-rustc), and sccache,
# handed that, takes `rustc` for a second input file and caches nothing. So a
# workspace crate runs under the typed lints' wrapper alone, and every other
# crate runs as `<cache> rustc <args>`, which sccache caches and shares
# between worktrees. <cache> is ~/.local/bin/builds-rustc when builds
# installed it, which runs sccache and remaps a filled OUT_DIR so crates such
# as serde_core give the same bytes in every worktree; without builds (CI) it
# is sccache.
#
# Only the crates named with `-p` (cargo marks them CARGO_PRIMARY_PACKAGE) are
# linted. A workspace crate they depend on still compiles through the typed
# lints' driver, since the read rule's lints read the record that each earlier
# workspace crate's compile writes beside its metadata, and only a compile
# through the driver writes it. That compile reports none of its findings: its
# lints are capped at warnings, and the wrapper drops each warning from what
# it hands cargo. cargo files such a crate's build as linted, so a later run
# that names it with -p would never lint it. After such a compile this
# records the unit in
# <profile dir>/clippy-unlinted/<package>__<crate>-<unit hash>,
# holding the path of the unit's fingerprint folder; scripts/clippy-crates.sh
# makes the units of the crates it names stale from those records, so they are
# rebuilt, and linted.
#
# The workspace crates compile incrementally, so a lint after an edit
# recompiles only what the edit changed. `builds cargo` turns incremental
# compilation off outside main, since sccache refuses an incremental compile,
# but sccache caches nothing the driver runs anyway. When cargo passes no
# `-C incremental`, the wrapper passes
# `-C incremental=<profile dir>/incremental`, which builds keeps.
set -uo pipefail
cache=sccache
if [ -x "$HOME/.local/bin/builds-rustc" ]; then
    cache=$HOME/.local/bin/builds-rustc
fi
case "$1" in
*/typed-lints-rustc)
    out_dir= incremental= name=
    prev=
    for arg in "$@"; do
        case "$prev" in
        --out-dir) out_dir=$arg ;;
        --crate-name) name=$arg ;;
        -C) case "$arg" in incremental=*) incremental=$arg ;; esac ;;
        esac
        prev=$arg
    done
    if [ -z "$incremental" ] && [ -n "$out_dir" ]; then
        set -- "$@" -C "incremental=$out_dir/../../../../incremental"
    fi
    if [ -n "${CARGO_PRIMARY_PACKAGE:-}" ] || [ -z "$name" ]; then
        exec "$@"
    fi
    # cargo reads the compile's diagnostics and artifact notices, one JSON
    # object a line, from its standard error; the filter passes on all but
    # the warnings.
    {
        "$@" --cap-lints warn 2>&1 >&3 3>&- | python3 -c '
import json, sys
for line in sys.stdin:
    try:
        message = json.loads(line)
        warning = isinstance(message, dict) and message.get("level") == "warning"
    except ValueError:
        warning = False
    if not warning:
        sys.stderr.write(line)
        sys.stderr.flush()
' 3>&-
        status=("${PIPESTATUS[@]}")
    } 3>&1
    [ "${status[0]}" -eq 0 ] || exit "${status[0]}"
    [ "${status[1]}" -eq 0 ] || exit "${status[1]}"
    unit=${out_dir%/out}
    fingerprint="$unit/fingerprint"
    if [ -n "$out_dir" ] && [ -d "$fingerprint" ]; then
        records="$unit/../../../clippy-unlinted"
        mkdir -p "$records"
        printf '%s\n' "$fingerprint" > "$records/${CARGO_PKG_NAME}__$name-${unit##*/}"
    fi
    ;;
*)
    exec "$cache" "$@"
    ;;
esac
