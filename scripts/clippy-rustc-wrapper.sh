#!/bin/sh
# cargo's RUSTC_WRAPPER for scripts/clippy-crates.sh. cargo runs a workspace
# crate as `<wrapper> <lints> rustc <args>`, where <lints> is the typed
# lints' wrapper (tools/typed-lints/built/typed-lints-rustc), and sccache,
# handed that, takes `rustc` for a second input file and caches nothing. So
# a workspace crate runs under the typed lints' wrapper alone: every one the
# named crates depend on too, since the read rule's lints read the record
# each earlier workspace crate's compile writes beside its metadata, and only
# a compile through the driver writes it. Every other crate runs as
# `<cache> rustc <args>`, which sccache caches and shares between worktrees.
# <cache> is ~/.local/bin/builds-rustc when builds installed it, which runs
# sccache and remaps a filled OUT_DIR so crates such as serde_core give the
# same bytes in every worktree; without builds (CI) it is sccache.
#
# The workspace crates compile incrementally, so a lint after an edit
# recompiles only what the edit changed. `builds cargo` turns incremental
# compilation off outside main, since sccache refuses an incremental compile,
# but sccache caches nothing the driver runs anyway. When cargo passes no
# `-C incremental`, the wrapper passes
# `-C incremental=<profile dir>/incremental`, which builds keeps.
cache=sccache
if [ -x "$HOME/.local/bin/builds-rustc" ]; then
    cache=$HOME/.local/bin/builds-rustc
fi
case "$1" in
*/typed-lints-rustc)
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
    ;;
*)
    exec "$cache" "$@"
    ;;
esac
