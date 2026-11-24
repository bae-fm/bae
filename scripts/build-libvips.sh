#!/usr/bin/env bash
# Build one dist of libvips and the libraries it loads, from source, into DIST,
# unless DIST already holds the release scripts/bae-libvips-release pins.
# Usage: scripts/build-libvips.sh TARGET DIST
#
# TARGET is macos (this Mac, built for its own architecture), android-aarch64
# or android-x86_64. The dist holds lib/ (shared libraries) and include/, in
# the shape of bae-ffmpeg's dists, and a VERSION file that names the release.
#
# The release name is v<libvips version>-bae<n>: <n> counts changes to this
# build (options, dependency pins) over one libvips version. The libvips
# tarball is pinned by its SHA-256 below. The libraries it loads are pinned by
# the wraps in scripts/libvips/deps/subprojects, each of which names its
# source and the source's hash. Change the release file with either.
#
# Two meson builds make the dist: scripts/libvips/deps builds the libraries
# and installs them with their pkg-config files, then libvips builds against
# that install as against system libraries. libvips takes only the formats
# bae reads: JPEG (libjpeg-turbo), PNG (libpng), TIFF, WebP, GIF (libvips'
# own reader), tile pyramids (libarchive), EXIF orientation (libexif) and colour profiles (lcms2). HEIC
# and JPEG 2000 are left out (plans/picture-pipeline.md, "libvips proof").
#
# The build runs in a folder beside DIST and the dist is renamed into place
# when it is whole and checked, so no build ever reads a partly written dist.
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 TARGET DIST" >&2
    exit 2
fi
target="$1"
dist="$2"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(dirname "$script_dir")"
release="$(cat "$script_dir/bae-libvips-release")"
libvips_version="${release#v}"
libvips_version="${libvips_version%%-bae*}"
libvips_sha256="5baaead3b0bb20ffdb9e9ff09aa9fda08620923df77b63b436654cb5e0b3bf94"
meson_version="1.12.1"
ninja_version="1.13.2"

holds_pin() {
    [[ -f "$dist/VERSION" && "$(cat "$dist/VERSION")" == "$release" ]]
}
if holds_pin; then
    exit 0
fi

# Downloads and tools live beside the store of dists (bae-libvips), not in it:
# builds deletes every folder of the store that no checkout's pin names.
cache="${XDG_CACHE_HOME:-$HOME/.cache}/bae-libvips-build"
mkdir -p "$cache/downloads" "$(dirname "$dist")"

# meson and ninja, at pinned versions, in a virtual environment of their own.
venv="$cache/tools/meson-$meson_version-ninja-$ninja_version"
if [[ ! -x "$venv/bin/meson" || ! -x "$venv/bin/ninja" ]]; then
    rm -rf "$venv"
    python3 -m venv "$venv"
    "$venv/bin/pip" install --quiet "meson==$meson_version" "ninja==$ninja_version"
fi
export PATH="$venv/bin:$PATH"

tarball="$cache/downloads/vips-$libvips_version.tar.xz"
if [[ ! -f "$tarball" ]]; then
    curl -fL --retry 3 --retry-delay 2 -o "$tarball.part" \
        "https://github.com/libvips/libvips/releases/download/v$libvips_version/vips-$libvips_version.tar.xz"
    mv "$tarball.part" "$tarball"
fi
if [[ "$(shasum -a 256 "$tarball" | cut -d' ' -f1)" != "$libvips_sha256" ]]; then
    echo "$tarball does not match the pinned SHA-256; deleting it." >&2
    rm -f "$tarball"
    exit 1
fi

work="$(mktemp -d "$(dirname "$dist")/.libvips-build.XXXXXX")"
trap 'rm -rf "$work"' EXIT
stage="$work/dist"
mkdir -p "$stage"

