//! Source-neutral selection of artwork files from an import candidate.

use super::folder_scanner::ScannedFile;
use bae_util::content_type_hint::ContentTypeHint;

/// The cover a folder holds for itself, first of:
///
/// 1. a folder image named as the front cover (`cover`, `folder`, `front`;
///    see [`conventional_artwork_name`]) — the image the folder's owner put
///    there to be the cover;
/// 2. the artwork its own audio embeds — `embedded`, the selection the
///    file-tag snapshot names;
/// 3. any other folder image, by [`default_local_cover_file`]'s order.
///
/// `None` when the folder has none of them.
///
/// One rule, run wherever a candidate's cover is filled in: a scan, a reset,
/// the migration that fills what earlier scans left empty. A candidate's
/// cover is a stored value, so nothing derives one when a candidate is read.
#[cfg(desktop)]
pub(crate) fn folder_cover<'a>(
    embedded: Option<super::cover_art::PreparedCover>,
    artwork: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<super::cover_art::PreparedCover> {
    let artwork = artwork.into_iter().collect::<Vec<_>>();
    front_cover_file(artwork.iter().copied())
        .map(local_selection)
        .or(embedded)
        .or_else(|| default_local_cover_file(artwork).map(local_selection))
}

/// The cover applying a folder's file tags selects: the tags' embedded
/// artwork, unless the folder holds an image named as the front cover, which
/// [`folder_cover`] ranks ahead of it. `None` when the tags embed nothing, so
/// applying them leaves the candidate's cover as it is.
#[cfg(desktop)]
pub(crate) fn file_tags_cover<'a>(
    embedded: Option<super::cover_art::PreparedCover>,
    artwork: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<super::cover_art::PreparedCover> {
    embedded.map(|embedded| front_cover_file(artwork).map(local_selection).unwrap_or(embedded))
}

/// The folder image named as the front cover, by [`artwork_order`] among
/// several.
#[cfg(desktop)]
fn front_cover_file<'a>(
    files: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<&'a ScannedFile> {
    files
        .into_iter()
        .filter(|file| {
            ContentTypeHint::path_is_supported_cover(&file.path)
                && conventional_artwork_name(file).is_some()
        })
        .min_by(|left, right| artwork_order(left, right))
}

#[cfg(desktop)]
fn local_selection(file: &ScannedFile) -> super::cover_art::PreparedCover {
    super::cover_art::PreparedCover::Local(file.relative_path.clone())
}

/// Select the folder image used when no source supplies artwork. Every caller
/// shares this ordering; only the selected file's later use differs.
pub(crate) fn default_local_cover_file<'a>(
    files: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<&'a ScannedFile> {
    files
        .into_iter()
        .filter(|file| ContentTypeHint::path_is_supported_cover(&file.path))
        .min_by(|left, right| artwork_order(left, right))
}

fn artwork_order(left: &ScannedFile, right: &ScannedFile) -> std::cmp::Ordering {
    let left_name = left.relative_path.to_lowercase();
    let right_name = right.relative_path.to_lowercase();
    match (conventional_artwork_name(left), conventional_artwork_name(right)) {
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
    let mut words =
        stem.split(|character: char| !character.is_alphanumeric()).filter(|word| !word.is_empty());
    let first = words.next()?;
    let names_front_artwork = ["cover", "folder", "front"].contains(&first);
    let names_non_front_artwork = words
        .any(|word| ["back", "rear", "disc", "cd", "booklet", "inlay", "inside"].contains(&word));
    (names_front_artwork && !names_non_front_artwork).then_some(stem)
}

#[cfg(test)]
#[path = "local_artwork_tests.rs"]
mod tests;
