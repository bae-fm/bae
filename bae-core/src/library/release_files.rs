//! A release's files in the order a person sorts the Storage sheet's file
//! table by, read through one live subscription whose release and order
//! change in place.

/// What a release's files can be ordered by: their names in natural order,
/// their sizes, or their kinds (content types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseFileColumn {
    Name,
    Size,
    Kind,
}

/// How a list of a release's files is ordered: by `column`, its ties broken
/// by name in natural order and then by id, the whole order running backward
/// when `descending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseFileOrder {
    pub column: ReleaseFileColumn,
    pub descending: bool,
}

/// The release whose files to read, and their order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFilesRequest {
    pub release_id: String,
    pub order: ReleaseFileOrder,
}

/// What a read of a release's files delivered: the request it answered, and
/// the files in its order, or `None` once no such release exists. A read for
/// no request delivers neither.
#[derive(Debug, Clone)]
pub struct ReleaseFilesSnapshot {
    pub request: Option<ReleaseFilesRequest>,
    pub files: Option<Vec<crate::album_detail::FileDetail>>,
}

/// A live read of one release's files in one order. The release and order
/// change in place through [`LiveRead::set`](super::LiveRead::set); `None`
/// reads nothing.
pub type ReleaseFilesSubscription =
    super::LiveRead<Option<ReleaseFilesRequest>, ReleaseFilesSnapshot>;
