//! Where a value read off a candidate's folder was read.

use bae_mirror::mirror_enum;

/// The surface a line of text was read off. Mirrors
/// `bae_core::signals::TextOrigin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTextOrigin {
    CueSheet,
    Artwork,
    FolderName,
    Filename,
    TextFile,
}

/// Where a barcode or catalog number was read. Mirrors
/// `bae_core::signals::SignalOrigin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalOrigin {
    /// Read out of a line of text on this surface.
    Text { origin: BridgeTextOrigin },
    /// A barcode the detector decoded from the bars on a cover image.
    ArtworkBarcode,
}

mirror_enum! {
    BridgeTextOrigin = bae_core::signals::TextOrigin,
    from_core: pub(crate) fn,
    variants: { CueSheet, Artwork, FolderName, Filename, TextFile },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeTextOrigin = bae_core::signals::TextOrigin,
    into_core: pub(crate) fn,
    variants: { CueSheet, Artwork, FolderName, Filename, TextFile },
}

mirror_enum! {
    BridgeSignalOrigin = bae_core::signals::SignalOrigin,
    from_core: pub(crate) fn,
    variants: { Text(origin: (BridgeTextOrigin)), ArtworkBarcode },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSignalOrigin = bae_core::signals::SignalOrigin,
    into_core: pub(crate) fn,
    variants: { Text(origin: (BridgeTextOrigin)), ArtworkBarcode },
}