# Machine files and the compiler's settings for each target.
# An empty array is unbound to the bash 3.2 that macOS ships, so it expands
# through ${cross[@]+...}.
cross=()
case "$target" in
    macos)
        export MACOSX_DEPLOYMENT_TARGET=15.0
        ;;
    android-aarch64 | android-x86_64)
        # shellcheck source=../bae-android/toolchain.sh
        source "$repo_root/bae-android/toolchain.sh"
        android_rust_env
        arch="${target#android-}"
        triple="$arch-linux-android"
        case "$arch" in
            aarch64) cpu_family=aarch64 ;;
            x86_64) cpu_family=x86_64 ;;
        esac
        clang="$ANDROID_NDK_TOOLCHAIN/bin/$triple$ANDROID_MIN_SDK-clang"
        cat > "$work/cross.ini" <<EOF2
[binaries]
c = '$clang'
cpp = '$clang++'
ar = '$ANDROID_NDK_TOOLCHAIN/bin/llvm-ar'
ranlib = '$ANDROID_NDK_TOOLCHAIN/bin/llvm-ranlib'
strip = '$ANDROID_NDK_TOOLCHAIN/bin/llvm-strip'
pkg-config = 'pkg-config'

[host_machine]
system = 'android'
cpu_family = '$cpu_family'
cpu = '$arch'
endian = 'little'

[properties]
needs_exe_wrapper = true

# libexif calls log and exp2 and finds them in the compiler's builtins, but a
# shared library names libm itself. meson finds iconv by linking a C++ test
# program against it, so both languages name the folder libiconv is in.
[built-in options]
c_link_args = ['-lm', '-L$stage/lib']
cpp_link_args = ['-lm', '-L$stage/lib']
EOF2
        cross=(--cross-file "$work/cross.ini")
        ;;
    *)
        echo "$0: unknown target $target" >&2
        exit 2
        ;;
esac

# Neither build may find a library outside the dist it is making.
export PKG_CONFIG_LIBDIR="$stage/lib/pkgconfig"
unset PKG_CONFIG_PATH

# glib needs iconv, which Android's libc has only from API 28 on, and the app
# runs from API 26: libiconv ships beside the other libraries. libtool names a
# library by its version, which Android's package step and loader do not find
# (-avoid-version).
if [[ "$target" == android-* ]]; then
    iconv_tarball="$cache/downloads/libiconv-1.18.tar.gz"
    if [[ ! -f "$iconv_tarball" ]]; then
        curl -fL --retry 3 --retry-delay 2 -o "$iconv_tarball.part" \
            "https://ftp.gnu.org/pub/gnu/libiconv/libiconv-1.18.tar.gz"
        mv "$iconv_tarball.part" "$iconv_tarball"
    fi
    if [[ "$(shasum -a 256 "$iconv_tarball" | cut -d' ' -f1)" != "3b08f5f4f9b4eb82f151a7040bfd6fe6c6fb922efe4b1659c66ea933276965e8" ]]; then
        echo "$iconv_tarball does not match the pinned SHA-256; deleting it." >&2
        rm -f "$iconv_tarball"
        exit 1
    fi
    mkdir -p "$work/iconv"
    tar -xzf "$iconv_tarball" -C "$work/iconv"
    (
        cd "$work/iconv/libiconv-1.18"
        export CC="$clang" AR="$ANDROID_NDK_TOOLCHAIN/bin/llvm-ar" RANLIB="$ANDROID_NDK_TOOLCHAIN/bin/llvm-ranlib"
        ./configure --host="$triple" --prefix="$stage" --libdir="$stage/lib" \
            --enable-shared --disable-static --disable-nls --disable-rpath
        make -j"$(sysctl -n hw.ncpu 2>/dev/null || nproc)" LDFLAGS=-avoid-version
        make install
    )
    rm -f "$stage"/lib/libiconv.la "$stage"/lib/libcharset.la
    # glib's iconv test includes iconv.h from here.
    export CPATH="$stage/include"
fi

