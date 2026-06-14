#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    echo "Usage: $0 [--skip-rust]"
    echo "  Builds and runs the Android app on emulator. Pass --skip-rust to skip the Rust build."
    exit 0
fi

cd "$(dirname "$0")/.."

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"

# Pick the install target BEFORE building, so we build and ship only that
# device's ABI (a stripped single-ABI APK is ~80 MB vs ~800 MB unstripped for
# both ABIs). Respect a caller-set ANDROID_SERIAL; otherwise prefer a physical
# device over an emulator, so a stray or crashing emulator can't hijack the
# install when a real device is plugged in. Boot an emulator only when nothing is
# connected. Exporting ANDROID_SERIAL pins both gradle's installDebug and the adb
# launch below to that one device (otherwise either fails when more than one
# device is attached).
if [[ -z "${ANDROID_SERIAL:-}" ]]; then
    ANDROID_SERIAL=$(adb devices | awk '$2 == "device" && $1 !~ /^emulator-/ { print $1; exit }')
    [[ -z "$ANDROID_SERIAL" ]] && ANDROID_SERIAL=$(adb devices | awk '$2 == "device" { print $1; exit }')
fi
if [[ -z "$ANDROID_SERIAL" ]]; then
    AVD=$(emulator -list-avds | head -1)
    echo "No device connected; booting emulator: $AVD"
    emulator -avd "$AVD" -no-snapshot-load &
    adb wait-for-device
    # Wait for boot to complete
    while [ "$(adb shell getprop sys.boot_completed 2>/dev/null)" != "1" ]; do sleep 1; done
    ANDROID_SERIAL=$(adb devices | awk '$2 == "device" { print $1; exit }')
fi
export ANDROID_SERIAL
echo "Target device: $ANDROID_SERIAL"

# The device's primary ABI (e.g. arm64-v8a). Drives both the Rust cross-build and
# gradle's packaging filter so the APK carries exactly this one ABI.
ABI=$(adb -s "$ANDROID_SERIAL" shell getprop ro.product.cpu.abi | tr -d '\r')
echo "Device ABI: $ABI"

if [[ "${1:-}" != "--skip-rust" ]]; then
    ./bae-bridge/build-android.sh --abi "$ABI"
fi

cd bae-android
./gradlew installDebug -Pbae.abi="$ABI"
adb shell am start -n fm.bae.app/.MainActivity
