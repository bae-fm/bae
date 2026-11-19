#!/bin/bash
# Lints bae-bridge for the Android target, where cfg(target_os = "android")
# code compiles, so its dead-code warnings fire; a host clippy never sees them.
# One ABI is enough: those warnings are per target OS, not per ABI.
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/build-layout.sh

source bae-android/toolchain.sh
android_rust_env
TARGET=$(android_rust_target "${ANDROID_ABIS%%,*}")
rustup target add "$TARGET"
# Into target-android, the folder build-android.sh builds into.
FFMPEG_DIR="$ANDROID_FFMPEG_DIR/${TARGET%%-*}" \
    tools/typed-lints/clippy.sh "${ANDROID_CARGO_CONFIG[@]}" --target "$TARGET" --target-dir target-android -p bae-bridge -- -D warnings
