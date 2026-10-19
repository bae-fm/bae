use super::*;
use DbUnattachedAudio::*;

fn original(name: &str) -> RecordedOriginal {
    RecordedOriginal {
        path: std::path::PathBuf::from("/music/Album").join(name),
        size: 1_000,
        modified_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
    }
}

fn seen(answers: &[(&str, OriginalOnDisk)]) -> OriginalsSeen {
    answers
        .iter()
        .map(|(name, on_disk)| (original(name), *on_disk))
        .collect()
}

#[test]
fn an_attached_release_plays_on_any_device() {
    assert_eq!(
        release_playability(&[], &OriginalsSeen::default()),
        ReleasePlayability::Playable
    );
}

#[test]
fn a_local_release_plays_where_its_originals_are() {
    assert_eq!(
        release_playability(
            &[Original(original("1.flac")), Original(original("2.flac"))],
            &seen(&[
                ("1.flac", OriginalOnDisk::Unchanged),
                ("2.flac", OriginalOnDisk::Unchanged)
            ]),
        ),
        ReleasePlayability::Playable
    );
}

#[test]
fn a_local_release_from_another_device_is_not_uploaded_yet() {
    assert_eq!(
        release_playability(&[Elsewhere], &OriginalsSeen::default()),
        ReleasePlayability::NotUploaded
    );
}

#[test]
fn a_missing_or_changed_original_is_reported_as_missing() {
    assert_eq!(
        release_playability(
            &[Original(original("1.flac")), Original(original("2.flac"))],
            &seen(&[
                ("1.flac", OriginalOnDisk::Unchanged),
                ("2.flac", OriginalOnDisk::Changed)
            ]),
        ),
        ReleasePlayability::OriginalMissing
    );
}

#[test]
fn another_device_outranks_a_changed_original() {
    assert_eq!(
        release_playability(
            &[Original(original("1.flac")), Elsewhere],
            &seen(&[("1.flac", OriginalOnDisk::Changed)]),
        ),
        ReleasePlayability::NotUploaded
    );
}

/// An original the disk hasn't answered for is neither playable nor missing.
#[test]
fn an_original_the_disk_has_not_answered_for_is_being_checked() {
    assert_eq!(
        release_playability(
            &[Original(original("1.flac")), Original(original("2.flac"))],
            &seen(&[("1.flac", OriginalOnDisk::Unchanged)]),
        ),
        ReleasePlayability::CheckingOriginals
    );
}

/// What the disk has already said is reported before what it hasn't.
#[test]
fn a_changed_original_outranks_one_being_checked() {
    assert_eq!(
        release_playability(
            &[Original(original("1.flac")), Original(original("2.flac"))],
            &seen(&[("1.flac", OriginalOnDisk::Changed)]),
        ),
        ReleasePlayability::OriginalMissing
    );
}

/// An answer for a different recorded file — the same path recorded with
/// another size — says nothing about this one.
#[test]
fn an_answer_is_for_the_file_as_recorded() {
    let mut other = original("1.flac");
    other.size = 2_000;
    let answered: OriginalsSeen = [(other, OriginalOnDisk::Unchanged)].into_iter().collect();
    assert_eq!(
        release_playability(&[Original(original("1.flac"))], &answered),
        ReleasePlayability::CheckingOriginals
    );
}
