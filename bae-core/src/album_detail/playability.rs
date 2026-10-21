//! Whether this device can play a release now, decided from what the library
//! records it holding and what its disk has said about those originals.

use crate::db::{DbUnstoredAudio, RecordedOriginal};
use std::collections::HashMap;

/// Whether this device can play a release now, and why not.
///
/// Decided from this device's own facts: a file storage holds is read through
/// coven (from its cache or storage), so it counts as playable here; any other
/// file plays from this device's original, while it uploads too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleasePlayability {
    /// Each audio file is in storage, or its original is on this device and
    /// unchanged.
    Playable,
    /// Audio another device holds: it stays there until that device's upload
    /// of it completes.
    NotUploaded,
    /// Audio whose originals on this device are gone or changed since import.
    OriginalMissing,
    /// Audio whose originals on this device the disk has not answered for
    /// yet.
    CheckingOriginals,
}

/// What this device's disk said about one recorded original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginalOnDisk {
    /// The file recorded at import: the same size and modification time.
    Unchanged,
    /// Gone, unreadable, or changed since import.
    Changed,
}

/// What this device's disk said about each original it was asked about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OriginalsSeen(HashMap<RecordedOriginal, OriginalOnDisk>);

impl OriginalsSeen {
    /// Take in a newer answer for each original `newer` covers.
    pub(crate) fn extend(&mut self, newer: OriginalsSeen) {
        self.0.extend(newer.0);
    }

    /// Forget every original `kept` doesn't name.
    pub(crate) fn retain(&mut self, kept: &[RecordedOriginal]) {
        self.0.retain(|original, _| kept.contains(original));
    }
}

impl FromIterator<(RecordedOriginal, OriginalOnDisk)> for OriginalsSeen {
    fn from_iter<I: IntoIterator<Item = (RecordedOriginal, OriginalOnDisk)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// The originals the library records this device holding among
/// `unstored_audio`: what the disk is asked about.
pub(crate) fn recorded_originals<'a>(
    unstored_audio: impl IntoIterator<Item = &'a DbUnstoredAudio>,
) -> Vec<RecordedOriginal> {
    unstored_audio
        .into_iter()
        .filter_map(|audio| match audio {
            DbUnstoredAudio::Original(original) => Some(original.clone()),
            DbUnstoredAudio::Elsewhere => None,
        })
        .collect()
}

/// The playability of a release from what the library records this device
/// holding for each of its audio files not attached to its row, and what the
/// disk has said of those originals. A release on another device is reported
/// before a changed original, since only uploading it makes it playable here;
/// a changed original is reported before one the disk hasn't answered for.
pub(crate) fn release_playability(
    unstored_audio: &[DbUnstoredAudio],
    seen: &OriginalsSeen,
) -> ReleasePlayability {
    let mut changed = false;
    let mut unanswered = false;
    for audio in unstored_audio {
        match audio {
            DbUnstoredAudio::Elsewhere => return ReleasePlayability::NotUploaded,
            DbUnstoredAudio::Original(original) => match seen.0.get(original) {
                Some(OriginalOnDisk::Unchanged) => {}
                Some(OriginalOnDisk::Changed) => changed = true,
                None => unanswered = true,
            },
        }
    }
    if changed {
        ReleasePlayability::OriginalMissing
    } else if unanswered {
        ReleasePlayability::CheckingOriginals
    } else {
        ReleasePlayability::Playable
    }
}

#[cfg(test)]
#[path = "playability_tests.rs"]
mod tests;
