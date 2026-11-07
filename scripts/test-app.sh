#!/bin/sh
# Builds a commit of the macOS app in the app checkout, whose build stays warm,
# taking turns with the agents on the checkout's lock, and copies the app out,
# since the next build there replaces it. Quits a running bae, then opens the
# new one.
#
# Usage: scripts/test-app.sh [--no-open] [<ref>]   (default ref: origin/main)
set -eu

CHECKOUT="$HOME/dev/bae-worktrees/side-1"
APP="$HOME/Applications/bae-test/bae.app"
BUNDLE_ID=fm.bae.desktop

OPEN=true
REF=origin/main
for arg in "$@"; do
  case "$arg" in
    --no-open) OPEN=false ;;
    -*) echo "Usage: $0 [--no-open] [<ref>]" >&2; exit 1 ;;
    *) REF=$arg ;;
  esac
done

running() {
  [ "$(osascript -e "application id \"$BUNDLE_ID\" is running")" = true ]
}

mkdir -p "$(dirname "$APP")"
echo "Waiting for the app checkout's lock, then building $REF..."
builds lock bae sh -c '
  set -eu
  cd "$1"
  if [ -n "$(git status --porcelain)" ]; then
    git status --short
    echo "the app checkout has changes; nothing built" >&2
    exit 1
  fi
  git fetch -q origin
  git checkout -q --detach "$2"
  git log --oneline -1
  ./bae-macos/run.sh --no-open
  rm -rf "$3.new"
  ditto "$(scripts/xcode-trees.sh macos-run)/Build/Products/Debug/bae.app" "$3.new"
' test-app "$CHECKOUT" "$REF" "$APP"

if running; then
  osascript -e "tell application id \"$BUNDLE_ID\" to quit"
  for _ in $(seq 60); do
    running || break
    sleep 0.5
  done
  if running; then
    echo "bae is still running 30 seconds after being asked to quit; the new app is at $APP.new" >&2
    exit 1
  fi
fi
rm -rf "$APP"
mv "$APP.new" "$APP"
echo "Built $APP"
if [ "$OPEN" = true ]; then
  open "$APP"
fi
