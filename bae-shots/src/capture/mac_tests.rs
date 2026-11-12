use super::*;

fn step(prefixes: &[&str], windows: bool, ffmpeg: Option<&str>, inherited: Option<&str>) -> Step {
    xcodebuild(
        Path::new("/repo/bae-macos/bae"),
        &[
            "<tree flag>".to_string(),
            "/b/unsigned".to_string(),
            "<packages flag>".to_string(),
            "/b/packages".to_string(),
        ],
        Path::new("/out"),
        &prefixes.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
        windows,
        ffmpeg.map(Path::new),
        inherited,
    )
}

fn env(step: &Step, key: &str) -> Option<String> {
    step.env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

#[test]
fn xcodebuild_runs_the_shots_scheme_in_the_named_trees() {
    let step = step(&[], false, None, None);
    assert_eq!(step.program, "xcodebuild");
    assert_eq!(step.cwd, Path::new("/repo/bae-macos/bae"));
    assert_eq!(
        step.args,
        [
            "-project",
            "bae.xcodeproj",
            "-scheme",
            "baeShots",
            "-configuration",
            "Debug",
            "CODE_SIGNING_ALLOWED=NO",
            "CODE_SIGNING_REQUIRED=NO",
            "<tree flag>",
            "/b/unsigned",
            "<packages flag>",
            "/b/packages",
            "test",
        ]
    );
}

#[test]
fn every_scene_is_asked_for_by_leaving_the_prefixes_out() {
    let step = step(&[], false, None, None);
    assert_eq!(env(&step, "TEST_RUNNER_BAE_SHOTS_OUT").as_deref(), Some("/out"));
    assert_eq!(env(&step, "TEST_RUNNER_BAE_SHOTS_SCENES"), None);
    assert_eq!(env(&step, "TEST_RUNNER_BAE_SHOTS_WINDOWS"), None);
    assert_eq!(env(&step, "DYLD_LIBRARY_PATH"), None);
}

#[test]
fn a_selection_passes_its_prefixes_and_windows_its_flag() {
    let step = step(&["mac.welcome.", "mac.settings."], true, None, None);
    assert_eq!(
        env(&step, "TEST_RUNNER_BAE_SHOTS_SCENES").as_deref(),
        Some("mac.welcome.,mac.settings.")
    );
    assert_eq!(env(&step, "TEST_RUNNER_BAE_SHOTS_WINDOWS").as_deref(), Some("1"));
}

#[test]
fn ffmpeg_goes_first_on_the_loader_path_of_xcodebuild() {
    let alone = step(&[], false, Some("/repo/bae-ffmpeg/dist/lib"), None);
    assert_eq!(env(&alone, "DYLD_LIBRARY_PATH").as_deref(), Some("/repo/bae-ffmpeg/dist/lib"));
    let joined = step(&[], false, Some("/ff"), Some("/other"));
    assert_eq!(env(&joined, "DYLD_LIBRARY_PATH").as_deref(), Some("/ff:/other"));
}

#[test]
fn the_harness_is_given_only_prefixes_that_start_a_shot_it_takes() {
    let prefixes =
        [Prefix::parse("mac.settings.").unwrap(), Prefix::parse("mac.welcome.").unwrap()];
    let taken = [Name::parse("mac.welcome.a").unwrap()];
    assert_eq!(harness_prefixes(&prefixes, &taken), ["mac.welcome."]);
    assert!(harness_prefixes(&[], &taken).is_empty());
}
