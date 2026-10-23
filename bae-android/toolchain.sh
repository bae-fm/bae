# shellcheck shell=bash
# shellcheck disable=SC2034 # the scripts that source this read its variables
# Sourced (bash) by every script that builds or checks the Android app: the
# build scripts, scripts/check.sh, the pre-commit hook and CI. The toolchain
# versions are written once, in bae-android/gradle.properties, which Gradle also
# reads; this reads them from there and finds the SDK, JDK and NDK they name.

BAE_ANDROID_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# android_property <name>: the value of bae.<name> in gradle.properties.
android_property() {
    local value
    value=$(sed -n "s/^bae\.$1=//p" "$BAE_ANDROID_DIR/gradle.properties")
    if [[ -z "$value" ]]; then
        echo "toolchain.sh: bae.$1 is not set in $BAE_ANDROID_DIR/gradle.properties" >&2
        return 1
    fi
    printf '%s\n' "$value"
}

ANDROID_MIN_SDK=$(android_property minSdk)
ANDROID_COMPILE_SDK=$(android_property compileSdk)
ANDROID_BUILD_TOOLS_VERSION=$(android_property buildToolsVersion)
ANDROID_NDK_VERSION=$(android_property ndkVersion)
ANDROID_JAVA_VERSION=$(android_property javaVersion)
# Comma-separated.
ANDROID_ABIS=$(android_property abis)

export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
ANDROID_PLATFORM_JAR="$ANDROID_HOME/platforms/android-$ANDROID_COMPILE_SDK/android.jar"

# Gradle runs on any JDK at least bae.javaVersion. Without JAVA_HOME, Android
# Studio's bundled JDK, then Homebrew's.
if [[ -z "${JAVA_HOME:-}" ]]; then
    for _jdk_home in \
        "/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
        "$(brew --prefix "openjdk@$ANDROID_JAVA_VERSION" 2>/dev/null)/libexec/openjdk.jdk/Contents/Home"; do
        if [[ -x "$_jdk_home/bin/java" ]]; then
            export JAVA_HOME="$_jdk_home"
            break
        fi
    done
    unset _jdk_home
fi

# The prebuilt FFmpeg per arch (aarch64, x86_64), which
# scripts/build-ffmpeg-android.sh downloads.
ANDROID_FFMPEG_DIR="$(dirname "$BAE_ANDROID_DIR")/bae-ffmpeg/android"

# android_rust_target <abi>: the Rust target triple that builds for an ABI.
android_rust_target() {
    case "$1" in
        arm64-v8a) echo aarch64-linux-android ;;
        x86_64) echo x86_64-linux-android ;;
        *) echo "toolchain.sh: no Rust target for ABI $1" >&2; return 1 ;;
    esac
}

# android_rust_env: points cargo at the pinned NDK for each ABI's target,
# against bae.minSdk's libc, so the libraries call nothing a phone at the
# app's oldest Android lacks and load on every phone the app installs on.
# ffmpeg-sys-next finds FFmpeg through FFMPEG_DIR and then does not pass the
# target to bindgen, so the per-target clang args are what keep its struct
# layouts from being the host's. Sets ANDROID_NDK_TOOLCHAIN.
android_rust_env() {
    local ndk="$ANDROID_HOME/ndk/$ANDROID_NDK_VERSION" host_tag
    if [[ ! -d "$ndk" ]]; then
        echo "toolchain.sh: NDK $ANDROID_NDK_VERSION is not installed at $ndk" >&2
        return 1
    fi
    # NDK toolchains are named by host.
    case "$(uname -s)" in
        Darwin) host_tag="darwin-x86_64" ;;
        Linux) host_tag="linux-x86_64" ;;
        *) echo "toolchain.sh: unsupported host OS $(uname -s)" >&2; return 1 ;;
    esac
    ANDROID_NDK_TOOLCHAIN="$ndk/toolchains/llvm/prebuilt/$host_tag"

    local abi triple var clang
    for abi in ${ANDROID_ABIS//,/ }; do
        triple=$(android_rust_target "$abi") || return 1
        var="${triple//-/_}"
        clang="$ANDROID_NDK_TOOLCHAIN/bin/$triple$ANDROID_MIN_SDK-clang"
        export "CARGO_TARGET_$(tr '[:lower:]' '[:upper:]' <<<"$var")_LINKER=$clang"
        export "CC_$var=$clang"
        export "AR_$var=$ANDROID_NDK_TOOLCHAIN/bin/llvm-ar"
        export "BINDGEN_EXTRA_CLANG_ARGS_$var=--target=$triple$ANDROID_MIN_SDK --sysroot=$ANDROID_NDK_TOOLCHAIN/sysroot -I$ANDROID_FFMPEG_DIR/${triple%%-*}/include"
    done
}
