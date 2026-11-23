//! Whether this device can play a release now, decided from what the library
//! records of where its files are.

use bae_records::OtherDevice;

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

impl ReleasePlayability {
    /// Whether this device can play the release.
    pub(crate) fn plays_here(&self) -> bool {
        matches!(self, Self::Playable)
    }

    /// Returns `OnOtherDevice` when `elsewhere` names a device and `Playable`
    /// when it is `None`.
    pub(crate) fn of(elsewhere: Option<OtherDevice>) -> Self {
        elsewhere.map_or(Self::Playable, Self::OnOtherDevice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_with_no_audio_elsewhere_plays_here() {
        assert_eq!(ReleasePlayability::of(None), ReleasePlayability::Playable);
    }

    #[test]
    fn a_release_with_audio_elsewhere_names_that_device() {
        let device = OtherDevice::Active { name: "Device A".to_string() };
        assert_eq!(
            ReleasePlayability::of(Some(device.clone())),
            ReleasePlayability::OnOtherDevice(device)
        );
    }
}
