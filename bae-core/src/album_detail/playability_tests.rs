use super::*;
use DbUnattachedAudio::*;

#[test]
fn an_attached_release_plays_on_any_device() {
    assert_eq!(release_playability(&[]), ReleasePlayability::Playable);
}

#[test]
fn a_local_release_plays_where_its_originals_are() {
    assert_eq!(
        release_playability(&[Unchanged, Unchanged]),
        ReleasePlayability::Playable
    );
}

#[test]
fn a_local_release_from_another_device_is_not_uploaded_yet() {
    assert_eq!(
        release_playability(&[Elsewhere]),
        ReleasePlayability::NotUploaded
    );
}

#[test]
fn a_missing_or_changed_original_is_reported_as_missing() {
    assert_eq!(
        release_playability(&[Unchanged, Changed]),
        ReleasePlayability::OriginalMissing
    );
}

#[test]
fn another_device_outranks_a_changed_original() {
    assert_eq!(
        release_playability(&[Changed, Elsewhere]),
        ReleasePlayability::NotUploaded
    );
}
