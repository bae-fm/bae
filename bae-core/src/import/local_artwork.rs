//! Source-neutral selection of artwork files from an import candidate.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use super::folder_scanner::CategorizedFiles;
use super::folder_scanner::ScannedFile;
use crate::util::content_type_hint::ContentTypeHint;

/// The folder fallback in the same complete form the detail pane consumes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn default_local_cover_choice(files: &CategorizedFiles) -> Option<super::CoverChoice> {
    default_local_cover_file(files.artwork())
        .map(|image| super::CoverChoice::local(image.relative_path.clone(), image.path.clone()))
}

/// Select the folder image used when no source supplies artwork. Every caller
/// shares this ordering; only the selected file's later use differs.
pub(crate) fn default_local_cover_file<'a>(
    files: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<&'a ScannedFile> {
    files
        .into_iter()
        .filter(|file| ContentTypeHint::path_is_raster_image(&file.path))
        .min_by(|left, right| artwork_order(left, right))
}

fn artwork_order(left: &ScannedFile, right: &ScannedFile) -> std::cmp::Ordering {
    let left_name = left.relative_path.to_lowercase();
    let right_name = right.relative_path.to_lowercase();
    match (
        conventional_artwork_name(left),
        conventional_artwork_name(right),
    ) {
        (Some(left_artwork), Some(right_artwork)) => left_artwork
            .cmp(&right_artwork)
            .then_with(|| left.size.cmp(&right.size))
            .then_with(|| left.relative_path.cmp(&right.relative_path)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left
            .size
            .cmp(&right.size)
            .then_with(|| left_name.cmp(&right_name))
            .then_with(|| left.relative_path.cmp(&right.relative_path)),
    }
}

fn conventional_artwork_name(file: &ScannedFile) -> Option<String> {
    let stem = std::path::Path::new(&file.relative_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| {
            stem.trim_start_matches(|character: char| {
                character.is_ascii_digit() || !character.is_alphanumeric()
            })
            .to_lowercase()
        })?;
    let mut words = stem
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty());
    let first = words.next()?;
    let names_front_artwork = ["cover", "folder", "front"].contains(&first);
    let names_non_front_artwork = words
        .any(|word| ["back", "rear", "disc", "cd", "booklet", "inlay", "inside"].contains(&word));
    (names_front_artwork && !names_non_front_artwork).then_some(stem)
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn conventional_folder_artwork_uses_case_insensitive_name_order() {
        let files = [
            artwork("z/Folder.png", 50),
            artwork("A/COVER.jpg", 500),
            artwork("front.jpg", 1),
        ];

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
        let files = [
            artwork("large.jpg", 500),
            artwork("b/scan.png", 20),
            artwork("A/scan.png", 20),
        ];

        assert_eq!(
            default_local_cover_file(&files).map(|file| file.relative_path.as_str()),
            Some("A/scan.png")
        );
    }
}
