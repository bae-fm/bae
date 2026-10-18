//! Whether this device can play a release now, decided from what it holds.

use crate::db::{DbFile, DbLocalOriginal};

/// Whether this device can play a release now, and why not.
///
/// Decided from this device's own facts: an attached release is read through
/// coven (from this device's source, its cache, or storage), so it counts as
/// playable here; whether storage has its bytes yet is found when it plays,
/// which reports an upload still under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleasePlayability {
    /// Its audio is attached, or it's a local release whose originals are on
    /// this device and unchanged.
    Playable,
    /// A local release imported on another device: its audio stays there
    /// until that device uploads it.
    NotUploaded,
    /// A local release imported on this device whose original files are gone
    /// or changed since import.
    OriginalMissing,
}

/// The playability of a release with these files, given the originals this
/// device holds for them. Only audio files decide it; a release on another
/// device is reported before a missing original, since only uploading it
/// makes it playable here.
pub(crate) fn release_playability(
    files: &[DbFile],
    originals: &[DbLocalOriginal],
) -> ReleasePlayability {
    let mut original_missing = false;
    for file in files
        .iter()
        .filter(|file| file.content_type.is_audio() && !file.attached)
    {
        match originals
            .iter()
            .find(|original| original.file_id == file.id)
        {
            None => return ReleasePlayability::NotUploaded,
            Some(original) if !original.readable => original_missing = true,
            Some(_) => {}
        }
    }
    if original_missing {
        ReleasePlayability::OriginalMissing
    } else {
        ReleasePlayability::Playable
    }
}

#[cfg(test)]
#[path = "playability_tests.rs"]
mod tests;
