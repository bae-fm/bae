use super::*;

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

/// One track's file named early in the first read and the next track's
/// named late in it are one file at a time: only the file both reads
/// name counts.
#[test]
fn a_file_counts_only_when_both_reads_name_it() {
    assert_eq!(
        named_by_both(paths(&["01.flac", "02.flac"]), paths(&["02.flac"])),
        paths(&["02.flac"])
    );
    assert_eq!(named_by_both(paths(&["01.flac"]), paths(&["02.flac"])), paths(&[]));
}

/// A file open through two descriptors in both reads counts twice; one
/// the second read names once counts once.
#[test]
fn a_file_counts_as_often_as_both_reads_name_it() {
    assert_eq!(
        named_by_both(paths(&["image.flac", "image.flac"]), paths(&["image.flac", "image.flac"])),
        paths(&["image.flac", "image.flac"])
    );
    assert_eq!(
        named_by_both(paths(&["image.flac", "image.flac"]), paths(&["image.flac"])),
        paths(&["image.flac"])
    );
}

#[test]
fn a_file_held_open_is_open_together_until_it_closes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a");
    std::fs::write(&path, b"a").unwrap();
    let path = path.canonicalize().unwrap();

    let file = std::fs::File::open(&path).unwrap();
    assert_eq!(open_together(dir.path()), vec![path]);
    drop(file);
    assert!(open_together(dir.path()).is_empty());
}
