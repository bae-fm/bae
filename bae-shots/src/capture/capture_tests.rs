use super::*;

fn names(texts: &[&str]) -> Vec<Name> {
    texts.iter().map(|t| Name::parse(t).unwrap()).collect()
}

fn prefix(text: &str) -> Prefix {
    Prefix::parse(text).unwrap()
}

#[test]
fn no_prefix_selects_every_shot_of_the_platform() {
    let declared = names(&["mac.welcome.a", "mac.settings.b", "android.welcome.a"]);
    let chosen = selected(&declared, Platform::Mac, &[]).unwrap();
    assert_eq!(chosen, names(&["mac.welcome.a", "mac.settings.b"]));
}

#[test]
fn a_prefix_selects_the_shots_it_starts() {
    let declared = names(&["mac.welcome.a", "mac.settings.b", "android.welcome.a"]);
    let chosen = selected(&declared, Platform::Mac, &[prefix("mac.welcome.")]).unwrap();
    assert_eq!(chosen, names(&["mac.welcome.a"]));
}

#[test]
fn a_prefix_that_starts_no_declared_shot_fails() {
    let declared = names(&["mac.welcome.a"]);
    let error = selected(&declared, Platform::Mac, &[prefix("mac.welcom.")]).unwrap_err();
    assert!(matches!(error, CaptureError::UnknownPrefix(p) if p == "mac.welcom."));
}

#[test]
fn a_platform_with_no_selected_shot_fails() {
    let declared = names(&["mac.welcome.a", "android.welcome.a"]);
    let error = selected(&declared, Platform::Android, &[prefix("mac.welcome.")]).unwrap_err();
    assert!(matches!(error, CaptureError::NothingSelected(Platform::Android)));
}

fn write(dir: &Path, files: &[&str]) {
    for file in files {
        std::fs::write(dir.join(file), "x").unwrap();
    }
}

#[test]
fn a_capture_is_complete_when_every_selected_shot_wrote_both_files() {
    let out = tempfile::tempdir().unwrap();
    write(out.path(), &["mac.welcome.a@macos.png", "mac.welcome.a@macos.json"]);
    let declared = names(&["mac.welcome.a", "mac.welcome.b"]);
    check_complete(out.path(), Platform::Mac, &declared[..1], &declared).unwrap();
}

#[test]
fn a_selected_shot_with_a_file_missing_fails() {
    let out = tempfile::tempdir().unwrap();
    write(out.path(), &["mac.welcome.a@macos.png"]);
    let declared = names(&["mac.welcome.a"]);
    let CaptureError::Incomplete(problems) =
        check_complete(out.path(), Platform::Mac, &declared, &declared).unwrap_err()
    else {
        panic!("not incomplete");
    };
    assert_eq!(problems, ["missing: mac.welcome.a@macos.json was not written"]);
}

#[test]
fn a_shot_file_no_harness_declares_fails() {
    let out = tempfile::tempdir().unwrap();
    write(
        out.path(),
        &[
            "mac.welcome.a@macos.png",
            "mac.welcome.a@macos.json",
            "mac.welcome.old@macos.png",
            "Not A Name@macos.json",
            "notes.txt",
            "android.welcome.a@android.png",
        ],
    );
    let declared = names(&["mac.welcome.a"]);
    let CaptureError::Incomplete(problems) =
        check_complete(out.path(), Platform::Mac, &declared, &declared).unwrap_err()
    else {
        panic!("not incomplete");
    };
    assert_eq!(
        problems,
        [
            "undeclared: Not A Name@macos.json is no declared shot",
            "undeclared: mac.welcome.old@macos.png is no declared shot",
        ]
    );
}

#[test]
fn clearing_removes_the_selected_shots_files_only() {
    let out = tempfile::tempdir().unwrap();
    write(
        out.path(),
        &["mac.welcome.a@macos.png", "mac.welcome.a@macos.json", "mac.welcome.b@macos.png"],
    );
    clear(out.path(), &names(&["mac.welcome.a"])).unwrap();
    assert!(!out.path().join("mac.welcome.a@macos.png").exists());
    assert!(!out.path().join("mac.welcome.a@macos.json").exists());
    assert!(out.path().join("mac.welcome.b@macos.png").exists());
}
