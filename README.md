# bae

> [!WARNING]
> bae is under active development and is not ready for general use. Builds are
> for testing while the app is pre-1.0; data and sync formats can change without
> migration.

A music library manager that uses decentralized identity and end-to-end encryption over pluggable storage for serverless, multi-device sync.

You pick releases from MusicBrainz or Discogs, point bae at your files, and it handles storage, playback, and organization. Everything in the cloud is encrypted. The storage provider sees opaque blobs. All trust lives in cryptography, not in the storage backend.

## How it works

**Import and play.** Import from local folders (file-per-track or CUE/FLAC). Match to a MusicBrainz or Discogs release for metadata, cover art, credits, label info. Browse and play with native audio, CUE pregap support, and media key integration.

**Sync across devices.** Sign in with a cloud provider (Google Drive, Dropbox, OneDrive) or configure an S3-compatible bucket. This creates your cloud home -- one encrypted location that holds everything. bae runs no server of its own; your devices sync directly through the cloud home, incrementally via changesets. Same user, multiple devices, one library. When sync hits a snag (storage full, access revoked, network out), the banner names the cause and the recovery step; transient failures back off and retry.

## Architecture

- **Identity**: each device has a locally generated Ed25519/X25519 keypair. Public keys are identities. No central identity server.
- **Encryption**: one symmetric key per library, shared across your devices. Everything in the cloud home is encrypted before it leaves the device.
- **Storage**: pluggable via a `CloudHome` trait -- Google Drive, Dropbox, OneDrive, iCloud Drive, any S3-compatible bucket, or local-only.
- **Sync**: SQLite session extension captures changesets automatically. Row-level last-writer-wins conflict resolution via hybrid logical clock. Deterministic merge.
- **Membership**: append-only chain of signed membership entries. Each changeset is signed by its author and verified against the membership chain on pull.

## Crates

| Crate | Description |
|-------|-------------|
| `bae-core` | Library, database, sync engine, encryption, cloud backends, import pipeline |
| `bae-bridge` | UniFFI bridge for the macOS/iOS/Android/Windows native apps |

## Roadmap

- LP mode -- pause at side breaks, "flip" to continue
- Shuffle
- Linux

## Versioning

Each app is versioned independently as `MAJOR.MINOR`, with a separate monotonic build number and a stamp of the exact source it was built from.

- **MAJOR -- the compatibility era, shared across every app.** It's the sync-and-storage compatibility generation: devices on the same major can sync and read each other's data. It bumps -- in lockstep across macOS, Android, and the rest -- only when the wire/on-disk format breaks: the `coven` sync format, or bae-core's own schema, membership chain, or encryption. So `macos-2.x` and `android-2.x` are the same generation no matter their minors, and `3.0` is a new, incompatible era. `coven` is a SemVer library, and a break in its *format* (not merely its API) forces a bae major bump -- though bae-core's own schema changes can force one too.
- **MINOR -- the per-app release count within the era.** Auto-incremented from that app's tags (`macos-v*`, `android-v*`). Minors diverge by design: macOS shipping more often than Android just means a higher minor. Only the major is comparable across platforms.
- **Build number** -- the CI run number, written into `CFBundleVersion` / Android `versionCode`. Monotonic; what Sparkle and the app stores use to order builds. Not user-facing.
- **Source stamp** -- every build records the bae commit and the pinned `coven` rev (macOS `Info.plist`, Android `BuildConfig`) for crash triage and sync-compatibility debugging.

Releases are tagged `macos-v<MAJOR>.<MINOR>` and `android-v<MAJOR>.<MINOR>`. Pre-1.0, everything is era `0` -- no compatibility promises, and `rm -rf ~/.bae` is the migration strategy.

## Development setup

macOS only for now. Requires Homebrew.

**Prerequisites:**

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# System libraries
brew install cmake pkg-config
```

**Quick start:**

```bash
# Clone (no submodules — coven is a pinned Cargo git dependency, fetched at build)
git clone <repository-url>
cd bae

# Setup bae-ffmpeg (downloads the prebuilt fork binaries). bae links FFmpeg only
# from here, never from a system/Homebrew install -- so local dev, CI, and
# release all link the same artifacts. Required setup.
./scripts/setup-ffmpeg.sh

# Add to your shell profile (~/.zshrc):
export FFMPEG_DIR="$PWD/bae-ffmpeg/dist"
export PKG_CONFIG_PATH="$FFMPEG_DIR/lib/pkgconfig:$PKG_CONFIG_PATH"
export LIBRARY_PATH="$FFMPEG_DIR/lib:$LIBRARY_PATH"
export DYLD_LIBRARY_PATH="$FFMPEG_DIR/lib:$DYLD_LIBRARY_PATH"
export BINDGEN_EXTRA_CLANG_ARGS="-I$FFMPEG_DIR/include ${BINDGEN_EXTRA_CLANG_ARGS:-}"

./scripts/install-hooks.sh

# Configure
cp .env.example .env
# Edit .env with your Discogs API key (from https://www.discogs.com/settings/developers)

# Run
# See bae-macos for the native macOS app
```

Dev mode activates automatically when `.env` exists.

### Building baeium yourself

baeium needs nothing proprietary to build, run, or distribute.

- **iOS** — open the Xcode project (`bae-ios`, generated by `xcodegen`) and build with your own signing. Because baeium signs with entitlements that drop the iCloud-*container* capability (a paid Apple Developer account is required to sign that one), it works under **free provisioning** — a personal Apple ID, no paid Developer Program. Build the bridge for baeium first so the app gets the baeium entitlements and S3-only bindings:

  ```bash
  BAE_BRIDGE_FEATURES= ./bae-bridge/build-ios.sh
  ```

  The iCloud Keychain still works under free provisioning, so your library's encryption keys sync across your Apple devices as usual — that needs only the keychain entitlement, which baeium keeps.

- **Android** — build the `baeium` product flavor; it carries no Google Play or other proprietary dependencies (barcode scanning uses ZXing, not ML Kit):

  ```bash
  BAE_BRIDGE_FEATURES= ./bae-bridge/build-android.sh
  cd bae-android && ./gradlew assembleBaeiumDebug
  ```

- **macOS / Windows** — build with `BAE_BRIDGE_FEATURES=desktop` for macOS and `BAE_BRIDGE_FEATURES=desktop BAE_BRIDGE_CSHARP_BINDINGS_DIR=bae-bridge/csharp-bindings-baeium ./bae-bridge/build-windows.sh` for Windows.

## Configuration

**Dev mode** (debug builds with `.env`): loads from `.env` file in repo root.

**Production mode** (release builds without `.env`): loads secrets from system keyring, settings from `~/.bae/config.yaml`.

## Logging

Log levels via `RUST_LOG`:

```bash
RUST_LOG=info              # General info (default)
RUST_LOG=debug             # Detailed debugging
RUST_LOG=bae=debug         # Debug only bae module
RUST_LOG=bae::import=debug # Debug specific submodule
```

## License

Currently unlicensed — all rights reserved while licensing is being decided. See [LICENSE](LICENSE).
