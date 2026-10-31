// ── Files whose names aren't UTF-8 ──────────────────────────────────────────
//
// Only Linux takes such a name: macOS and Windows store names as Unicode.

/// The Latin-1 spelling of `Café` followed by `extension`: a name that isn't
/// UTF-8.
#[cfg(target_os = "linux")]
fn latin1_name(extension: &str) -> std::ffi::OsString {
    use std::os::unix::ffi::OsStringExt;
    let mut name = b"Caf\xE9.".to_vec();
    name.extend_from_slice(extension.as_bytes());
    std::ffi::OsString::from_vec(name)
}

/// A file whose name isn't UTF-8 is read like any other: the scan lists its
/// release, with the file under its escaped name and its path as it is.
#[cfg(target_os = "linux")]
#[test]
fn a_file_whose_name_is_not_utf8_is_listed_under_its_escaped_name() {
    let (tmp, album) = album_dir();
    let track = album.join(latin1_name("flac"));
    std::fs::write(&track, fake_flac()).unwrap();

    let candidates = scan_valid(tmp.path());

    let [candidate] = candidates.as_slice() else {
        panic!("one release: {candidates:?}");
    };
    assert_eq!(candidate.name, "Album");
    let [entry] = candidate.files.files.as_slice() else {
        panic!("one file: {:?}", candidate.files.files);
    };
    assert_eq!(entry.role, FileRole::Audio);
    assert_eq!(entry.file.relative_path, r"Caf\xE9.flac");
    assert_eq!(entry.file.file_name, r"Caf\xE9.flac");
    assert_eq!(entry.file.path, track);
}

/// A name spelled `Caf\xE9.flac` and a Latin-1 `Café.flac` show the same, and
/// a release holds each name once: the folder is skipped, naming it.
#[cfg(target_os = "linux")]
#[test]
fn two_files_showing_one_name_skip_their_folder() {
    let (tmp, album) = album_dir();
    std::fs::write(album.join(r"Caf\xE9.flac"), fake_flac()).unwrap();
    std::fs::write(album.join(latin1_name("flac")), fake_flac()).unwrap();

    let items = scan_items(tmp.path());

    let [ScanItem::Invalid(invalid)] = items.as_slice() else {
        panic!("one skipped folder: {items:?}");
    };
    assert_eq!(
        invalid.reason(),
        InvalidReason::SameName {
            name: r"Caf\xE9.flac".to_string()
        }
    );
    let mut paths: Vec<PathBuf> = invalid
        .files
        .iter()
        .map(|file| file.file.path.clone())
        .collect();
    paths.sort();
    let mut expected = [album.join(r"Caf\xE9.flac"), album.join(latin1_name("flac"))];
    expected.sort();
    assert_eq!(paths, expected);
    assert!(invalid
        .files
        .iter()
        .all(|file| file.relative_path == r"Caf\xE9.flac" && file.role == ProposedRole::Audio));
}

/// Two of a folder's own files showing one name leave them unimportable, the
/// way a release holding them would be.
#[cfg(target_os = "linux")]
#[test]
fn two_files_showing_one_name_make_the_sidecar_invalid() {
    let (_temp, root, album) = album_of_discs(&["Disc 1", "Disc 2"]);
    std::fs::write(album.join(r"Caf\xE9.txt"), "one").unwrap();
    std::fs::write(album.join(latin1_name("txt")), "two").unwrap();

    let found = sidecars(
        &root,
        readings(&[(
            "Artist/Album",
            FolderReleaseDecision::KeepAsSeparateReleases,
            FolderReleaseDecisionAuthor::User,
        )]),
    );

    let [sidecar] = found.as_slice() else {
        panic!("one sidecar: {found:?}");
    };
    let SidecarFiles::Invalid(files) = &sidecar.files else {
        panic!(
            "two files showing one name leave the files unimportable: {:?}",
            sidecar.files
        );
    };
    assert_eq!(
        InvalidReason::of(files),
        InvalidReason::SameName {
            name: r"Caf\xE9.txt".to_string()
        }
    );
}
