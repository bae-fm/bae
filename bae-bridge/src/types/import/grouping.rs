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
