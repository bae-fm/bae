//! The `ContentType` and role each release file is stored with, decided by
//! the role the scan read it as.

use crate::ImportError;
use bae_records::ReleaseFileRole;
use bae_util::content_type::ContentType;
use bae_util::content_type_hint::ContentTypeHint;

/// The `ContentType` a release file is stored with, decided by the role the
/// scan read it as.
///
/// Audio reuses the scan's probe facts: only the decoder knows which codec the
/// container holds — a `.m4a` could be ALAC or AAC, and the extension can't
/// say. A damaged image is plain bytes: its extension names a picture its
/// bytes are not, and the library reads anything stored as an image as one.
/// Every other file takes its `ContentType` from the extension hint, an honest
/// mapping for images (an extension on image bytes does predict the codec),
/// text, and PDF. Anything the hint can't classify becomes `OctetStream` and
/// flows through the DB as-is — including a file with no extension at all,
/// which a rip folder can legitimately hold (a `README`, a `.`-less checksum
/// file) and which the release carries like any other file.
pub(super) fn resolve_file_content_type(
    entry: &crate::folder_scanner::CandidateFile,
) -> Result<ContentType, ImportError> {
    use crate::folder_scanner::FileRole;

    let path = &entry.file.version.path;
    match entry.role {
        FileRole::Audio => {
            return entry
                .file
                .source_audio
                .as_ref()
                .map(|audio| audio.content_type.clone())
                .ok_or_else(|| ImportError::UnusableFile {
                    detail: format!("{} has no scanned audio facts", path.display()),
                })
        }
        FileRole::DamagedImage => return Ok(ContentType::OctetStream),
        FileRole::TrackSheet { .. }
        | FileRole::Artwork
        | FileRole::Document { .. }
        | FileRole::Other => {}
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return Ok(ContentType::OctetStream);
    };
    let hint = ContentTypeHint::from_extension(ext);

    if hint.is_audio() {
        return Err(ImportError::UnusableFile {
            detail: format!("{} is named as audio the scan did not read as audio", path.display()),
        });
    }

    if let Some(image) = hint.image_content_type() {
        return Ok(image);
    }
    Ok(match hint {
        ContentTypeHint::PlainText => ContentType::PlainText,
        ContentTypeHint::Pdf => ContentType::Pdf,
        // Audio hints were handled above; anything else is unknown binary.
        _ => ContentType::OctetStream,
    })
}

/// The role a release file is stored with: the scan's, less the parse,
/// binding and disc a track sheet's role holds, which the import has made
/// into the release's tracks.
pub(super) fn release_file_role(role: &crate::folder_scanner::FileRole) -> ReleaseFileRole {
    use crate::folder_scanner::FileRole;
    match role {
        FileRole::Audio => ReleaseFileRole::Audio,
        FileRole::TrackSheet { .. } => ReleaseFileRole::TrackSheet,
        FileRole::Artwork => ReleaseFileRole::Artwork,
        FileRole::DamagedImage => ReleaseFileRole::DamagedImage,
        FileRole::Document { kind } => ReleaseFileRole::Document(*kind),
        FileRole::Other => ReleaseFileRole::Other,
    }
}

#[cfg(test)]
#[path = "content_type_tests.rs"]
mod tests;
