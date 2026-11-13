#!/usr/bin/env bash
# Runs detekt's two passes (the default-config rules, then the type-resolved
# rules) over the hand-written Kotlin files named as arguments: the ones under
# a module's src/main/java. With no such file it runs nothing.
set -euo pipefail
cd "$(dirname "$0")/.."

inputs=()
for file in "$@"; do
    case "$file" in
        bae-android/app/src/main/java/* | bae-android/fastvalues/src/main/java/* \
            | bae-android/screensubscription/src/main/java/*)
            inputs+=("$file")
            ;;
    esac
done
[ "${#inputs[@]}" -gt 0 ] || exit 0
input=$(IFS=,; echo "${inputs[*]}")

detekt --input "$input" \
    --config bae-android/detekt.yml \
    --build-upon-default-config

# The type-resolved rules read the platform jar of the app's compileSdk.
source bae-android/toolchain.sh
if [ ! -f "$ANDROID_PLATFORM_JAR" ]; then
    echo "detekt's type-resolved rules need $ANDROID_PLATFORM_JAR; install platforms;android-${ANDROID_COMPILE_SDK}" >&2
    exit 1
fi
detekt --input "$input" \
    --config bae-android/detekt-type-resolved.yml \
    --classpath "$ANDROID_PLATFORM_JAR" \
    --jvm-target "$ANDROID_JAVA_VERSION"
