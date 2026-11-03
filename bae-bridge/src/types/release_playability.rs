//! Whether this device can play a release, and the line a release this
//! device can't play shows.

use super::{BridgeFormerDeviceState, BridgeMessage, BridgeMessageArg};

/// Whether this device can play a release now, and why not. Mirrors
/// `bae_core::album_detail::ReleasePlayability`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleasePlayability {
    Playable,
    OnOtherDevice { device: BridgeOtherDevice },
}

mirror_enum! {
    BridgeReleasePlayability = bae_core::album_detail::ReleasePlayability,
    from_core: pub fn,
    variants: {
        Playable,
        OnOtherDevice(device: (BridgeOtherDevice)),
    },
}

/// The other device a release's audio is on. Mirrors
/// `bae_core::db::OtherDevice`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOtherDevice {
    Active { name: String },
    Former { name: String, state: BridgeFormerDeviceState },
}

mirror_enum! {
    BridgeOtherDevice = bae_core::db::OtherDevice,
    from_core: pub fn,
    variants: {
        Active { name },
        Former { name, state: (BridgeFormerDeviceState) },
    },
}

/// The line a release shows when this device can't play it, or `None` when
/// it can. A release on another device names that device ("On Dima's Mac"),
/// and says so when the device was removed from the library or replaced.
#[uniffi::export]
pub fn bridge_release_playability_line(
    playability: &BridgeReleasePlayability,
) -> Option<BridgeMessage> {
    let message =
        |key: &str, args: Vec<BridgeMessageArg>| BridgeMessage { key: key.to_string(), args };
    match playability {
        BridgeReleasePlayability::Playable => None,
        BridgeReleasePlayability::OnOtherDevice { device } => Some(match device {
            BridgeOtherDevice::Active { name } => message(
                "core.release.on_device",
                vec![BridgeMessageArg::new("device", name.clone())],
            ),
            BridgeOtherDevice::Former { name, state } => message(
                match state {
                    BridgeFormerDeviceState::Removed => "core.release.on_removed_device",
                    BridgeFormerDeviceState::Replaced => "core.release.on_replaced_device",
                },
                vec![BridgeMessageArg::new("device", name.clone())],
            ),
        }),
    }
}
