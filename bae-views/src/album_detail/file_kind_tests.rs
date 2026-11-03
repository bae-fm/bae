use super::*;

#[test]
fn a_file_kind_reads_its_content_type_and_a_rips_extensions() {
    assert_eq!(
        FileKind::of(&ContentType::Flac, "01 Track Title.flac"),
        FileKind::Audio { format: "FLAC".into() }
    );
    assert_eq!(
        FileKind::of(&ContentType::Jpeg, "front.jpg"),
        FileKind::Image { format: "JPEG".into() }
    );
    assert_eq!(FileKind::of(&ContentType::PlainText, "Album Title.cue"), FileKind::CueSheet);
    assert_eq!(FileKind::of(&ContentType::OctetStream, "Album Title.LOG"), FileKind::RipLog);
    assert_eq!(FileKind::of(&ContentType::PlainText, "notes.txt"), FileKind::Text);
    assert_eq!(FileKind::of(&ContentType::Pdf, "booklet.pdf"), FileKind::Pdf);
    assert_eq!(FileKind::of(&ContentType::OctetStream, "data.bin"), FileKind::Other);
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
