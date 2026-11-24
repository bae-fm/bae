#!/usr/bin/env bash
# Build one dist of libvips and the libraries it loads, from source, into DIST,
# unless DIST already holds the release scripts/bae-libvips-release pins.
# Usage: scripts/build-libvips.sh TARGET DIST
#
# TARGET is macos (this Mac, built for its own architecture), linux (this
# Linux machine, built for its own architecture), windows-x86_64 or
# windows-aarch64 (this Windows machine, built with the MSVC toolchain that is
# on PATH, which must target that architecture), android-aarch64 or
# android-x86_64. The dist holds lib/ (shared libraries, and on Windows their
# import libraries) and include/, in the shape of bae-ffmpeg's dists, a bin/
# with the DLLs on Windows, and a VERSION file that names the release.
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
pkgconf_version="2.5.1"

host="$(uname -s)"
case "$host" in
    Darwin | Linux) windows_host=false ;;
    MINGW* | MSYS* | CYGWIN*) windows_host=true ;;
    *) echo "$0: unknown host $host" >&2; exit 2 ;;
esac

# The target's host must be this machine: only the Android dists cross-build.
case "$target:$host" in
    macos:Darwin | linux:Linux | android-*:Darwin | android-*:Linux) ;;
    windows-x86_64:MINGW* | windows-x86_64:MSYS* | windows-aarch64:MINGW* | windows-aarch64:MSYS*) ;;
    *) echo "$0: $target does not build on $host" >&2; exit 2 ;;
esac

# Python runs the hash checks and unpacks the archives, so no host needs a
# shasum, xz or unzip of its own: a Windows machine's python is `python`.
if python3 -c '' 2>/dev/null; then
    py=python3
else
    py=python
fi
# What a program that is not a bash script sees of a path: on Windows the
# drive form with forward slashes, since the MSYS form of a path means nothing
# to meson and its compilers.
native_path() {
    if [[ "$windows_host" == true ]]; then
        cygpath -m "$1"
    else
        printf '%s\n' "$1"
    fi
}
sha256_of() { # FILE
    "$py" -c 'import hashlib, sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$(native_path "$1")"
}
unpack() { # ARCHIVE FOLDER [MEMBER-PREFIX...]: a .tar.gz, .tar.xz, .tar.bz2 or .zip
    local archive="$1" folder="$2"
    shift 2
    "$py" -I -c '
import shutil, sys, tarfile
archive, folder, prefixes = sys.argv[1], sys.argv[2], tuple(sys.argv[3:])
if not prefixes:
    shutil.unpack_archive(archive, folder)
else:
    with tarfile.open(archive) as tar:
        # Only the members under a prefix, whose first path part is the
        # archive'"'"'s own folder.
        tar.extractall(folder, [
            member for member in tar.getmembers()
            if member.name.split("/", 1)[-1].startswith(prefixes)
        ], filter="data")
' "$(native_path "$archive")" "$(native_path "$folder")" "$@"
}
cpu_count() {
    "$py" -c 'import os; print(os.cpu_count() or 1)'
}

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

# meson, ninja and pkgconf at pinned versions, from their own releases, which
# need only the python3, the compiler and the make the build already needs: no
# pip, which a host's system python may lack, and no Homebrew package. meson
# runs from its unpacked source, and pkgconf is installed as pkg-config, which
# meson finds libraries through.
meson_sha256="ab0a6ca09f8ef70c564c8241fb5a23957886a0b53fb58412b5e07eaf07dba743"
pkgconf_sha256="cd05c9589b9f86ecf044c10a2269822bc9eb001eced2582cfffd658b0a50c243"
case "$host:$(uname -m)" in
    Darwin:*)
        ninja_asset=ninja-mac
        ninja_sha256="c99048673aa765960a99cf10c6ddb9f1fad506099ff0a0e137ad8960a88f321b" ;;
    Linux:x86_64)
        ninja_asset=ninja-linux
        ninja_sha256="5749cbc4e668273514150a80e387a957f933c6ed3f5f11e03fb30955e2bbead6" ;;
    Linux:aarch64 | Linux:arm64)
        ninja_asset=ninja-linux-aarch64
        ninja_sha256="fd2cacc8050a7f12a16a2e48f9e06fca5c14fc4c2bee2babb67b58be17a607fc" ;;
    *)
        # A Windows machine builds for the architecture its compiler targets.
        if [[ "$target" == windows-aarch64 ]]; then
            ninja_asset=ninja-winarm64
            ninja_sha256="e52f0bdef9dfb1003229dbd6508a508c4073fd017247002adc66e5e806cb0391"
        else
            ninja_asset=ninja-win
            ninja_sha256="07fc8261b42b20e71d1720b39068c2e14ffcee6396b76fb7a795fb460b78dc65"
        fi ;;
esac
tools="$cache/tools/meson-$meson_version-ninja-$ninja_version-pkgconf-$pkgconf_version-$ninja_asset"

