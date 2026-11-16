use super::*;

#[test]
fn gradle_runs_the_shot_tests_into_the_directory() {
    let step = gradle(Path::new("/repo"), Path::new("/out"), &[]);
    assert_eq!(step.program, "bae-android/gradle.sh");
    assert_eq!(step.cwd, Path::new("/repo"));
    assert_eq!(
        step.args,
        [
            "--no-daemon",
            ":app:testFullDebugUnitTest",
            "--tests",
            "fm.bae.app.shots.*",
            "-Pbae.shotsDir=/out",
        ]
    );
}

#[test]
fn a_selection_passes_its_prefixes_to_the_tests() {
    let prefixes = ["android.welcome.".to_string(), "android.component.row.".to_string()];
    let step = gradle(Path::new("/repo"), Path::new("/out"), &prefixes);
    assert_eq!(
        step.args.last().map(String::as_str),
        Some("-Pbae.shotsSelect=android.welcome.,android.component.row.")
    );
}
