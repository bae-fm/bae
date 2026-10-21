#!/usr/bin/env bash
# The baeium (S3-only) macOS bridge build exports none of the functions gated
# on the oauth-providers or cloudkit feature; availableCloudProviders stays
# (always compiled in). Run after the baeium bridge is built and its Swift
# binding installed. Each name must still be a bridge function, so a rename
# fails here instead of leaving the guard checking nothing.
set -euo pipefail

cd "$(dirname "$0")/.."

bindings=bae-bridge/swift-bindings-macos/bae_bridge.swift
gated=(
  useCloudkit:use_cloudkit
  signIn:sign_in
  signInAgain:sign_in_again
  setupOauthStorage:setup_oauth_storage
  onedriveRoot:onedrive_root
  onedriveFolders:onedrive_folders
  createOnedriveFolder:create_onedrive_folder
  dropboxSharedFolders:dropbox_shared_folders
  setOauthClients:set_oauth_clients
)

if [ ! -f "$bindings" ]; then
  echo "error: $bindings is missing; build and install the baeium macOS bridge first." >&2
  exit 1
fi

status=0
for entry in "${gated[@]}"; do
  swift_name=${entry%%:*}
  rust_name=${entry#*:}
  if ! grep -rqE "fn ${rust_name}\(" bae-bridge/src; then
    echo "error: the guard names ${rust_name}, which bae-bridge no longer defines: update this list." >&2
    status=1
  fi
  if grep -nE "func ${swift_name}\(" "$bindings"; then
    echo "error: baeium bridge exported ${swift_name} (above): the gate leaked into the S3-only build." >&2
    status=1
  fi
done
[ "$status" -eq 0 ] || exit 1
echo "baeium bridge is clean: no OAuth/CloudKit bindings exported."
