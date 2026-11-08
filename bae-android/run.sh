#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "Usage: $0 [--skip-rust]"
    echo "  Builds the Android app's full debug APK and installs it on the connected"
    echo "  phone, keeping its data, when one is connected (ANDROID_SERIAL's, else the"
    echo "  first adb lists). Opens nothing, and never boots an emulator."
    echo "  --skip-rust skips the Rust build."
}

SKIP_RUST=false
for arg in "$@"; do
    case "$arg" in
        -h | --help) usage; exit 0 ;;
        --skip-rust) SKIP_RUST=true ;;
        *) echo "Unknown argument: $arg" >&2; usage >&2; exit 2 ;;
    esac
done

cd "$(dirname "$0")/.."
./scripts/build-layout.sh

source bae-android/toolchain.sh
export PATH="$ANDROID_HOME/platform-tools:$PATH"

# Pick the phone BEFORE building, so we build and ship only its ABI (a stripped
# single-ABI APK is ~80 MB vs ~800 MB unstripped for both ABIs). An emulator is
# never picked. Exporting ANDROID_SERIAL pins gradle's installFullDebug to that
# one device (otherwise it fails when more than one is attached). Split on
# tabs: a wireless device's name can hold spaces.
if [[ -z "${ANDROID_SERIAL:-}" ]]; then
    ANDROID_SERIAL=$(adb devices | awk -F'\t' '$2 == "device" && $1 !~ /^emulator-/ { print $1; exit }')
fi
if [[ -n "$ANDROID_SERIAL" ]]; then
    export ANDROID_SERIAL
    echo "Target device: $ANDROID_SERIAL"
    # The phone's primary ABI (e.g. arm64-v8a). Drives both the Rust
    # cross-build and gradle's packaging filter so the APK carries exactly this
    # one ABI.
    ABI=$(adb -s "$ANDROID_SERIAL" shell getprop ro.product.cpu.abi | tr -d '\r')
    TASK=installFullDebug
else
    echo "No phone is connected to adb, so the APK is built and not installed:"
    adb devices -l
    echo "A phone on wireless debugging that adb discovers but can't reach (adb mdns services)"
    echo "usually needs pairing again: Wireless debugging > Pair device with pairing code,"
    echo "then 'adb pair <ip:port> <code>'."
    ABI=$(android_host_abi)
    TASK=assembleFullDebug
fi
echo "ABI: $ABI"

if [[ "$SKIP_RUST" == false ]]; then
    ./bae-bridge/build-android.sh --abi "$ABI"
fi

cd bae-android
# The full edition for local dev (OAuth on). build-android.sh's default
# BAE_BRIDGE_FEATURES=oauth-providers writes the full bindings this consumes.
./gradlew "$TASK" -Pbae.abi="$ABI"
echo "APK: $PWD/app/build/outputs/apk/full/debug/app-full-debug.apk"
