#!/usr/bin/env bash
# Prints the short git rev of the coven (sync library) dependency pinned in
# Cargo.toml. Single source for stamping builds — macOS Info.plist
# (BaeCovenRev) and Android BuildConfig (BAE_COVEN_REV) — with the exact coven
# commit they were built against. coven's version is the cross-device sync
# compatibility identity, so each binary records which one it carries.
set -euo pipefail
cd "$(dirname "$0")/.."

rev=$(grep -E '^coven = .*rev = "' Cargo.toml | head -1 | sed -E 's/.*rev = "([0-9a-f]+)".*/\1/')
if [[ -z "${rev:-}" || "$rev" == coven* ]]; then
  echo "coven rev not found in Cargo.toml" >&2
  exit 1
fi
echo "${rev:0:7}"
