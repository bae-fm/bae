#!/bin/sh
# coven's typed lints' driver: clippy's driver with coven's typed lints beside
# clippy's own (tools/typed-lints in coven), at the coven revision below and
# for this checkout's toolchain. builds keeps one build per key on each
# machine (the `tool` line of .build-dirs, `builds help` BUILT TOOLS):
#
#   tool.sh key             print the toolchain (its host and the commit its
#                           rustc was built from) and the coven revision
#   tool.sh build <folder>  fetch coven's typed lints at that revision and
#                           build the driver into <folder> with coven's own
#                           tools/typed-lints/tool.sh, for this checkout's
#                           toolchain
#
# The driver compiles bae's crates with the compiler it links, so it must be
# built for the toolchain that compiles the rest of bae: the build hands
# coven's script this checkout's rust-toolchain.toml.
set -eu
cd "$(dirname "$0")/../.."
revision=6c16c5a2169cf5fa8f3808be78d691cedf285036

case "${1-}:$#" in
key:1)
    version=$(rustc -vV)
    commit=$(printf '%s\n' "$version" | sed -n 's/^commit-hash: //p')
    host=$(printf '%s\n' "$version" | sed -n 's/^host: //p')
    if [ -z "$commit" ] || [ -z "$host" ]; then
        echo "typed lints: rustc -vV names no commit or host" >&2
        exit 1
    fi
    echo "$host-$commit coven-$revision"
    ;;
build:2)
    out=$2
    work=${BUILDS_TOOL_WORK:?tool.sh build needs BUILDS_TOOL_WORK}
    mkdir -p "$work"
    source=$(mktemp -d "$work/coven.XXXXXX")
    trap 'rm -rf "$source"' EXIT
    git -C "$source" init --quiet
    git -C "$source" sparse-checkout set --no-cone /tools/typed-lints/ /scripts/cargo.sh
    git -C "$source" fetch --quiet --depth 1 --filter=blob:none \
        https://github.com/bae-fm/coven "$revision"
    git -C "$source" checkout --quiet FETCH_HEAD
    # Without its .git the fetched tree is no checkout, so the build's
    # `builds cargo` takes it for none.
    rm -rf "$source/.git"
    cp rust-toolchain.toml "$source/"
    "$source/tools/typed-lints/tool.sh" build "$out"
    ;;
*)
    echo "usage: tools/typed-lints/tool.sh key | tools/typed-lints/tool.sh build <folder>" >&2
    exit 2
    ;;
esac
