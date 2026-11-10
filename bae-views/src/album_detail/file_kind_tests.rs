use super::*;

/// A file's kind is the role and document kind its import stored, whatever
/// its name says, and an audio file's or an image's format, and a PDF, its
/// content type.
#[test]
fn a_file_kind_reads_its_stored_role_and_its_content_type() {
    use bae_records::{DocumentKind, ReleaseFileRole};
    assert_eq!(
        FileKind::of(ReleaseFileRole::Audio, &ContentType::Flac),
        FileKind::Audio { format: "FLAC".into() }
    );
    assert_eq!(
        FileKind::of(ReleaseFileRole::Artwork, &ContentType::Jpeg),
        FileKind::Image { format: "JPEG".into() }
    );
    assert_eq!(
        FileKind::of(ReleaseFileRole::DamagedImage, &ContentType::OctetStream),
        FileKind::DamagedImage
    );
    assert_eq!(
        FileKind::of(ReleaseFileRole::TrackSheet, &ContentType::PlainText),
        FileKind::CueSheet
    );
    for (kind, file_kind) in [
        (DocumentKind::CueSheet, FileKind::CueSheet),
        (DocumentKind::RipLog, FileKind::RipLog),
        (DocumentKind::AccurateRipReport, FileKind::AccurateRipReport),
        (DocumentKind::Playlist, FileKind::Playlist),
        (DocumentKind::Text, FileKind::Text),
    ] {
        assert_eq!(
            FileKind::of(ReleaseFileRole::Document(kind), &ContentType::PlainText),
            file_kind
        );
    }
    assert_eq!(FileKind::of(ReleaseFileRole::Other, &ContentType::Pdf), FileKind::Pdf);
    assert_eq!(
        FileKind::of(ReleaseFileRole::Other, &ContentType::Svg),
        FileKind::Image { format: ContentType::Svg.display_name().into() }
    );
    assert_eq!(FileKind::of(ReleaseFileRole::Other, &ContentType::OctetStream), FileKind::Other);
}

#[test]
fn a_file_place_splits_its_folders_from_its_name() {
    assert_eq!(
        FilePlace::of("Disc 1/07 Track Title.flac"),
        FilePlace { name: "07 Track Title.flac".into(), folder: Some("Disc 1".into()) }
    );
    assert_eq!(FilePlace::of("Scans/Booklet/page 1.jpg").folder.as_deref(), Some("Scans/Booklet"));
    assert_eq!(FilePlace::of("cover.jpg"), FilePlace { name: "cover.jpg".into(), folder: None });
}