download_checked() { # URL SHA256 FILE
    if [[ ! -f "$3" ]]; then
        curl -fL --retry 3 --retry-delay 2 -o "$3.part" "$1"
        mv "$3.part" "$3"
    fi
    if [[ "$(sha256_of "$3")" != "$2" ]]; then
        echo "$3 does not match the pinned SHA-256; deleting it." >&2
        rm -f "$3"
        exit 1
    fi
}

exe=""
if [[ "$windows_host" == true ]]; then
    exe=".exe"
    # The compiler must be on PATH, and its link.exe ahead of the coreutils'
    # link that Git's bash puts first.
    if ! command -v cl >/dev/null; then
        echo "$0: cl is not on PATH; run this where the MSVC toolchain is set up (vcvarsall)." >&2
        exit 1
    fi
    case "${VSCMD_ARG_TGT_ARCH:-}:$target" in
        x64:windows-x86_64 | arm64:windows-aarch64) ;;
        *)
            echo "$0: the MSVC toolchain targets '${VSCMD_ARG_TGT_ARCH:-unknown}', not $target." >&2
            exit 1 ;;
    esac
    PATH="$(dirname "$(command -v cl)"):$PATH"
    export PATH
fi

if [[ ! -x "$tools/bin/meson" || ! -x "$tools/bin/ninja$exe" || ! -x "$tools/bin/pkg-config$exe" ]]; then
    rm -rf "$tools"
    mkdir -p "$tools/bin"
    download_checked "https://github.com/mesonbuild/meson/releases/download/$meson_version/meson-$meson_version.tar.gz" \
        "$meson_sha256" "$cache/downloads/meson-$meson_version.tar.gz"
    download_checked "https://github.com/ninja-build/ninja/releases/download/v$ninja_version/$ninja_asset.zip" \
        "$ninja_sha256" "$cache/downloads/$ninja_asset-$ninja_version.zip"
    download_checked "https://distfiles.ariadne.space/pkgconf/pkgconf-$pkgconf_version.tar.xz" \
        "$pkgconf_sha256" "$cache/downloads/pkgconf-$pkgconf_version.tar.xz"
    # Only what meson runs from: its source tree also holds test cases whose
    # paths come near a Windows path's limit.
    unpack "$cache/downloads/meson-$meson_version.tar.gz" "$tools" meson.py mesonbuild/
    unpack "$cache/downloads/$ninja_asset-$ninja_version.zip" "$tools/bin"
    chmod 755 "$tools/bin/ninja$exe"
    printf '#!/bin/sh\nexec %s "%s/meson-%s/meson.py" "$@"\n' "$py" "$(native_path "$tools")" "$meson_version" > "$tools/bin/meson"
    chmod 755 "$tools/bin/meson"
    mkdir -p "$tools/pkgconf-src"
    unpack "$cache/downloads/pkgconf-$pkgconf_version.tar.xz" "$tools/pkgconf-src"
    (
        cd "$tools/pkgconf-src/pkgconf-$pkgconf_version"
        if [[ "$windows_host" == true ]]; then
            # No ./configure and make on Windows: pkgconf builds with meson,
            # statically, so pkg-config.exe needs no DLL beside it.
            PATH="$tools/bin:$PATH" meson setup build --prefix="$(native_path "$tools")" \
                --buildtype=release --default-library=static -Dtests=disabled
            PATH="$tools/bin:$PATH" meson install -C build --quiet
            cp "$tools/bin/pkgconf.exe" "$tools/bin/pkg-config.exe"
        else
            ./configure --prefix="$tools" --disable-shared >/dev/null
            make -j"$(cpu_count)" >/dev/null
            make install >/dev/null
            ln -sf pkgconf "$tools/bin/pkg-config"
        fi
    )
    rm -rf "$tools/pkgconf-src"
fi
export PATH="$tools/bin:$PATH"

tarball="$cache/downloads/vips-$libvips_version.tar.xz"
if [[ ! -f "$tarball" ]]; then
    curl -fL --retry 3 --retry-delay 2 -o "$tarball.part" \
        "https://github.com/libvips/libvips/releases/download/v$libvips_version/vips-$libvips_version.tar.xz"
    mv "$tarball.part" "$tarball"
fi
if [[ "$(sha256_of "$tarball")" != "$libvips_sha256" ]]; then
    echo "$tarball does not match the pinned SHA-256; deleting it." >&2
    rm -f "$tarball"
    exit 1
fi

# The folder's name is short because a Windows build's longest paths, deep in
# glib's tree, come near the 260 characters the compiler accepts.
work="$(mktemp -d "$(dirname "$dist")/.vb.XXXXXX")"
trap 'rm -rf "$work"' EXIT
stage="$work/dist"
mkdir -p "$stage"

