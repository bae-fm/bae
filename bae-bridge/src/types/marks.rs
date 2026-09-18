//! The names read off an object itself, and where each was read.

use bae_mirror::{mirror_enum, mirror_struct};

/// Which name a mark is. Mirrors `bae_core::import::MarkKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMarkKind {
    DiscId,
    Barcode,
    CatalogNumber,
}

mirror_enum! {
    BridgeMarkKind = bae_core::import::MarkKind,
    from_core: pub(crate) fn,
    variants: { DiscId, Barcode, CatalogNumber },
}

impl BridgeMarkKind {
    fn loc_key(self) -> &'static str {
        match self {
            Self::DiscId => "core.mark.kind.disc_id",
            Self::Barcode => "core.mark.kind.barcode",
            Self::CatalogNumber => "core.mark.kind.catalog_number",
        }
    }
}

/// Localization key for what a mark's line is labelled with — resolved by the
/// UI against the `Core` string table.
#[uniffi::export]
pub fn bridge_mark_kind_key(kind: BridgeMarkKind) -> String {
    kind.loc_key().to_string()
}

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

/// One name read off the object itself. Mirrors
/// `bae_core::import::ReleaseMarkLine`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseMark {
    pub kind: BridgeMarkKind,
    pub value: String,
    /// Every surface this value was read from, each named once and in the
    /// order it was first read from them. Core folds the sightings, so a line
    /// tags what it tags without any surface counting them.
    pub origins: Vec<BridgeSignalOrigin>,
    /// This value's lookup named the chosen release record.
    pub corroborated: bool,
}

mirror_struct! {
    BridgeReleaseMark = bae_core::import::ReleaseMarkLine,
    from_core: pub(crate) fn,
    fields: {
        kind: (BridgeMarkKind),
        value,
        origins: (each BridgeSignalOrigin),
        corroborated,
    },
}
