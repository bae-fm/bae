# bae

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
| `bae-bridge` | UniFFI bridge for macOS/iOS native apps |

## Roadmap

- LP mode -- pause at side breaks, "flip" to continue
- Shuffle
- Windows and Linux

## Development setup

macOS only for now. Requires Homebrew.

**Prerequisites:**

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# System libraries
brew install cmake pkg-config libdiscid
```

**Quick start:**

```bash
# Clone with submodules
git clone --recurse-submodules <repository-url>
cd bae

# Setup bae-ffmpeg (downloads prebuilt binaries)
./scripts/setup-ffmpeg.sh

# Add to your shell profile (~/.zshrc):
export FFMPEG_DIR="$PWD/bae-ffmpeg/dist"
export PKG_CONFIG_PATH="$FFMPEG_DIR/lib/pkgconfig:$PKG_CONFIG_PATH"
export LIBRARY_PATH="$FFMPEG_DIR/lib:$LIBRARY_PATH"
export DYLD_LIBRARY_PATH="$FFMPEG_DIR/lib:$DYLD_LIBRARY_PATH"

./scripts/install-hooks.sh

# Configure
cp .env.example .env
# Edit .env with your Discogs API key (from https://www.discogs.com/settings/developers)

# Run
# See bae-macos for the native macOS app
```

Dev mode activates automatically when `.env` exists.

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
