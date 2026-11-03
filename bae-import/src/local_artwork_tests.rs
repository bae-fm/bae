use super::*;
use std::path::PathBuf;

fn artwork(relative_path: &str, size: u64) -> ScannedFile {
    ScannedFile::new(
        PathBuf::from("/candidate").join(relative_path),
        relative_path.to_string(),
        size,
        1,
    )
}

fn embedded() -> Option<crate::CoverSelection> {
    Some(crate::CoverSelection::Embedded("01 Track.mp3".to_string()))
}

fn local(path: &str) -> Option<crate::CoverSelection> {
    Some(crate::CoverSelection::Local(path.to_string()))
}

#[test]
fn a_front_cover_image_wins_over_embedded_artwork() {
    let files = [artwork("Front.jpg", 500), artwork("back.jpg", 1)];

    assert_eq!(folder_cover(embedded(), &files), local("Front.jpg"));
    assert_eq!(file_tags_cover(embedded(), &files), local("Front.jpg"));
}

#[test]
fn embedded_artwork_wins_over_an_image_not_named_as_the_front_cover() {
    let files = [artwork("back.jpg", 1), artwork("scan 1.jpg", 2)];

    assert_eq!(folder_cover(embedded(), &files), embedded());
    assert_eq!(file_tags_cover(embedded(), &files), embedded());
}

#[test]
fn without_embedded_artwork_the_folder_image_order_decides() {
    let files = [artwork("back.jpg", 1), artwork("scan 1.jpg", 2)];

    assert_eq!(folder_cover(None, &files), local("back.jpg"));
    assert_eq!(
        file_tags_cover(None, &files),
        None,
        "applying tags that embed nothing leaves the cover as it is"
    );
}

#[test]
fn conventional_folder_artwork_uses_case_insensitive_name_order() {
    let files = [artwork("z/Folder.png", 50), artwork("A/COVER.jpg", 500), artwork("front.jpg", 1)];

    assert_eq!(
        default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
        Some("A/COVER.jpg")
    );
}

#[test]
fn numbered_front_artwork_beats_the_smallest_image() {
    let files = [artwork("00Front.jpg", 500), artwork("disc.jpg", 1)];

    assert_eq!(
        default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
        Some("00Front.jpg")
    );
}

#[test]
fn conventional_artwork_order_ignores_numbering_prefixes() {
    let files = [artwork("00Front.jpg", 1), artwork("99 - cover.jpg", 500)];

    assert_eq!(
        default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
        Some("99 - cover.jpg")
    );
}

#[test]
fn descriptive_front_artwork_beats_the_smallest_image() {
    let files = [artwork("01 - Front scan.jpg", 500), artwork("disc.jpg", 1)];

    assert_eq!(
        default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
        Some("01 - Front scan.jpg")
    );
}

#[test]
fn back_artwork_does_not_rank_as_a_conventional_cover() {
    let files = [artwork("cover-back.jpg", 1), artwork("front scan.jpg", 500)];

    assert_eq!(
        default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
        Some("front scan.jpg")
    );
}

#[test]
fn remaining_folder_artwork_uses_size_then_name() {
    let files = [artwork("large.jpg", 500), artwork("b/scan.png", 20), artwork("A/scan.png", 20)];

    assert_eq!(
        default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
        Some("A/scan.png")
    );
}

#[test]
fn unsupported_cover_format_does_not_hide_supported_artwork() {
    for name in ["scan.gif", "scan.webp", "scan.png", "scan.jpg"] {
        let files = [artwork("cover.bmp", 1), artwork(name, 500)];
        assert_eq!(
            default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
            Some(name)
        );
    }
    assert!(default_local_cover_file(&[artwork("cover.bmp", 1)]).is_none());
}
