use super::*;
use crate::util::content_type::ContentType;

fn file(id: &str, content_type: ContentType, attached: bool) -> DbFile {
    let mut file = DbFile::new(
        "release-id",
        "Track.flac",
        10,
        content_type,
        id.to_string(),
        chrono::Utc::now(),
    );
    file.attached = attached;
    file
}

fn original(file_id: &str, readable: bool) -> DbLocalOriginal {
    DbLocalOriginal {
        file_id: file_id.to_string(),
        readable,
    }
}

#[test]
fn an_attached_release_plays_on_any_device() {
    let files = [file("a", ContentType::Flac, true)];
    assert_eq!(
        release_playability(&files, &[]),
        ReleasePlayability::Playable
    );
}

#[test]
fn a_local_release_plays_where_its_originals_are() {
    let files = [
        file("a", ContentType::Flac, false),
        file("cover", ContentType::Jpeg, false),
    ];
    assert_eq!(
        release_playability(&files, &[original("a", true)]),
        ReleasePlayability::Playable
    );
}

#[test]
fn a_local_release_from_another_device_is_not_uploaded_yet() {
    let files = [file("a", ContentType::Flac, false)];
    assert_eq!(
        release_playability(&files, &[]),
        ReleasePlayability::NotUploaded
    );
}

#[test]
fn a_missing_or_changed_original_is_reported_as_missing() {
    let files = [
        file("a", ContentType::Flac, false),
        file("b", ContentType::Flac, false),
    ];
    assert_eq!(
        release_playability(&files, &[original("a", true), original("b", false)]),
        ReleasePlayability::OriginalMissing
    );
}
