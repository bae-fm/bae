use super::*;

/// Bytes a row or the app hands over are a folder below a watched folder
/// only when each name between the `/`s is one ordinary name.
#[test]
fn bytes_that_are_not_names_below_a_root_are_refused() {
    for refused in [&b"a//b"[..], b"a/./b", b"a/../b", b"/a", b"a/"] {
        assert!(
            RelativeFolder::from_bytes(refused.to_vec()).is_err(),
            "{}",
            String::from_utf8_lossy(refused)
        );
    }
    assert!(RelativeFolder::from_bytes(Vec::new()).unwrap().is_root());
    assert_eq!(
        RelativeFolder::from_bytes(b"a/b".to_vec()).unwrap(),
        RelativeFolder::of(Path::new("a/b")).unwrap()
    );
}

/// A folder spelled from a scan's relative path goes back to the same
/// absolute folder, and shows as its names joined by `/`.
#[test]
fn a_folder_below_a_root_round_trips() {
    let root = host_root("/music");
    let folder = root.join("Artist").join("Album");
    let relative = folder_relative_below(&root, &folder).unwrap().unwrap();
    assert_eq!(relative.text(), "Artist/Album");
    assert_eq!(folder_below(&root, &relative).as_os_str(), folder.as_os_str());
    assert_eq!(folder_relative_below(&root, &root).unwrap(), Some(RelativeFolder::root()));
    let beside = host_root("/other");
    assert_eq!(folder_relative_below(&root, &beside).unwrap(), None);
}

/// A name that isn't valid UTF-8 keys its folder by its bytes and shows
/// escaped, as SQL's `coven_path_text` shows the same bytes.
#[cfg(target_os = "linux")]
#[test]
fn a_name_that_is_not_utf8_keeps_its_bytes() {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::OsStr::from_bytes(b"Caf\xE9");
    let relative = RelativeFolder::of(&Path::new("Artist").join(name)).unwrap();
    assert_eq!(relative.as_bytes(), b"Artist/Caf\xE9");
    assert_eq!(relative.text(), "Artist/Caf\\xE9");
    assert_eq!(RelativeFolder::from_bytes(relative.as_bytes().to_vec()).unwrap(), relative);
}

/// The range of paths below a folder holds what is inside it and nothing
/// beside it whose name merely starts the same.
#[cfg(desktop)]
#[test]
fn the_paths_below_a_folder_are_one_byte_range() {
    let folder = host_root("/music/Album");
    let (first, past) = paths_below(&folder);
    let inside = coven::path_bytes(&folder.join("Disc 1"));
    let beside = coven::path_bytes(&host_root("/music/Album 2"));
    assert!(first <= inside && inside < past);
    assert!(!(first <= beside && beside < past));
    assert!(!(first <= coven::path_bytes(&folder) && coven::path_bytes(&folder) < past));
}

/// The forward-slash spelling a `bae://import` link or `file://` drop gives
/// on Windows for `host_root("/music/rips")`.
#[cfg(windows)]
const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
#[cfg(not(windows))]
const URL_SPELLINGS: &[&str] = &[];

/// Every spelling a picker, drop, or link gives for one folder is accepted
/// and stored as the same string.
#[test]
fn a_root_has_one_stored_spelling_however_it_was_written() {
    let canonical = host_root("/music/rips");
    let spellings = [
        canonical.clone(),
        host_root("/music/rips/"),
        host_root("/music//rips"),
        host_root("/music/./rips"),
    ];

    for spelling in
        spellings.iter().map(PathBuf::as_path).chain(URL_SPELLINGS.iter().map(Path::new))
    {
        assert_eq!(
            canonical_absolute_root(spelling).unwrap().as_os_str(),
            canonical.as_os_str(),
            "{}",
            spelling.display()
        );
    }
}

/// On Windows a leading separator means the current drive, so the path
/// names a different folder depending on the process.
#[cfg(windows)]
#[test]
fn a_drive_relative_root_is_refused() {
    for rooted in ["/music/rips", r"\music\rips"] {
        let error = canonical_absolute_root(Path::new(rooted)).unwrap_err();
        assert!(error.to_string().contains("absolute"), "{rooted}: {error}");
    }
}

/// Network share and verbatim roots keep their prefix; only what follows
/// is rejoined.
#[cfg(windows)]
#[test]
fn unc_and_verbatim_roots_keep_their_prefix() {
    for (written, stored) in
        [(r"\\storage\share\Music\", r"\\storage\share\Music"), (r"\\?\C:\Music", r"\\?\C:\Music")]
    {
        assert_eq!(
            canonical_absolute_root(Path::new(written)).unwrap().as_os_str(),
            stored,
            "{written}"
        );
    }
}

/// Canonicalizing is stable, so [`validate_absolute_root`] accepts every
/// root this stores.
#[test]
fn canonicalizing_a_canonical_root_changes_nothing() {
    // A bare share root's canonical form keeps a trailing separator
    // (`\\storage\share\`); re-reading it must not change it.
    #[cfg(windows)]
    const SHARE_ROOTS: &[&str] = &[r"\\storage\share"];
    #[cfg(not(windows))]
    const SHARE_ROOTS: &[&str] = &[];

    let spellings =
        [host_root("/music/rips/"), host_root("/music//rips"), host_root("/music/./rips")];

    for written in spellings
        .iter()
        .map(PathBuf::as_path)
        .chain(URL_SPELLINGS.iter().map(Path::new))
        .chain(SHARE_ROOTS.iter().map(Path::new))
    {
        let stored = canonical_absolute_root(written).unwrap();
        assert_eq!(
            canonical_absolute_root(&stored).unwrap().as_os_str(),
            stored.as_os_str(),
            "{}",
            written.display()
        );
        validate_absolute_root(&stored).unwrap_or_else(|e| panic!("{}: {e}", written.display()));
    }
}

/// `..` is refused rather than resolved: resolving it without reading the
/// filesystem is wrong when a symlink is in the path.
#[test]
fn a_root_climbing_out_of_itself_is_refused() {
    let error = canonical_absolute_root(&host_root("/music/../rips")).unwrap_err();
    assert!(error.to_string().contains(".."), "{error}");
}

/// A stored root spelled with a trailing separator names the same
/// components as its canonical spelling, but not the same bytes, so the
/// rows keyed by it would miss: it is refused.
#[test]
fn a_stored_root_with_a_trailing_separator_is_refused() {
    let error = validate_absolute_root(&host_root("/music/rips/")).unwrap_err();
    assert!(error.to_string().contains("canonical"), "{error}");
}
