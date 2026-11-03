use super::*;

/// A temporary directory is on this machine's own disk. Weak as a test of
/// the network branch — there is no network volume to mount from a test —
/// but it holds the one answer a wrong `statfs` call would break: the
/// ordinary case must not read as network, or every local folder would
/// quietly lose its watch.
#[tokio::test]
async fn a_folder_on_this_machine_is_local() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        volume_kind(temp.path(), &bae_diagnostics::Diagnostics::noop()).await,
        VolumeKind::Local
    );
}

/// A path nothing is mounted at answers `Local`, so a root that has gone
/// away is watched exactly as it was rather than changing behaviour on its
/// way out.
#[tokio::test]
async fn a_path_that_is_not_there_is_local() {
    assert_eq!(
        volume_kind(
            std::path::Path::new("/nowhere-at-all-1a2b3c"),
            &bae_diagnostics::Diagnostics::noop()
        )
        .await,
        VolumeKind::Local
    );
}

/// The cheap check answers "nothing moved" only for a set it recorded and
/// still finds exactly as it left it, and otherwise names what moved. A directory that was written to, one
/// that is gone, and a root nothing was recorded for all read as changed —
/// each of those has to end in a walk, because none of them is evidence
/// that the folder is as it was.
#[test]
fn only_an_untouched_recorded_set_reads_as_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let album = temp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), b"x").unwrap();

    let recorded = |path: &std::path::Path| {
        vec![(
            path.to_path_buf(),
            directory_modified_at(path).expect("a directory that is there has an mtime"),
        )]
    };
    let untouched = recorded(&album);
    assert_eq!(changed_directories(&untouched), Some(Vec::new()));

    assert_eq!(changed_directories(&[]), None, "nothing recorded is not an answer");
    let gone = temp.path().join("Gone");
    assert_eq!(
        changed_directories(&[(gone.clone(), 0)]),
        Some(vec![gone]),
        "a directory that is gone is a change"
    );
    assert_eq!(
        changed_directories(&[(album.clone(), 1)]),
        Some(vec![album]),
        "an mtime that does not match is a change"
    );
}
