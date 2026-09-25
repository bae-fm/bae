/// Which of the two grouping actions a release offers, if either: to be read
/// together with others as one, or to be read as the folders it is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeGroupingAction {
    Combine,
    Separate,
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeGroupingAction = bae_core::import::grouping::GroupingAction,
    from_core: pub(crate) fn,
    variants: { Combine, Separate },
}

/// One folder of a release read from several, in play order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleasePart {
    pub folder_path: String,
    pub name: String,
}

#[cfg(feature = "desktop")]
impl BridgeReleasePart {
    pub(crate) fn from_core(part: &bae_core::import::ReleasePart) -> Self {
        Self {
            folder_path: part.folder.to_string_lossy().into_owned(),
            name: part.name(),
        }
    }
}
