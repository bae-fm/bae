use super::*;
use crate::file_tag_snapshot::{EmbeddedCoverFact, FileTagSnapshot};
use bae_util::content_type::ContentType;

fn snapshot(embedded_cover: Option<EmbeddedCoverFact>) -> FileTagSnapshot {
    FileTagSnapshot { scan_generation: 1, file_edit_revision: 0, files: Vec::new(), embedded_cover }
}

#[test]
fn embedded_artwork_becomes_the_file_tags_cover_selection() {
    let snapshot = snapshot(Some(EmbeddedCoverFact {
        source_relative_path: "01.flac".to_string(),
        content_type: ContentType::Jpeg,
        data: vec![1, 2, 3],
    }));

    assert_eq!(
        embedded_cover_selection(&snapshot),
        Some(PreparedCover::Embedded("01.flac".to_string()))
    );
}

#[test]
fn missing_embedded_artwork_stores_no_file_tags_cover_selection() {
    assert_eq!(embedded_cover_selection(&snapshot(None)), None);
}

#[test]
fn unsupported_snapshot_artwork_is_not_selected_when_applying_file_tags() {
    let snapshot = snapshot(Some(EmbeddedCoverFact {
        source_relative_path: "01.flac".to_string(),
        content_type: ContentType::Bmp,
        data: b"BM".to_vec(),
    }));
    assert_eq!(embedded_cover_selection(&snapshot), None);
    assert!(snapshot.embedded_cover.is_some(), "the stored observation is retained");
}
