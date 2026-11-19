#!/bin/bash
# Runs clippy, with the typed lints beside clippy's own:
# `tools/typed-lints/clippy.sh <cargo args> [-- <clippy args>]`, as
# `cargo clippy` takes them. It does what cargo-clippy does with
# clippy-driver: runs `cargo check` with the typed lints' wrapper
# (typed-lints-rustc) as the workspace's rustc wrapper and the clippy
# arguments in CLIPPY_ARGS, which the driver reads. The wrapper runs the
# driver beside it with `-Zalways-encode-mir`, so each workspace crate's
# metadata holds the bodies of its functions, which the typed lints read in
# the crates after it. A mark for a typed lint, such as
# `#[expect(clippy::reqwest_error_display, reason = "...")]`, is a lint clippy's
# own driver does not know and warns about. The hook's clippy
# (scripts/clippy-crates.sh), `builds land`'s compile check (.land-compile) and
# CI's clippy runs on every platform go through here.
#
# cargo may run on another build machine: the program BUILDS_CARGO_BEFORE
# names runs first on whichever machine runs it, and must point
# tools/typed-lints/built at the driver there. It is driver.sh unless the
# caller names a program that runs driver.sh itself (scripts/clippy-crates.sh).
set -euo pipefail
cd "$(dirname "$0")/../.."
top=$PWD
case "$(rustc -vV | sed -n 's/^host: //p')" in
*windows*)
    exe=.exe
    # Git Bash's paths (/d/a/...) mean nothing to cargo, which runs the driver
    # from RUSTC_WORKSPACE_WRAPPER, so the path is Windows' own (D:/a/...).
    top=$(cygpath -m "$PWD")
    # The driver loads the compiler's DLLs, which Windows finds through PATH,
    # from the toolchain's bin folder.
    PATH="$(cygpath -u "$(rustc --print sysroot)")/bin:$PATH"
    export PATH
    ;;
*) exe= ;;
esac

cargo_args=()
clippy_args=""
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
    cargo_args+=("$1")
    shift
done
[ "$#" -gt 0 ] && shift
for arg in "$@"; do
    clippy_args+="${arg}__CLIPPY_HACKERY__"
done

BUILDS_CARGO_BEFORE="${BUILDS_CARGO_BEFORE:-tools/typed-lints/driver.sh}" \
    RUSTC_WORKSPACE_WRAPPER="$top/tools/typed-lints/built/typed-lints-rustc$exe" \
    CLIPPY_ARGS="$clippy_args" \
    exec scripts/cargo.sh check "${cargo_args[@]}"
