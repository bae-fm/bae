//! The `desktop` cfg, written once for every crate with desktop-only code.
//!
//! Mobile is a sync and playback client: no import pipeline, no signal
//! extraction, no metadata providers. What only the desktop builds carry is
//! gated `#[cfg(desktop)]`. Each such crate has a `build.rs` of its own that
//! calls [`emit`], so cargo records the script by a path inside the crate (a
//! script shared by path from another crate is recorded by absolute path, which
//! a cloned build folder then resolves to the checkout it was cloned from). A
//! manifest's `[target.'cfg(..)']` tables cannot read a cfg a build script sets,
//! so the desktop-only dependencies spell it out:
//! `cfg(not(any(target_os = "ios", target_os = "android")))`.

/// Prints the cargo directives of a build script that sets `desktop` on every
/// target that is not iOS or Android. The only input is the target OS, so the
/// script reruns only when that changes, never for an edit of the crate using
/// it.
pub fn emit() {
    println!("cargo::rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo::rustc-check-cfg=cfg(desktop)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS")
        .expect("cargo sets CARGO_CFG_TARGET_OS for a build script");
    if target_os != "ios" && target_os != "android" {
        println!("cargo::rustc-cfg=desktop");
    }
}
