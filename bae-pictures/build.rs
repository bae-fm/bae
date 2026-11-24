//! Links libvips from the dist that scripts/prepare-libvips.sh (macOS) or
//! scripts/build-libvips-android.sh (Android) installs, found through
//! LIBVIPS_DIR the way FFMPEG_DIR finds FFmpeg. With no dist, nothing links:
//! a crate that only checks compiles, and a binary that links fails at the
//! link step, naming the missing library.

use std::path::PathBuf;

#[expect(
    clippy::disallowed_methods,
    reason = "file system: the build script looks for the libvips dist, which no library's file reaches"
)]
fn main() {
    println!("cargo:rerun-if-env-changed=LIBVIPS_DIR");
    let Some(dir) = std::env::var_os("LIBVIPS_DIR") else {
        return;
    };
    let lib = PathBuf::from(dir).join("lib");
    if !lib.is_dir() {
        return;
    }
    println!("cargo:rerun-if-changed={}", lib.display());
    println!("cargo:rustc-link-search=native={}", lib.display());
    for name in ["vips", "gobject-2.0", "glib-2.0"] {
        println!("cargo:rustc-link-lib=dylib={name}");
    }
    // A host binary finds the dist where it was built. An Android binary runs
    // from the folder its libraries were pushed to.
    let target_os =
        std::env::var("CARGO_CFG_TARGET_OS").expect("cargo sets the target OS of a build script");
    if target_os == "android" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    } else {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
    }
}
