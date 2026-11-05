//! How a device lost access to the library.

/// How a device lost access to the library, as coven records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormerDeviceState {
    Removed,
    Replaced,
}
