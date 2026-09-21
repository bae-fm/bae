//! Where a value read off a candidate's folder was read.

use bae_mirror::mirror_enum;

/// Where a value was read. Mirrors `bae_core::signals::SignalOrigin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalOrigin {
    DiscToc,
    CueSheet,
    Artwork,
    FolderName,
    Filename,
    TextFile,
}

mirror_enum! {
    BridgeSignalOrigin = bae_core::signals::SignalOrigin,
    from_core: pub(crate) fn,
    variants: { DiscToc, CueSheet, Artwork, FolderName, Filename, TextFile },
}

impl BridgeSignalOrigin {
    fn loc_key(self) -> &'static str {
        match self {
            Self::DiscToc => "core.mark.origin.disc_toc",
            Self::CueSheet => "core.mark.origin.cue_sheet",
            Self::Artwork => "core.mark.origin.artwork",
            Self::FolderName => "core.mark.origin.folder_name",
            Self::Filename => "core.mark.origin.filename",
            Self::TextFile => "core.mark.origin.text_file",
        }
    }
}

/// Localization key for the short tag naming where a value was read —
/// resolved by the UI against the `Core` string table.
#[uniffi::export]
pub fn bridge_signal_origin_key(origin: BridgeSignalOrigin) -> String {
    origin.loc_key().to_string()
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSignalOrigin = bae_core::signals::SignalOrigin,
    into_core: pub(crate) fn,
    variants: { DiscToc, CueSheet, Artwork, FolderName, Filename, TextFile },
}
