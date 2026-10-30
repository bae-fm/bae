#!/usr/bin/env bash

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# The application-configurable checker has its own pin. Newer coven checkers
# enforce coven's internal workspace policy and no longer accept bae's policy.
# This tool pin is independent of the runtime dependency in Cargo.toml.
checker_revision="c24cf1d530396c1a8050213cd97e1e5812c51038"
metadata="$(cargo metadata --format-version 1 --locked)"
target_directory="$(printf '%s' "$metadata" | python3 -c '
import json
import sys

print(json.load(sys.stdin)["target_directory"])
')"
rust_roots="$(printf '%s' "$metadata" | python3 -c '
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1]).resolve()
metadata = json.load(sys.stdin)
packages = {package["id"]: package for package in metadata["packages"]}
for package_id in metadata["workspace_members"]:
    package_root = pathlib.Path(packages[package_id]["manifest_path"]).resolve().parent
    print(package_root.relative_to(root).as_posix())
' "$ROOT")"
# Isolate the executable and Cargo artifacts by the exact checker revision.
checker_target="$target_directory/owner-construction-check/$checker_revision"
checker_install="$checker_target/install"
if [ ! -x "$checker_install/bin/owner-construction-check" ]; then
    scripts/cargo.sh install --locked --git https://github.com/bae-fm/coven \
        --rev "$checker_revision" --root "$checker_install" \
        --target-dir "$checker_target" owner-construction-check
fi

# These are retained state and service capabilities, not values passed between
# operations. The checker also follows fields containing them, so an owner made
# from another owner is covered. Receivers, records, and configuration values
# are omitted.
capability_types=(
  AbortHandle
  AirPlaySink
  AirPlayStreamControl
  AppServices
  ArtworkAnalyzer
  ArtworkAnalyzerCallback
  AtomicBool
  AtomicU64
  AtomicUsize
  AudioDataReader
  # An AudioOutputDevice opens a new output for each player that asks. The
  # system device owns what its outputs share: on a Mac, the formats bae's
  # streams set the output devices to, which only one owner of both players'
  # outputs can put back.
  AudioOutput
  AudioStream
  CancellationToken
  CandidateRuntime
  CastController
  ConfigHandle
  CoverUrlProvider
  CovenHandle
  Database
  Diagnostics
  DiagnosticsTransport
  DiscogsClient
  DiscogsValidationObserver
  DiskImageCache
  ExtractionService
  ExtractionServiceHandle
  FetchArbiter
  FileStream
  FolderWatcher
  Handle
  IdSource
  IdSourceRef
  IdentifyServiceHandle
  ImportService
  ImportServiceHandle
  JoinHandle
  LibraryManager
  LocalSessionManager
  LruCache
  McpServerController
  McpTokenProvider
  MediaUrlProvider
  MonotonicClock
  Mutex
  Notify
  OnceLock
  PcmSource
  PlaybackQueue
  PlaybackService
  ReleaseQueue
  RemoteImageCache
  Renderer
  RendererChannel
  RootRemovalBackend
  RtspConnection
  Runtime
  SaveService
  Sender
  ServiceDaemon
  SessionCache
  SparseStreamingBuffer
  SrpClient
  StoreDir
  SubsonicPasswordProvider
  SubsonicServerController
  SyncController
  TrackStream
  TransferService
  UploadThroughput
  WatchBackend
  Write
  WriteAvioContext
  WriteSeek
)

# These capabilities are created for the caller's operation. They remain
# capability types so retained fields and getters are checked; only a newly
# returned instance is permitted.
allowed_capability_outputs=(
  AirPlayStreamControl
  AudioDataReader
  # An AudioOutputDevice opens a new output for each player that asks. The
  # system device owns what its outputs share: on a Mac, the formats bae's
  # streams set the output devices to, which only one owner of both players'
  # outputs can put back.
  AudioOutput
  AudioStream
  FileStream
  DiscogsClient
  ExtractionServiceHandle
  IdentifyServiceHandle
  ImportServiceHandle
)

checker_args=(--owner-dependency-only)
while IFS= read -r rust_root; do
  checker_args+=(--rust-root "$rust_root")
done <<< "$rust_roots"
for capability_type in "${capability_types[@]}"; do
  checker_args+=(--capability-type "$capability_type")
done
for output_type in "${allowed_capability_outputs[@]}"; do
  checker_args+=(--allowed-capability-output "$output_type")
done

"$checker_install/bin/owner-construction-check" "${checker_args[@]}" "$ROOT"

python3 scripts/swift-owner-dependency-boundary.py
python3 scripts/ui-projection-boundary.py
python3 scripts/tests/ui-projection-boundary-test.py
python3 scripts/keyed-value-stream-boundary.py
python3 scripts/tests/keyed-value-stream-boundary-test.py
