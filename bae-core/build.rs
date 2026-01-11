use std::path::Path;
use std::process::Command;

fn main() {
    set_version_env();
    compile_cpp_storage();
    link_libsodium();
}

fn link_libsodium() {
    println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
    println!("cargo:rustc-link-search=native=/usr/local/lib");
    println!("cargo:rustc-link-lib=sodium");
}

fn set_version_env() {
    // For local dev builds, derive version from git.
    // CI sets BAE_VERSION env var before building releases.
    let version = std::env::var("BAE_VERSION").unwrap_or_else(|_| {
        Command::new("git")
            .args(["describe", "--tags", "--always"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "dev".to_string())
    });

    println!("cargo:rustc-env=BAE_VERSION={}", version);
    println!("cargo:rerun-if-env-changed=BAE_VERSION");
}

fn compile_cpp_storage() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let cpp_dir = Path::new(manifest_dir).join("cpp");
    if !cpp_dir.exists() {
        println!("cargo:warning=CPP directory not found, skipping C++ compilation");
        return;
    }
    let header = cpp_dir.join("bae_storage.h");
    let source = cpp_dir.join("bae_storage.cpp");
    let helpers_header = cpp_dir.join("bae_storage_helpers.h");
    let helpers_source = cpp_dir.join("bae_storage_helpers.cpp");
    let ffi_rs_abs = Path::new(manifest_dir).join("src/torrent/ffi.rs");
    if !header.exists() || !source.exists() || !helpers_header.exists() || !helpers_source.exists()
    {
        println!("cargo:warning=Custom storage C++ files not found, skipping compilation",);
        return;
    }
    let wrappers_source = cpp_dir.join("bae_storage_cxx_wrappers.cpp");
    // Collect include paths from pkg-config for libtorrent
    let mut include_paths = vec![
        cpp_dir.to_str().unwrap().to_string(),
        "/opt/homebrew/include".to_string(),
        "/usr/local/include".to_string(),
    ];
    if let Ok(output) = Command::new("pkg-config")
        .args(["--cflags", "libtorrent-rasterbar"])
        .output()
    {
        if output.status.success() {
            let flags = String::from_utf8_lossy(&output.stdout);
            for flag in flags.split_whitespace() {
                if let Some(path) = flag.strip_prefix("-I") {
                    let path = path.to_string();
                    if !include_paths.contains(&path) {
                        include_paths.push(path);
                    }
                }
            }
        }
    }
    // Build with all include paths - use relative path for cxx_build to get portable header paths
    let mut binding = cxx_build::bridge("src/torrent/ffi.rs");
    let base_build = binding
        .file(&source)
        .file(&helpers_source)
        .file(&wrappers_source)
        .flag("-std=c++17");
    let build = include_paths
        .iter()
        .fold(base_build, |acc, path| acc.include(path));
    build.compile("bae_storage");
    println!("cargo:rerun-if-changed={}", ffi_rs_abs.display());
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", helpers_source.display());
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", helpers_header.display());
    println!("cargo:rerun-if-changed={}", wrappers_source.display());
    let out_dir = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=bae_storage");
    println!("cargo:rustc-link-lib=torrent-rasterbar");
    println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
    println!("cargo:rustc-link-search=native=/usr/local/lib");
}
