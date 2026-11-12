//! The Android capture: Robolectric tests under
//! `bae-android/app/src/test/java/fm/bae/app/shots` draw each board's real
//! composable and write `<board>@android.{png,json}` into the output
//! directory.

use super::{check_complete, clear, selected, CaptureError, Step};
use crate::names::{Name, Platform, Prefix};
use std::path::Path;

const BINDINGS: &str = "bae-bridge/kotlin-bindings-full/uniffi";
const CORE_STRINGS: &str = "bae-android/app/src/main/res/values/core_strings.xml";

/// Takes the shots `prefixes` select (all when empty) into `out`, an absolute
/// path.
pub(crate) fn capture(
    root: &Path,
    declared: &[Name],
    prefixes: &[Prefix],
    out: &Path,
) -> Result<(), CaptureError> {
    let wanted = selected(declared, Platform::Android, prefixes)?;
    clear(out, Platform::Android)?;

    Step::new("build-layout", root, "scripts/build-layout.sh", &[]).run()?;
    // The tests compile against the full edition's generated Kotlin bindings
    // and the localization resources, both products of the bridge build.
    if !root.join(BINDINGS).is_dir() || !root.join(CORE_STRINGS).is_file() {
        Step::new("bridge", root, "bae-bridge/build-android.sh", &["--abi", "host"]).run()?;
    }
    gradle(
        root,
        out,
        &prefixes
            .iter()
            .filter(|p| p.platform() == Platform::Android)
            .map(|p| p.as_str().to_string())
            .collect::<Vec<_>>(),
    )
    .run()?;

    let declared_here: Vec<Name> =
        declared.iter().filter(|n| n.platform() == Platform::Android).cloned().collect();
    check_complete(out, Platform::Android, &wanted, &declared_here)
}

/// The Gradle run of the shot tests. `bae.shotsDir` is where they write, and
/// `bae.shotsSelect` (the prefixes, none for every board) the boards they
/// draw: a board outside it is skipped.
pub(crate) fn gradle(root: &Path, out: &Path, prefixes: &[String]) -> Step {
    let mut step = Step::new(
        "gradle shot tests",
        root,
        "bae-android/gradle.sh",
        &["--no-daemon", ":app:testFullDebugUnitTest", "--tests", "fm.bae.app.shots.*"],
    );
    step.args.push(format!("-Pbae.shotsDir={}", out.display()));
    if !prefixes.is_empty() {
        step.args.push(format!("-Pbae.shotsSelect={}", prefixes.join(",")));
    }
    step
}

#[cfg(test)]
#[path = "android_tests.rs"]
mod tests;
