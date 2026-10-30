#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
source bae-android/toolchain.sh

./bae-bridge/build-android.sh --abi "$(android_host_abi)"
open -a "Android Studio" bae-android
