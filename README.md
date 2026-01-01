# bae

A desktop music library app. Album-oriented, metadata-first: you pick releases from Discogs or MusicBrainz, point bae at your files, and it handles storage, playback, and streaming.

## Features

**Import sources**
- Local folders (file-per-track or CUE/FLAC)
- Torrents (.torrent files)
- CD ripping (libcdio-paranoia with error correction)

**Storage**
- Cloud: S3-compatible storage (AWS, MinIO, etc.) with optional AES-GCM encryption
- Local: filesystem path with optional encryption
- Storage profiles let you configure different destinations

**Playback**
- Native audio via cpal
- Subsonic 1.16.1 API on localhost:4533 for external clients (DSub, play:Sub, etc.)
- macOS media key support

**Metadata**
- MusicBrainz with DiscID exact matching (from CUE sheets or rip logs)
- Discogs search and matching
- Cover art from local files, MusicBrainz, or Discogs

**Other**
- Torrent seeding: imported releases can be seeded back via libtorrent
- CUE/FLAC: streams individual tracks from single-file albums without splitting

## Development setup

macOS only for now. Requires Homebrew.

**Prerequisites:**

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Dioxus CLI
cargo install dioxus-cli --locked

# System libraries
brew install cmake pkg-config libdiscid libcdio libtorrent-rasterbar boost
```

**Quick start:**

```bash
# Start MinIO for local S3
docker run -d -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  quay.io/minio/minio server /data --console-address ":9001"

# Clone and setup
git clone <repository-url>
cd bae
./scripts/install-hooks.sh
npm install

# Configure
cp .env.example .env
# Edit .env:
#   BAE_ENCRYPTION_KEY=<run: openssl rand -hex 32>
#   BAE_DISCOGS_API_KEY=<from https://www.discogs.com/settings/developers>

# Run
cd bae && dx serve
```

Dev mode activates automatically when `.env` exists.

## Configuration

**Dev mode** (debug builds with `.env`): loads from `.env` file in repo root.

**Production mode** (release builds without `.env`): loads secrets from system keyring, settings from `~/.bae/config.yaml`.

## Logging

Log levels via `RUST_LOG`:

```bash
RUST_LOG=info dx serve              # General info (default)
RUST_LOG=debug dx serve             # Detailed debugging
RUST_LOG=bae=debug dx serve         # Debug only bae module
RUST_LOG=bae::import=debug dx serve # Debug specific submodule
```
