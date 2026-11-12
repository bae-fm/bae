//! The Mac capture: builds the bridge and the Xcode project, then runs the
//! `baeShots` test bundle, which draws each scene and writes
//! `<board>@macos.{png,json}` into the output directory.

use super::{check_complete, clear, taken, CaptureError, Step, NEEDS_WINDOW};
use crate::declared::Declared;
use crate::names::{Name, Platform, Prefix};
use std::path::Path;

const XCODE_DIRECTORY: &str = "bae-macos/bae";
const FFMPEG_LIBRARIES: &str = "bae-ffmpeg/dist/lib";

/// Takes the shots `prefixes` select (all when empty) into `out`, an absolute
/// path. Without `windows`, a shot shown in its window is left out, said on
/// stderr, and returned.
pub(crate) fn capture(
    root: &Path,
    declared: &[Declared],
    prefixes: &[Prefix],
    windows: bool,
    out: &Path,
) -> Result<Vec<Name>, CaptureError> {
    let taken = taken(declared, Platform::Mac, prefixes, windows)?;
    for name in &taken.not_taken {
        eprintln!("bae-shots: {name}: {NEEDS_WINDOW}");
    }
    clear(out, Platform::Mac)?;
    if taken.names.is_empty() {
        return Ok(taken.not_taken);
    }

    let xcode = root.join(XCODE_DIRECTORY);
    for step in [
        Step::new("build-layout", root, "scripts/build-layout.sh", &[]),
        Step::new("bridge", root, "bae-bridge/build-macos.sh", &[]),
        Step::new("Swift bindings", root, "bae-bridge/install-swift-bindings.sh", &["macos"]),
        Step::new("xcodegen", &xcode, "xcodegen", &[]),
    ] {
        step.run()?;
    }

    let trees =
        Step::new("Xcode trees", root, "scripts/xcode-trees.sh", &["macos-test-args"]).output()?;
    let trees: Vec<String> = trees.lines().map(str::to_string).collect();

    let ffmpeg = root.join(FFMPEG_LIBRARIES);
    let step = xcodebuild(
        &xcode,
        &trees,
        out,
        &harness_prefixes(prefixes, &taken.names),
        windows,
        ffmpeg.is_dir().then_some(ffmpeg.as_path()),
        std::env::var("DYLD_LIBRARY_PATH").ok().as_deref(),
    );
    step.run()?;

    let declared_here: Vec<Name> =
        declared.iter().map(|d| d.name.clone()).filter(|n| n.platform() == Platform::Mac).collect();
    check_complete(out, Platform::Mac, &taken.names, &declared_here)?;
    Ok(taken.not_taken)
}

/// The prefixes the harness is given: the Mac ones that start a shot this run
/// takes. The harness fails on a prefix that starts none of the scenes it
/// renders, and without `--windows` it renders no window scene.
fn harness_prefixes(prefixes: &[Prefix], taken: &[Name]) -> Vec<String> {
    prefixes
        .iter()
        .filter(|p| p.platform() == Platform::Mac && taken.iter().any(|n| p.matches(n)))
        .map(|p| p.as_str().to_string())
        .collect()
}

/// The `xcodebuild … test` of the `baeShots` scheme. Xcode forwards a
/// variable named `TEST_RUNNER_<NAME>` to the test process as `<NAME>`, so
/// the harness reads `BAE_SHOTS_OUT`, `BAE_SHOTS_SCENES` (the prefixes, none
/// for every scene) and `BAE_SHOTS_WINDOWS`. The app host links FFmpeg from
/// the repository's prebuilt dist, so its libraries go on the loader path of
/// the xcodebuild process itself.
pub(crate) fn xcodebuild(
    xcode_directory: &Path,
    tree_args: &[String],
    out: &Path,
    prefixes: &[String],
    windows: bool,
    ffmpeg_libraries: Option<&Path>,
    inherited_loader_path: Option<&str>,
) -> Step {
    let mut step = Step::new(
        "xcodebuild test",
        xcode_directory,
        "xcodebuild",
        &[
            "-project",
            "bae.xcodeproj",
            "-scheme",
            "baeShots",
            "-configuration",
            "Debug",
            "CODE_SIGNING_ALLOWED=NO",
            "CODE_SIGNING_REQUIRED=NO",
        ],
    );
    step.args.extend(tree_args.iter().cloned());
    step.args.push("test".to_string());
    let mut env = vec![("TEST_RUNNER_BAE_SHOTS_OUT", out.to_string_lossy().into_owned())];
    if !prefixes.is_empty() {
        env.push(("TEST_RUNNER_BAE_SHOTS_SCENES", prefixes.join(",")));
    }
    if windows {
        env.push(("TEST_RUNNER_BAE_SHOTS_WINDOWS", "1".to_string()));
    }
    if let Some(libraries) = ffmpeg_libraries {
        let libraries = libraries.to_string_lossy();
        env.push((
            "DYLD_LIBRARY_PATH",
            match inherited_loader_path {
                Some(inherited) if !inherited.is_empty() => {
                    format!("{libraries}:{inherited}")
                }
                _ => libraries.into_owned(),
            },
        ));
    }
    step.env = env.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    step
}

#[cfg(test)]
#[path = "mac_tests.rs"]
mod tests;
