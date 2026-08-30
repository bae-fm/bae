//! Source-neutral selection of artwork files from an import candidate.

use super::folder_scanner::{CategorizedFiles, ScannedFile};
use super::CoverSelection;
use crate::util::content_type_hint::ContentTypeHint;

/// The folder image a candidate uses when no metadata source supplies artwork.
/// This reads only scan facts: no audio file or tag is opened.
pub(crate) fn default_local_cover(files: &CategorizedFiles) -> Option<CoverSelection> {
    default_local_cover_file(files.files.iter().map(|entry| &entry.file))
        .map(|file| CoverSelection::Local(file.relative_path.clone()))
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
    match (conventional_artwork(left), conventional_artwork(right)) {
        (true, true) => left_name
            .cmp(&right_name)
            .then_with(|| left.relative_path.cmp(&right.relative_path)),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => left
            .size
            .cmp(&right.size)
            .then_with(|| left_name.cmp(&right_name))
            .then_with(|| left.relative_path.cmp(&right.relative_path)),
    }
}

fn conventional_artwork(file: &ScannedFile) -> bool {
    std::path::Path::new(&file.relative_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| {
            let stem = stem.trim_start_matches(|character: char| {
                character.is_ascii_digit() || matches!(character, ' ' | '-' | '_' | '.')
            });
            ["front", "cover", "folder"]
                .iter()
                .any(|name| stem.eq_ignore_ascii_case(name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::folder_scanner::{CandidateFile, FileRole};
    use std::path::PathBuf;

    fn artwork(relative_path: &str, size: u64) -> ScannedFile {
        ScannedFile::new(
            PathBuf::from("/candidate").join(relative_path),
            relative_path.to_string(),
            size,
            1,
            "0".repeat(64),
        )
    }

    fn files_with_artwork(artwork: impl IntoIterator<Item = ScannedFile>) -> CategorizedFiles {
        CategorizedFiles {
            files: artwork
                .into_iter()
                .map(|file| CandidateFile {
                    file,
                    role: FileRole::Artwork,
                    proposed_audio: false,
                })
                .collect(),
        }
    }

    #[test]
    fn conventional_folder_artwork_uses_case_insensitive_name_order() {
        let files = files_with_artwork([
            artwork("z/Folder.png", 50),
            artwork("A/COVER.jpg", 500),
            artwork("front.jpg", 1),
        ]);

        assert_eq!(
            default_local_cover(&files),
            Some(CoverSelection::Local("A/COVER.jpg".to_string()))
        );
    }

    #[test]
    fn numbered_front_artwork_beats_the_smallest_image() {
        let files = files_with_artwork([artwork("00Front.jpg", 500), artwork("disc.jpg", 1)]);

        assert_eq!(
            default_local_cover(&files),
            Some(CoverSelection::Local("00Front.jpg".to_string()))
        );
    }

    #[test]
    fn remaining_folder_artwork_uses_size_then_name() {
        let files = files_with_artwork([
            artwork("large.jpg", 500),
            artwork("b/scan.png", 20),
            artwork("A/scan.png", 20),
        ]);

        assert_eq!(
            default_local_cover(&files),
            Some(CoverSelection::Local("A/scan.png".to_string()))
        );
    }
}
