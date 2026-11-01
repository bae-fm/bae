/// Why this device stopped for good, ending a change it asked for before the
/// change was made. Nothing refused the change: another device may still
/// make it (§5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDeviceStop {
    /// This device, or its person, was removed from the library.
    Removed,
    /// A restore put another copy of bae in this device's place.
    Replaced,
    /// Another copy of this device's library syncs as this device.
    Copied,
    /// Another of the person's devices replaced their keys.
    MemberKeysReplaced,
}
mirror_enum! { BridgeDeviceStop = coven::DeviceStop,
    from_core: pub(crate) fn,
    variants: { Removed, Replaced, Copied, MemberKeysReplaced },
}
