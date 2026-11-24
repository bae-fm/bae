//! Links libvips from the dist that scripts/prepare-libvips.sh (macOS and
//! Linux), scripts/build-libvips.sh (Windows) or
//! scripts/build-libvips-android.sh (Android) installs, found through
//! LIBVIPS_DIR the way FFMPEG_DIR finds FFmpeg, and sets the `libvips` cfg
//! that the code calling it compiles under. macOS, Linux, Windows and Android
//! always link it, and a build without the dist fails here, naming how to
//! install it. On iOS, which has no libvips yet, the crate builds without it
//! and every copy fails with `PicturesError::NoLibvips`.

use std::path::PathBuf;

// The build script runs on the host, and a Windows host builds this crate for
// Windows, so the script compiles there too, though only a Mac target links
// the dist's dylibs into its output folder.
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(windows)]
use std::os::windows::fs::symlink_file as symlink;

#[expect(
    clippy::disallowed_methods,
    reason = "file system: the build script looks for the libvips dist, which no library's file reaches"
)]
fn main() {
    println!("cargo:rerun-if-env-changed=LIBVIPS_DIR");
    println!("cargo:rustc-check-cfg=cfg(libvips)");
    let target_os =
        std::env::var("CARGO_CFG_TARGET_OS").expect("cargo sets the target OS of a build script");
    if !["macos", "linux", "windows", "android"].contains(&target_os.as_str()) {
        return;
    }
    let lib = std::env::var_os("LIBVIPS_DIR").map(|dir| PathBuf::from(dir).join("lib"));
    let Some(lib) = lib.filter(|lib| lib.is_dir()) else {
        panic!(
            "bae-pictures links libvips on {target_os}, and LIBVIPS_DIR names no dist. Run \
             `builds setup`, or scripts/prepare-libvips.sh for the Mac or Linux dist, \
             scripts/build-libvips.sh for the Windows dist and \
             scripts/build-libvips-android.sh for Android's."
        );
    };
    println!("cargo:rerun-if-changed={}", lib.display());
    println!("cargo:rustc-cfg=libvips");
    let search = if target_os == "macos" { link_into_out_dir(&lib) } else { lib.clone() };
    println!("cargo:rustc-link-search=native={}", search.display());
    // A Windows dist keeps its DLLs in bin/, which cargo puts on the PATH of
    // every test and `cargo run` binary as a native search path, as it does
    // the output folder a Mac's dylibs are linked into.
    if target_os == "windows" {
        println!("cargo:rustc-link-search=native={}", lib.with_file_name("bin").display());
    }
    for name in ["vips", "gobject-2.0", "glib-2.0"] {
        println!("cargo:rustc-link-lib=dylib={name}");
    }
    // An Android test binary runs from the folder its libraries were pushed
    // to.
    if target_os == "android" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    }
}

/// Links each of the dist's dylibs from this build's output folder, and gives
/// that folder back. A link argument of a build script reaches no dependent's
/// binary, but cargo puts the output folders its build scripts name as native
/// search paths on the fallback path (DYLD_FALLBACK_LIBRARY_PATH) of every
/// test and `cargo run` binary, which is how a binary that links libvips
/// through this crate finds its `@rpath` names.
#[expect(
    clippy::disallowed_methods,
    reason = "file system: the build script links the libvips dist into its own output folder, which no library's file reaches"
)]
fn link_into_out_dir(lib: &std::path::Path) -> PathBuf {
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    for entry in std::fs::read_dir(lib).expect("the libvips dist's lib folder lists") {
        let entry = entry.expect("an entry of the libvips dist's lib folder reads");
        if entry.path().extension().is_some_and(|extension| extension == "dylib") {
            let link = out.join(entry.file_name());
            // A link to a dist that is gone, or to an older one, is replaced.
            drop(std::fs::remove_file(&link));
            symlink(entry.path(), &link)
                .unwrap_or_else(|error| panic!("link {}: {error}", link.display()));
        }
    }
    out
}
