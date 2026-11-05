/// One folder of a release read from several, in play order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleasePart {
    /// The folder, as coven's bytes for its path.
    pub folder_path: Vec<u8>,
    pub name: String,
}

#[cfg(desktop)]
impl BridgeReleasePart {
    pub(crate) fn from_core(part: &bae_import::ReleasePart) -> Self {
        Self { folder_path: coven::path_bytes(&part.folder), name: part.name() }
    }
}
