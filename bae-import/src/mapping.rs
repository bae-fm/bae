//! The mapping table: every source unit the folder offers, alongside the track
//! committing makes of it.
//!
//! The draft holds one track per audio unit, in the folder's order, so row `i`
//! pairs unit `i` with draft track `i`. Surfaces render this projection without
//! joining separate lists.

use crate::folder_scanner::AudioDamage;
use crate::types::RawTrackEdit;
use crate::{SheetGroup, TrackTableSide};

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingTable {
    /// Every image the folder holds, in the scan's authoritative order.
    pub images: Vec<MappingImage>,
    /// The rows that can become release tracks, on the track table's sides,
    /// with a bound sheet heading the slices it carves.
    pub track_sides: Vec<TrackTableSide>,
    /// Files carried with the release but not represented by track rows.
    pub files: Vec<MappingFileRow>,
    /// How many track rows nobody has named yet. Committing anyway titles
    /// each after its file.
    pub untitled_track_count: u32,
}

/// One row in the files section of the mapping table.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingFileRow {
    File(MappingFile),
    /// A sheet that currently carves no track rows and can be assigned audio.
    Sheet(SheetGroup),
}

/// One of the folder's images, as the gallery shows it.
///
/// Whether an image is the release's cover is not a property of the image.
/// bae picks the cover from the stored cover row first, then from the picked
/// release's own art, then from the folder's images in name order. The
/// gallery lists every image the folder has, and the card shows the cover
/// that the person or bae picked.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingImage {
    /// The file's identity within the release (its relative path).
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    /// The version the scan read — what a thumbnail and the lightbox draw.
    pub file: crate::folder_scanner::FileVersion,
}

/// What one of a release's files is, as a row of the mapping table or of a
/// library release's files.
///
/// Narrower than the role the scan proposes: a folder's track sheet is not a
/// row here — it heads a group of rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MappingRole {
    /// Playable audio.
    Audio,
    /// Readable evidence: a rip log, a tracklist, a playlist, a CUE that
    /// could not be parsed as a sheet, or a library release's track sheet,
    /// which reads as the text it is.
    Document,
    /// An image the release carries, listed with its other files; a folder's
    /// images are also the gallery above its tracks.
    Image,
    /// Named as an image, but its bytes do not decode as one: carried with
    /// the release, and kept out of the gallery the cover is picked from.
    DamagedImage,
    /// Named as audio, and bae can't play it. Only a folder that failed
    /// validation holds one — it is why the folder failed.
    DamagedAudio { damage: AudioDamage },
    /// In the folder and carried with the release, unrecognized.
    Other,
}

/// A file of a release as its Files list shows it: a file of the folder, or
/// of a library release.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MappingFile {
    /// The id the file is read by: in a folder, its relative path, which the
    /// sheet bindings take; in the library, its release file's id.
    pub file_id: String,
    /// The file's identity within the release — its relative path, the same
    /// name the storage manager lists it under. Every file lists flat; a
    /// directory shows up only as the prefix its files carry.
    pub name: String,
    pub size: u64,
    /// The whole-file audition target of an audio file of a folder.
    #[serde(with = "stored_preview_target")]
    pub preview_target: Option<bae_track_audio::PreviewTarget>,
    pub role: MappingRole,
}

/// A file's audition target as JSON: the file's path and the samples the
/// target spans. A target records no byte landings, so these three fields
/// hold all of it.
mod stored_preview_target {
    use bae_track_audio::PreviewTarget;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Stored {
        path: std::path::PathBuf,
        start_sample: u64,
        end_sample: Option<u64>,
    }

    pub(super) fn serialize<S: serde::Serializer>(
        target: &Option<PreviewTarget>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        target
            .as_ref()
            .map(|target| Stored {
                path: target.path().to_path_buf(),
                start_sample: target.span().start_sample,
                end_sample: target.span().end_sample,
            })
            .serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<PreviewTarget>, D::Error> {
        Ok(Option::<Stored>::deserialize(deserializer)?.map(|stored| {
            PreviewTarget::sample_range(stored.path, stored.start_sample, stored.end_sample)
        }))
    }
}

/// The table's track rows in commit order — what the editor shapes into the
/// release it writes.
pub fn mapping_tracks(table: &MappingTable) -> Vec<RawTrackEdit> {
    table.track_sides.iter().flat_map(TrackTableSide::rows).map(|row| row.track.clone()).collect()
}

/// A folder that failed validation, as the pane looking into it reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidCandidateDetail {
    pub candidate: crate::triage::InvalidRow,
    /// The images that decode, in name order, as the gallery shows them.
    pub images: Vec<MappingImage>,
    /// Every other file, in name order, each with what it is — the damaged
    /// audio among them is why the folder failed.
    pub files: Vec<MappingFile>,
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
pub(crate) mod tests;
