//! The Mac capture: builds the bridge and the Xcode project, then runs the
//! `baeShots` test bundle, which draws each scene and writes
//! `<board>@macos.{png,json}` into the output directory.

use super::{check_complete, clear, selected, CaptureError, Step};
use crate::names::{Name, Platform, Prefix};
use std::path::Path;

const XCODE_DIRECTORY: &str = "bae-macos/bae";
const FFMPEG_LIBRARIES: &str = "bae-ffmpeg/dist/lib";

/// Takes the shots `prefixes` select (all when empty) into `out`, an absolute
/// path.
pub(crate) fn capture(
    root: &Path,
    declared: &[Name],
    prefixes: &[Prefix],
    windows: bool,
    out: &Path,
) -> Result<(), CaptureError> {
    let wanted = selected(declared, Platform::Mac, prefixes)?;
    clear(out, &wanted)?;

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
        &prefixes
            .iter()
            .filter(|p| p.platform() == Platform::Mac)
            .map(|p| p.as_str().to_string())
            .collect::<Vec<_>>(),
        windows,
        ffmpeg.is_dir().then_some(ffmpeg.as_path()),
        std::env::var("DYLD_LIBRARY_PATH").ok().as_deref(),
    );
    step.run()?;

    let declared_here: Vec<Name> =
        declared.iter().filter(|n| n.platform() == Platform::Mac).cloned().collect();
    check_complete(out, Platform::Mac, &wanted, &declared_here)
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
