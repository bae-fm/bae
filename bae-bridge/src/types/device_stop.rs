/// Why this device stopped for good, ending a change it asked for before the
/// change was made. Nothing refused the change: another device may still
/// make it (§5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDeviceStop {
    /// This device, or its person, was removed from the library; or the
    /// library refused the admission of a joined device, which was never in
    /// it, and which shows as a removal does (coven §13.2).
    Removed,
    /// A restore put another copy of bae in this device's place.
    Replaced,
    /// Another copy of this device's library syncs as this device.
    Copied,
    /// Another of the person's devices replaced their keys.
    MemberKeysReplaced,
}
impl BridgeDeviceStop {
    pub(crate) fn from_core(stop: coven::DeviceStop) -> Self {
        match stop {
            coven::DeviceStop::Removed | coven::DeviceStop::NotAdmitted => Self::Removed,
            coven::DeviceStop::Replaced => Self::Replaced,
            coven::DeviceStop::Copied => Self::Copied,
            coven::DeviceStop::MemberKeysReplaced => Self::MemberKeysReplaced,
        }
    }
}
