#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# The generator reads the metadata written by the UniFFI revision in Cargo.toml.
REV=e5fee5ebdf8cdac89638b531c7062bc49d300412
VERSION=0.11.0+v0.32.2

case "${1:-}" in
    --revision)
        printf '%s\n' "$REV"
        ;;
    --install)
        shift
        exec ./scripts/cargo.sh install --locked \
            --git https://github.com/bae-fm/uniffi-bindgen-cs --rev "$REV" \
            uniffi-bindgen-cs "$@"
        ;;
    --path)
        if generator="$(command -v uniffi-bindgen-cs)" &&
            [[ "$("$generator" --version)" == "uniffi-bindgen $VERSION" ]]; then
            printf '%s\n' "$generator"
        else
            echo "Run scripts/csharp-bindgen.sh --install to install uniffi-bindgen-cs $VERSION." >&2
            exit 1
        fi
        ;;
    *)
        echo "Run scripts/csharp-bindgen.sh with --install, --path or --revision." >&2
        exit 1
        ;;
esac
