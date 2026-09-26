//! Where a value read off a candidate's folder was read.

use bae_mirror::mirror_enum;

/// Where a value was read. Mirrors `bae_core::signals::SignalOrigin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalOrigin {
    DiscToc,
    CueSheet,
    Artwork,
    ArtworkBarcode,
    FolderName,
    Filename,
    TextFile,
}

mirror_enum! {
    BridgeSignalOrigin = bae_core::signals::SignalOrigin,
    from_core: pub(crate) fn,
    variants: { DiscToc, CueSheet, Artwork, ArtworkBarcode, FolderName, Filename, TextFile },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSignalOrigin = bae_core::signals::SignalOrigin,
    into_core: pub(crate) fn,
    variants: { DiscToc, CueSheet, Artwork, ArtworkBarcode, FolderName, Filename, TextFile },
}
