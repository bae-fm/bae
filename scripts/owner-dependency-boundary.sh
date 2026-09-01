#!/usr/bin/env bash

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# Run the checker from the exact coven revision that bae builds against. This
# keeps the local hook and CI on the same ownership rule as the dependency.
metadata="$(cargo metadata --format-version 1 --locked)"
coven_manifest="$(printf '%s' "$metadata" | python3 -c '
import json
import sys

manifests = [
    package["manifest_path"]
    for package in json.load(sys.stdin)["packages"]
    if package["name"] == "coven"
]
if len(manifests) != 1:
    raise SystemExit(f"expected one coven package, found {len(manifests)}")
print(manifests[0])
')"
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
coven_root="$(cd "$(dirname "$coven_manifest")/../.." && pwd)"
coven_revision="$(git -C "$coven_root" rev-parse HEAD)"

# The repository uses one shared Cargo target directory. Give each pinned coven
# revision its own checker target so an identically named binary built from an
# earlier checkout cannot satisfy this invocation.
checker_target="$target_directory/owner-construction-check/$coven_revision"

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
  AudioOutput
  AudioStream
  BlobCache
  BlobStream
  BlobTransitionObserver
  CancellationRegistry
  CancellationToken
  CandidateDriver
  CandidateRuntime
  CastController
  CloudKitDriver
  ConfigHandle
  CoverUrlProvider
  Database
  Diagnostics
  DiagnosticsTransport
  DiscogsClient
  DiscogsValidationObserver
  DiskImageCache
  DownloadQueue
  EncryptionService
  ExactCloudHome
  ExtractionService
  ExtractionServiceHandle
  FetchArbiter
  FolderWatcher
  Handle
  IdProvider
  IdentifyService
  IdentifyServiceHandle
  ImportQueue
  ImportService
  ImportServiceHandle
  JoinHandle
  KeyService
  LibraryManager
  LocalSessionManager
  LruCache
  MasterCache
  McpServerController
  McpTokenProvider
  MediaUrlProvider
  MonotonicClock
  Mutex
  Notify
  OAuthClient
  OnceLock
  OutputQueue
  PcmSource
  PlaybackQueue
  PlaybackService
  QueueSweepHandle
  ReleaseCache
  ReleaseQueue
  ReleaseUploadObserver
  RemoteImageCache
  RemoteUploadQueue
  Renderer
  RendererChannel
  RendererService
  RootRemovalBackend
  RtspConnection
  Runtime
  RwLock
  SaveService
  Sender
  SequentialIdProvider
  SequentialUuidProvider
  ServiceDaemon
  SessionCache
  SparseStreamingBuffer
  SrpClient
  StorageManager
  StoreDir
  StoreKeys
  SubsonicPasswordProvider
  SubsonicServerController
  SyncController
  SyncManager
  TrackStream
  TransferService
  UiEventBus
  UploadSessions
  UploadThroughput
  UuidProvider
  WatchBackend
  WeakUploadObserver
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
  AudioStream
  BlobStream
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

cargo run --quiet \
  --manifest-path "$coven_root/tools/owner-construction-check/Cargo.toml" \
  --target-dir "$checker_target" \
  -- "${checker_args[@]}" "$ROOT"

python3 scripts/swift-owner-dependency-boundary.py
python3 scripts/ui-projection-boundary.py
python3 scripts/tests/ui-projection-boundary-test.py
