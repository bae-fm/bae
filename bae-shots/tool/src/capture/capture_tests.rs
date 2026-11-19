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
fn write_shot(dir: &bae_util::test_files::TestFolder, name: &str) {
    let name = Name::parse(name).unwrap();
    let list = crate::shots::tests::LIST.replace("mac.welcome.a", &name.to_string());
    crate::shots::tests::write_pair(dir, &name, &list, 800, 380);
}

fn write(dir: &bae_util::test_files::TestFolder, files: &[&str]) {
    for file in files {
        dir.write(file, "x");
    }
}

#[test]
fn a_capture_is_complete_when_every_selected_shot_wrote_both_files() {
    let out = bae_util::test_files::TestFolder::new();
    write_shot(&out, "mac.welcome.a");
    let declared = names(&["mac.welcome.a", "mac.welcome.b"]);
    check_complete(out.path(), Platform::Mac, &declared[..1], &declared).unwrap();
}

#[test]
fn a_selected_shot_with_a_file_missing_fails() {
    let out = bae_util::test_files::TestFolder::new();
    write(&out, &["mac.welcome.a@macos.png"]);
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
    let out = bae_util::test_files::TestFolder::new();
    write_shot(&out, "mac.welcome.a");
    write(
        &out,
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
    let out = bae_util::test_files::TestFolder::new();
    write(
        &out,
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
    let left = out.entries("");
    assert_eq!(left, ["android.welcome.a@android.png", "notes.txt"]);
}

#[test]
fn a_shot_another_branch_left_behind_is_not_undeclared_after_clearing() {
    let out = bae_util::test_files::TestFolder::new();
    write_shot(&out, "mac.welcome.old");
    clear(out.path(), Platform::Mac).unwrap();
    write_shot(&out, "mac.welcome.a");
    let declared = names(&["mac.welcome.a"]);
    check_complete(out.path(), Platform::Mac, &declared, &declared).unwrap();
}

#[test]
fn a_shot_that_does_not_read_back_fails() {
    let out = bae_util::test_files::TestFolder::new();
    write_shot(&out, "mac.welcome.a");
    write_shot(&out, "mac.welcome.b");
    out.write("mac.welcome.b@macos.json", "{");
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
    let out = bae_util::test_files::TestFolder::new();
    write_shot(&out, "mac.welcome.a");
    let name = Name::parse("mac.welcome.a").unwrap();
    out.write(name.file_name("png"), crate::shots::tests::png(100, 100));
    let declared = names(&["mac.welcome.a"]);
    let CaptureError::Incomplete(problems) =
        check_complete(out.path(), Platform::Mac, &declared, &declared).unwrap_err()
    else {
        panic!("not incomplete");
    };
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("inconsistent: mac.welcome.a: its picture is 100x100"));
}

fn declared(texts: &[(&str, bool)]) -> Vec<Declared> {
    texts
        .iter()
        .map(|(t, needs_window)| Declared {
            name: Name::parse(t).unwrap(),
            file: PathBuf::from("Scenes.swift"),
            needs_window: *needs_window,
        })
        .collect()
}

#[test]
fn a_window_shot_is_not_taken_without_windows_and_taken_with_them() {
    let all = declared(&[("mac.welcome.a", false), ("mac.settings.toolbar", true)]);
    let without = taken(&all, Platform::Mac, &[], false).unwrap();
    assert_eq!(without.names, names(&["mac.welcome.a"]));
    assert_eq!(without.not_taken, names(&["mac.settings.toolbar"]));
    let with = taken(&all, Platform::Mac, &[], true).unwrap();
    assert_eq!(with.names, names(&["mac.welcome.a", "mac.settings.toolbar"]));
    assert!(with.not_taken.is_empty());
}

#[test]
fn a_selection_of_only_window_shots_is_all_not_taken_without_windows() {
    let all = declared(&[("mac.welcome.a", false), ("mac.settings.toolbar", true)]);
    let chosen = taken(&all, Platform::Mac, &[prefix("mac.settings.")], false).unwrap();
    assert!(chosen.names.is_empty());
    assert_eq!(chosen.not_taken, names(&["mac.settings.toolbar"]));
}

#[test]
fn the_completeness_check_passes_without_a_window_shot_when_it_was_not_taken() {
    let out = bae_util::test_files::TestFolder::new();
    write_shot(&out, "mac.welcome.a");
    let all = declared(&[("mac.welcome.a", false), ("mac.settings.toolbar", true)]);
    let chosen = taken(&all, Platform::Mac, &[], false).unwrap();
    let declared: Vec<Name> = all.iter().map(|d| d.name.clone()).collect();
    check_complete(out.path(), Platform::Mac, &chosen.names, &declared).unwrap();
}

#[test]
fn the_completeness_check_reports_a_window_shot_missing_when_windows_were_given() {
    let out = bae_util::test_files::TestFolder::new();
    write_shot(&out, "mac.welcome.a");
    let all = declared(&[("mac.welcome.a", false), ("mac.settings.toolbar", true)]);
    let chosen = taken(&all, Platform::Mac, &[], true).unwrap();
    let declared: Vec<Name> = all.iter().map(|d| d.name.clone()).collect();
    let CaptureError::Incomplete(problems) =
        check_complete(out.path(), Platform::Mac, &chosen.names, &declared).unwrap_err()
    else {
        panic!("not incomplete");
    };
    assert_eq!(
        problems,
        [
            "missing: mac.settings.toolbar@macos.json was not written",
            "missing: mac.settings.toolbar@macos.png was not written",
        ]
    );
}
