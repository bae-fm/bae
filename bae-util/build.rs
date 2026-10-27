//! Sets the `desktop` cfg on every target that is not iOS or Android. Mobile is
//! a sync and playback client: no import pipeline, no signal extraction, no
//! metadata providers. What only the desktop builds carry is gated
//! `#[cfg(desktop)]`.
//!
//! Every crate with desktop-only code names this file as its build script, so
//! the predicate is written once. A manifest's `[target.'cfg(..)']` tables
//! cannot read a cfg a build script sets, so the desktop-only dependencies
//! spell it out: `cfg(not(any(target_os = "ios", target_os = "android")))`.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(desktop)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS")
        .expect("cargo sets CARGO_CFG_TARGET_OS for a build script");
    if target_os != "ios" && target_os != "android" {
        println!("cargo::rustc-cfg=desktop");
    }
}
