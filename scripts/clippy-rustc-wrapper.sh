#!/bin/sh
# cargo's RUSTC_WRAPPER for scripts/clippy-crates.sh. cargo runs a workspace
# crate as `<wrapper> <driver> rustc <args>`, where the driver is the typed
# lints' (tools/typed-lints/built/typed-lints-driver), and sccache, handed
# that, takes `rustc` for a second input file and caches nothing. Only the
# crates named with `-p` (cargo marks them CARGO_PRIMARY_PACKAGE) are linted,
# so they run under the driver; every other crate runs as
# `<cache> rustc <args>`, a plain check that sccache caches and shares between
# worktrees. <cache> is ~/.local/bin/builds-rustc when builds installed it, which
# runs sccache and remaps a filled OUT_DIR so crates such as serde_core give the
# same bytes in every worktree; without builds (CI) it is sccache.
#
# cargo keeps each unit in <profile dir>/build/<package>/<unit hash>/, and
# passes that folder's `out` as --out-dir; its fingerprint folder is beside
# `out`. cargo files a workspace crate's build under the driver's fingerprint
# whatever ran here, so a crate checked without lints would be fresh when a
# later run names it with -p, and its lints would never run. After such a check
# this records the unit in
# <profile dir>/clippy-unlinted/<package>__<crate>-<unit hash>,
# holding the path of the unit's fingerprint folder; scripts/clippy-crates.sh
# makes the units of the crates it names stale from those records, so they are
# rebuilt, and linted.
#
# The linted crates compile incrementally, so a lint after an edit recompiles
# only what the edit changed. `builds cargo` turns incremental compilation off
# outside main, since sccache refuses an incremental compile, but sccache
# caches nothing the driver runs anyway. When cargo passes no
# `-C incremental`, the wrapper passes
# `-C incremental=<profile dir>/incremental`, which builds keeps.
cache=sccache
if [ -x "$HOME/.local/bin/builds-rustc" ]; then
    cache=$HOME/.local/bin/builds-rustc
fi
case "$1" in
*/typed-lints-driver)
    if [ -n "${CARGO_PRIMARY_PACKAGE:-}" ]; then
        out_dir= incremental=
        prev=
        for arg in "$@"; do
            case "$prev" in
            --out-dir) out_dir=$arg ;;
            -C) case "$arg" in incremental=*) incremental=$arg ;; esac ;;
            esac
            prev=$arg
        done
        if [ -z "$incremental" ] && [ -n "$out_dir" ]; then
            set -- "$@" -C "incremental=$out_dir/../../../../incremental"
        fi
        exec "$@"
    fi
    shift
    "$cache" "$@" || exit $?
    out_dir= name=
    prev=
    for arg in "$@"; do
        case "$prev" in
        --out-dir) out_dir=$arg ;;
        --crate-name) name=$arg ;;
        esac
        prev=$arg
    done
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
