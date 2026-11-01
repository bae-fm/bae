#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "Usage: $0 [--skip-rust] [--emulator]"
    echo "  Builds and runs the Android app on a connected physical device, and fails"
    echo "  when none is connected. --emulator runs it on an emulator instead, booting"
    echo "  one when none is running. --skip-rust skips the Rust build."
}

SKIP_RUST=false
EMULATOR=false
for arg in "$@"; do
    case "$arg" in
        -h | --help) usage; exit 0 ;;
        --skip-rust) SKIP_RUST=true ;;
        --emulator) EMULATOR=true ;;
        *) echo "Unknown argument: $arg" >&2; usage >&2; exit 2 ;;
    esac
done

cd "$(dirname "$0")/.."
./scripts/build-layout.sh

source bae-android/toolchain.sh
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"

# Pick the install target BEFORE building, so we build and ship only that
# device's ABI (a stripped single-ABI APK is ~80 MB vs ~800 MB unstripped for
# both ABIs). A physical device unless --emulator says otherwise: an emulator
# is never picked or booted in its place. A caller-set ANDROID_SERIAL is kept
# when it is the kind asked for. Exporting ANDROID_SERIAL pins both gradle's
# installDebug and the adb launch below to that one device (otherwise either
# fails when more than one device is attached). Split on tabs: a wireless
# device's name can hold spaces.
if [[ -n "${ANDROID_SERIAL:-}" ]]; then
    if [[ "$ANDROID_SERIAL" == emulator-* && "$EMULATOR" == false ]]; then
        echo "ANDROID_SERIAL=$ANDROID_SERIAL is an emulator; pass --emulator to run on it." >&2
        exit 1
    fi
    if [[ "$ANDROID_SERIAL" != emulator-* && "$EMULATOR" == true ]]; then
        echo "ANDROID_SERIAL=$ANDROID_SERIAL is not an emulator, but --emulator was passed." >&2
        exit 1
    fi
elif [[ "$EMULATOR" == false ]]; then
    ANDROID_SERIAL=$(adb devices | awk -F'\t' '$2 == "device" && $1 !~ /^emulator-/ { print $1; exit }')
    if [[ -z "$ANDROID_SERIAL" ]]; then
        echo "No physical device is connected to adb (adb devices):" >&2
        adb devices -l >&2
        echo "A phone on wireless debugging that adb discovers but can't reach (adb mdns services)" >&2
        echo "usually needs pairing again: Wireless debugging > Pair device with pairing code," >&2
        echo "then 'adb pair <ip:port> <code>'. Pass --emulator to run on an emulator instead." >&2
        exit 1
    fi
else
    ANDROID_SERIAL=$(adb devices | awk -F'\t' '$2 == "device" && $1 ~ /^emulator-/ { print $1; exit }')
    if [[ -z "$ANDROID_SERIAL" ]]; then
        AVD=$(emulator -list-avds | head -1)
        echo "No emulator running; booting $AVD"
        # -no-snapshot: boot cold and save no quick-boot snapshot on exit (about
        # 4 GB on the disk the builds share).
        emulator -avd "$AVD" -no-snapshot &
        until ANDROID_SERIAL=$(adb devices | awk -F'\t' '$2 == "device" && $1 ~ /^emulator-/ { print $1; exit }') \
            && [[ -n "$ANDROID_SERIAL" ]]; do
            sleep 1
        done
        while [ "$(adb -s "$ANDROID_SERIAL" shell getprop sys.boot_completed 2>/dev/null)" != "1" ]; do sleep 1; done
    fi
fi
export ANDROID_SERIAL
echo "Target device: $ANDROID_SERIAL"

# The device's primary ABI (e.g. arm64-v8a). Drives both the Rust cross-build and
# gradle's packaging filter so the APK carries exactly this one ABI.
ABI=$(adb -s "$ANDROID_SERIAL" shell getprop ro.product.cpu.abi | tr -d '\r')
echo "Device ABI: $ABI"

if [[ "$SKIP_RUST" == false ]]; then
    ./bae-bridge/build-android.sh --abi "$ABI"
fi

cd bae-android
# Install the full edition for local dev (OAuth on). build-android.sh's default
# BAE_BRIDGE_FEATURES=oauth-providers writes the full bindings this consumes.
./gradlew installFullDebug -Pbae.abi="$ABI"
adb shell am start -n fm.bae.app/.MainActivity