# The libraries.
cp -R "$script_dir/libvips/deps" "$work/deps"
mkdir -p "$cache/downloads/wraps"
rm -rf "$work/deps/subprojects/packagecache"
ln -s "$cache/downloads/wraps" "$work/deps/subprojects/packagecache"
mkdir -p "$stage/lib/pkgconfig"
meson setup "$work/deps-build" "$work/deps" ${cross[@]+"${cross[@]}"} --prefix="$stage" --libdir=lib \
    --buildtype=release --default-library=shared
meson install -C "$work/deps-build" --quiet
# libarchive's wrap installs no headers: archive.h and archive_entry.h are its
# whole public interface, and meson refuses to install a nested subproject's
# files from the project above it.
cp "$work"/deps/subprojects/libarchive-*/libarchive/archive.h \
    "$work"/deps/subprojects/libarchive-*/libarchive/archive_entry.h "$stage/include/"

# libvips.
mkdir -p "$work/src"
tar -xJf "$tarball" -C "$work/src"
meson setup "$work/vips-build" "$work/src/vips-$libvips_version" ${cross[@]+"${cross[@]}"} --prefix="$stage" --libdir=lib \
    --buildtype=release --default-library=shared --wrap-mode=nofallback \
    -Dmodules=disabled -Dintrospection=disabled -Ddocs=false -Dcplusplus=false \
    -Dexamples=false -Ddeprecated=false \
    -Dcfitsio=disabled -Dcgif=disabled -Dexif=enabled -Dfftw=disabled \
    -Dfontconfig=disabled -Darchive=enabled -Dheif=disabled -Dimagequant=disabled \
    -Djpeg=enabled -Duhdr=disabled -Djpeg-xl=disabled -Dlcms=enabled \
    -Dmagick=disabled -Dmatio=disabled -Dnifti=disabled -Dopenexr=disabled \
    -Dopenjpeg=disabled -Dopenslide=disabled -Dhighway=disabled -Dorc=disabled \
    -Dpangocairo=disabled -Dpdfium=disabled -Dpng=enabled -Dpoppler=disabled \
    -Dquantizr=disabled -Draw=disabled -Drsvg=disabled -Dspng=disabled \
    -Dtiff=enabled -Dwebp=enabled -Dzlib=enabled
meson install -C "$work/vips-build" --quiet

# What the dist keeps: the shared libraries and the headers.
rm -rf "$stage/bin" "$stage/share" "$stage/lib/gio" "$stage/lib/pkgconfig"
rm -f "$stage"/lib/*.a "$stage"/lib/libgirepository*

if [[ "$target" == macos ]]; then
    # meson names each library by its absolute install path, which would tie
    # the dist to this folder. @rpath names resolve through the binary's own
    # rpath, as bae-ffmpeg's do. Changing a name invalidates the signature, so
    # each library is signed again, ad hoc.
    for lib in "$stage"/lib/*.dylib; do
        [[ -L "$lib" ]] && continue
        install_name_tool -id "@rpath/$(basename "$lib")" "$lib"
        while read -r dependency; do
            install_name_tool -change "$dependency" "@rpath/$(basename "$dependency")" "$lib"
        done < <(otool -L "$lib" | tail -n +2 | awk '{print $1}' | grep "^$stage/" || true)
        codesign --force --sign - "$lib"
    done
    python3 "$script_dir/check-bundled-libraries.py" "$stage"
else
    python3 "$script_dir/check-bundled-libraries.py" --platform android "$stage"
fi

printf '%s\n' "$release" > "$stage/VERSION"
chmod 755 "$stage"
if [[ -e "$dist" ]] && ! holds_pin; then
    rm -rf "$dist"
fi
# rename(2) puts the dist in place at once, and fails when another build put a
# dist there after the check above.
if python3 -c 'import os, sys; os.rename(sys.argv[1], sys.argv[2])' "$stage" "$dist" 2>/dev/null; then
    echo "bae-libvips $release ($target) installed to: $dist"
elif holds_pin; then
    echo "bae-libvips $release ($target) was already installed to: $dist"
else
    echo "$0: could not put $stage in place as $dist" >&2
    exit 1
fi
