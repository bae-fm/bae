//! The Storage sheet's read of a release's files in the order its file table
//! sorts by. The read itself is desktop-only (`handle::release_files`).

use super::BridgeFile;

/// What a release's files can be ordered by. Mirrors `ReleaseFileColumn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseFileColumn {
    Name,
    Size,
    Kind,
}

/// How a list of a release's files is ordered: by `column`, ties by name in
/// natural order and then by id, the whole order backward when
/// `descending`. Mirrors `ReleaseFileOrder`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseFileOrder {
    pub column: BridgeReleaseFileColumn,
    pub descending: bool,
}

/// The release whose files to read, and their order. Mirrors
/// `ReleaseFilesRequest`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseFilesRequest {
    pub release_id: String,
    pub order: BridgeReleaseFileOrder,
}

/// One value the read delivered: the request it answered, and the files in
/// its order, or none once no such release exists.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseFilesSnapshot {
    pub request: Option<BridgeReleaseFilesRequest>,
    pub files: Option<Vec<BridgeFile>>,
}
