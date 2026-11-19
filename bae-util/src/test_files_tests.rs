use super::*;

#[test]
fn a_folder_writes_reads_lists_and_removes_files_inside_it() {
    let folder = TestFolder::new();
    let written = folder.write("album/01.flac", b"track");
    assert_eq!(written, folder.path().join("album/01.flac"));
    assert_eq!(folder.read(&written), b"track");
    assert_eq!(folder.read_text("album/01.flac"), "track");
    assert_eq!(folder.entries("album"), ["01.flac"]);
    folder.remove("album");
    assert!(!folder.exists("album"));
}

#[test]
#[should_panic(expected = "leaves the test folder")]
fn a_path_through_the_parent_folder_panics() {
    TestFolder::new().write("../outside", b"");
}

#[test]
#[should_panic(expected = "is outside the test folder")]
fn a_full_path_elsewhere_panics() {
    let folder = TestFolder::new();
    let other = TestFolder::new();
    folder.read(other.path().join("file"));
}

#[test]
fn a_full_path_through_the_resolved_folder_is_inside() {
    let folder = TestFolder::new();
    folder.write("file", b"bytes");
    assert_eq!(folder.read(folder.resolved.join("file")), b"bytes");
}

#[test]
fn a_fixture_reads_from_the_repo() {
    let manifest = read_fixture(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
    assert!(String::from_utf8(manifest).unwrap().contains("name = \"bae-util\""));
}

#[test]
#[should_panic(expected = "is not a file in the repo")]
fn a_fixture_outside_the_repo_panics() {
    let folder = TestFolder::new();
    read_fixture(folder.write("file", b""));
}

#[test]
fn a_failed_open_gives_back_its_error() {
    let folder = TestFolder::new();
    let error = folder.try_open("missing").expect_err("no file is there");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn a_fixture_folder_lists_from_the_repo() {
    let names = fixture_entries(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
    assert!(names.contains(&"test_files.rs".to_string()), "{names:?}");
}
