//! Whether this device can play a release now, decided from what the library
//! records of where its files are.

use bae_records::{DbUnstoredAudio, OtherDevice};

/// Whether this device can play a release now, and why not.
///
/// Decided from the library's records alone: a file storage holds is read
/// through coven (from its cache or storage), so it counts as playable here;
/// any other file plays from this device's original, while it uploads too.
/// The disk is not asked: an original gone or changed since import fails
/// the play, export or move that reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleasePlayability {
    /// Each audio file is in storage, or this device holds its original.
    Playable,
    /// Audio on another device only: it plays here once that device moves it
    /// to the cloud.
    OnOtherDevice(OtherDevice),
}

/// The playability of a release from what the library records this device
/// holding for each of its audio files on one device only: the device the
/// first file on another device is on, or playable.
pub(crate) fn release_playability(unstored_audio: &[DbUnstoredAudio]) -> ReleasePlayability {
    unstored_audio
        .iter()
        .find_map(|audio| match audio {
            DbUnstoredAudio::Elsewhere(device) => {
                Some(ReleasePlayability::OnOtherDevice(device.clone()))
            }
            DbUnstoredAudio::Original => None,
        })
        .unwrap_or(ReleasePlayability::Playable)
}

#[cfg(test)]
#[path = "playability_tests.rs"]
mod tests;
