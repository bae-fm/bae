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

/// A shot's valid pair of files, drawn at 2 pixels to the point.
fn write_shot(dir: &Path, name: &str) {
    let name = Name::parse(name).unwrap();
    let list = crate::shots::tests::LIST.replace("mac.welcome.a", &name.to_string());
    crate::shots::tests::write_pair(dir, &name, &list, 800, 380);
}

fn write(dir: &Path, files: &[&str]) {
    for file in files {
        std::fs::write(dir.join(file), "x").unwrap();
    }
}

#[test]
fn a_capture_is_complete_when_every_selected_shot_wrote_both_files() {
    let out = tempfile::tempdir().unwrap();
    write_shot(out.path(), "mac.welcome.a");
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
    write_shot(out.path(), "mac.welcome.a");
    write(
        out.path(),
        &[
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
fn clearing_empties_the_platforms_shot_files_and_leaves_other_files() {
    let out = tempfile::tempdir().unwrap();
    write(
        out.path(),
        &[
            "mac.welcome.a@macos.png",
            "mac.welcome.a@macos.json",
            "mac.welcome.left-by-another-branch@macos.png",
            "Not A Name@macos.json",
            "android.welcome.a@android.png",
            "notes.txt",
        ],
    );
    clear(out.path(), Platform::Mac).unwrap();
    let mut left: Vec<String> = std::fs::read_dir(out.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["android.welcome.a@android.png", "notes.txt"]);
}

#[test]
fn a_shot_another_branch_left_behind_is_not_undeclared_after_clearing() {
    let out = tempfile::tempdir().unwrap();
    write_shot(out.path(), "mac.welcome.old");
    clear(out.path(), Platform::Mac).unwrap();
    write_shot(out.path(), "mac.welcome.a");
    let declared = names(&["mac.welcome.a"]);
    check_complete(out.path(), Platform::Mac, &declared, &declared).unwrap();
}

#[test]
fn a_shot_that_does_not_read_back_fails() {
    let out = tempfile::tempdir().unwrap();
    write_shot(out.path(), "mac.welcome.a");
    write_shot(out.path(), "mac.welcome.b");
    std::fs::write(out.path().join("mac.welcome.b@macos.json"), "{").unwrap();
    let declared = names(&["mac.welcome.a", "mac.welcome.b"]);
    let CaptureError::Incomplete(problems) =
        check_complete(out.path(), Platform::Mac, &declared, &declared).unwrap_err()
    else {
        panic!("not incomplete");
    };
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("unreadable: mac.welcome.b: mac.welcome.b@macos.json:"));
}

#[test]
fn a_shot_whose_files_disagree_fails() {
    let out = tempfile::tempdir().unwrap();
    write_shot(out.path(), "mac.welcome.a");
    let name = Name::parse("mac.welcome.a").unwrap();
    std::fs::write(out.path().join(name.file_name("png")), crate::shots::tests::png(100, 100))
        .unwrap();
    let declared = names(&["mac.welcome.a"]);
    let CaptureError::Incomplete(problems) =
        check_complete(out.path(), Platform::Mac, &declared, &declared).unwrap_err()
    else {
        panic!("not incomplete");
    };
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("inconsistent: mac.welcome.a: its picture is 100x100"));
}
