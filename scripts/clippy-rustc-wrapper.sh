#!/bin/sh
# cargo's RUSTC_WRAPPER for scripts/clippy-crates.sh. cargo runs a workspace
# crate as `<wrapper> clippy-driver rustc <args>`, and sccache, handed that,
# takes `rustc` for a second input file and caches nothing. Only the crates
# named with `-p` (cargo marks them CARGO_PRIMARY_PACKAGE) are linted, so they
# run under clippy-driver as before; every other crate runs as
# `sccache rustc <args>`, a plain check that sccache caches and shares between
# worktrees.
#
# cargo files a workspace crate's build under clippy's fingerprint whatever ran
# here, so a crate checked without lints would be fresh when a later run names
# it with -p, and its lints would never run. After such a check this records
# the unit in <profile dir>/clippy-unlinted/<package>__<unit>, holding the path
# of the unit's folder in .fingerprint; scripts/clippy-crates.sh makes the units
# of the crates it names stale from those records, so they are rebuilt, and
# linted.
case "$1" in
*/clippy-driver)
    if [ -n "${CARGO_PRIMARY_PACKAGE:-}" ]; then
        exec "$@"
    fi
    shift
    sccache "$@" || exit $?
    out_dir= name= extra=
    prev=
    for arg in "$@"; do
        case "$prev" in
        --out-dir) out_dir=$arg ;;
        --crate-name) name=$arg ;;
        -C) case "$arg" in extra-filename=*) extra=${arg#extra-filename=} ;; esac ;;
        esac
        prev=$arg
    done
    fingerprint="$out_dir/../.fingerprint/${CARGO_PKG_NAME}$extra"
    if [ -n "$out_dir" ] && [ -d "$fingerprint" ]; then
        records="$out_dir/../clippy-unlinted"
        mkdir -p "$records"
        printf '%s\n' "$fingerprint" > "$records/${CARGO_PKG_NAME}__$name$extra"
    fi
    ;;
*)
    exec sccache "$@"
    ;;
esac