# Machine files and the compiler's settings for each target.
# An empty array is unbound to the bash 3.2 that macOS ships, so it expands
# through ${cross[@]+...}. `native` holds the options both meson builds take on
# the machine they run on.
cross=()
native=()
# libvips has C++ sources, so meson links its library with the C++ options.
native_vips=()
case "$target" in
    macos)
        export MACOSX_DEPLOYMENT_TARGET=15.0
        ;;
    linux)
        # Each library finds the others beside it, wherever the dist is: the
        # checks of scripts/check-bundled-libraries.py require a runpath
        # relative to the library's own folder. meson quotes the value for
        # ninja's shell, so $ORIGIN reaches the linker as written.
        native=('-Dc_link_args=-Wl,-rpath,$ORIGIN')
        native_vips=('-Dc_link_args=-Wl,-rpath,$ORIGIN' '-Dcpp_link_args=-Wl,-rpath,$ORIGIN')
        ;;
    windows-x86_64 | windows-aarch64)
        # The libraries carry their C runtime, as the Rust code that loads
        # them does (`crt-static` in .cargo/config.toml), so the dist needs
        # no Visual C++ redistributable.
        native=(-Db_vscrt=static_from_buildtype)
        native_vips=("${native[@]}")
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
PKG_CONFIG_LIBDIR="$(native_path "$stage/lib/pkgconfig")"
export PKG_CONFIG_LIBDIR
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
    if [[ "$(sha256_of "$iconv_tarball")" != "3b08f5f4f9b4eb82f151a7040bfd6fe6c6fb922efe4b1659c66ea933276965e8" ]]; then
        echo "$iconv_tarball does not match the pinned SHA-256; deleting it." >&2
        rm -f "$iconv_tarball"
        exit 1
    fi
    mkdir -p "$work/iconv"
    unpack "$iconv_tarball" "$work/iconv"
    (
        cd "$work/iconv/libiconv-1.18"
        export CC="$clang" AR="$ANDROID_NDK_TOOLCHAIN/bin/llvm-ar" RANLIB="$ANDROID_NDK_TOOLCHAIN/bin/llvm-ranlib"
        ./configure --host="$triple" --prefix="$stage" --libdir="$stage/lib" \
            --enable-shared --disable-static --disable-nls --disable-rpath
        make -j"$(cpu_count)" LDFLAGS=-avoid-version
        make install
    )
    rm -f "$stage"/lib/libiconv.la "$stage"/lib/libcharset.la
    # glib's iconv test includes iconv.h from here.
    export CPATH="$stage/include"
fi

# The libraries.
cp -R "$script_dir/libvips/deps" "$work/deps"
# The downloads of the wraps are kept beside the other downloads. They are
# copied in and out, not linked, because a link is a copy in Git's bash on
# Windows.
mkdir -p "$cache/downloads/wraps"
rm -rf "$work/deps/subprojects/packagecache"
cp -R "$cache/downloads/wraps" "$work/deps/subprojects/packagecache"
mkdir -p "$stage/lib/pkgconfig"
meson setup "$(native_path "$work/deps-build")" "$(native_path "$work/deps")" ${cross[@]+"${cross[@]}"} \
    --prefix="$(native_path "$stage")" --libdir=lib ${native[@]+"${native[@]}"} \
    --buildtype=release --default-library=shared
meson install -C "$(native_path "$work/deps-build")" --quiet
cp -R "$work/deps/subprojects/packagecache/." "$cache/downloads/wraps/"
# libarchive's wrap installs no headers: archive.h and archive_entry.h are its
# whole public interface, and meson refuses to install a nested subproject's
# files from the project above it.
cp "$work"/deps/subprojects/libarchive-*/libarchive/archive.h \
    "$work"/deps/subprojects/libarchive-*/libarchive/archive_entry.h "$stage/include/"

# libvips.
mkdir -p "$work/src"
unpack "$tarball" "$work/src"
meson setup "$(native_path "$work/vips-build")" "$(native_path "$work/src/vips-$libvips_version")" ${cross[@]+"${cross[@]}"} \
    --prefix="$(native_path "$stage")" --libdir=lib ${native_vips[@]+"${native_vips[@]}"} \
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
meson install -C "$(native_path "$work/vips-build")" --quiet

# What the dist keeps: the shared libraries and the headers. On Windows the
# DLLs are in bin/ and their import libraries in lib/.
if [[ "$windows_host" == true ]]; then
    find "$stage/bin" -type f ! -name '*.dll' -delete
else
    rm -rf "$stage/bin"
fi
rm -rf "$stage/share" "$stage/lib/gio" "$stage/lib/pkgconfig"
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
    "$py" "$script_dir/check-bundled-libraries.py" "$stage"
elif [[ "$windows_host" == true ]]; then
    "$py" "$(native_path "$script_dir/check-bundled-libraries.py")" "$(native_path "$stage/bin")"
else
    "$py" "$script_dir/check-bundled-libraries.py" --platform "${target%%-*}" "$stage"
fi

printf '%s\n' "$release" > "$stage/VERSION"
chmod 755 "$stage"
if [[ -e "$dist" ]] && ! holds_pin; then
    rm -rf "$dist"
fi
# rename(2) puts the dist in place at once, and fails when another build put a
# dist there after the check above.
if "$py" -c 'import os, sys; os.rename(sys.argv[1], sys.argv[2])' "$(native_path "$stage")" "$(native_path "$dist")" 2>/dev/null; then
    echo "bae-libvips $release ($target) installed to: $dist"
elif holds_pin; then
    echo "bae-libvips $release ($target) was already installed to: $dist"
else
    echo "$0: could not put $stage in place as $dist" >&2
    exit 1
fi
