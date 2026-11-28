//! A release's files in the order a person sorts the Storage sheet's file
//! table by, read through one live subscription whose release and order
//! change in place.

use super::*;
use bae_records::records::file_fragments::ReleaseFileRecord;

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

// A release's files in each order the Storage sheet sorts them by, one
// statement for each: by name in natural order, by size or by kind, ties going
// by name and then id, the whole order running backward in the `Backward` one.
// Each walks the index of the release's files its order names; sizes are
// coven's, so the size orders sort the release's files. A release with no
// files gives one row with no file, and a release the library does not hold
// gives none.
coven::read!(
    writer ReleaseFilesByName(release: String),
    r#"
    SELECT held.*, file.*
    FROM @bae_records::records::fragments::ReleaseAlbum AS held
    LEFT JOIN @bae_records::records::file_fragments::ReleaseFileRecord AS file
           ON file.release_id = :release
    WHERE held.release_id = :release
    ORDER BY file.original_filename COLLATE natural_order, file.id
"#
);

coven::read!(
    writer ReleaseFilesByNameBackward(release: String),
    r#"
    SELECT held.*, file.*
    FROM @bae_records::records::fragments::ReleaseAlbum AS held
    LEFT JOIN @bae_records::records::file_fragments::ReleaseFileRecord AS file
           ON file.release_id = :release
    WHERE held.release_id = :release
    ORDER BY file.original_filename COLLATE natural_order DESC, file.id DESC
"#
);

coven::read!(
    writer ReleaseFilesBySize(release: String),
    r#"
    SELECT held.*, file.*
    FROM @bae_records::records::fragments::ReleaseAlbum AS held
    LEFT JOIN @bae_records::records::file_fragments::ReleaseFileRecord AS file
           ON file.release_id = :release
    WHERE held.release_id = :release
    ORDER BY file.file_size, file.original_filename COLLATE natural_order, file.id
"#
);

coven::read!(
    writer ReleaseFilesBySizeBackward(release: String),
    r#"
    SELECT held.*, file.*
    FROM @bae_records::records::fragments::ReleaseAlbum AS held
    LEFT JOIN @bae_records::records::file_fragments::ReleaseFileRecord AS file
           ON file.release_id = :release
    WHERE held.release_id = :release
    ORDER BY file.file_size DESC, file.original_filename COLLATE natural_order DESC, file.id DESC
"#
);

coven::read!(
    writer ReleaseFilesByKind(release: String),
    r#"
    SELECT held.*, file.*
    FROM @bae_records::records::fragments::ReleaseAlbum AS held
    LEFT JOIN @bae_records::records::file_fragments::ReleaseFileRecord AS file
           ON file.release_id = :release
    WHERE held.release_id = :release
    ORDER BY file.content_type, file.original_filename COLLATE natural_order, file.id
"#
);

coven::read!(
    writer ReleaseFilesByKindBackward(release: String),
    r#"
    SELECT held.*, file.*
    FROM @bae_records::records::fragments::ReleaseAlbum AS held
    LEFT JOIN @bae_records::records::file_fragments::ReleaseFileRecord AS file
           ON file.release_id = :release
    WHERE held.release_id = :release
    ORDER BY file.content_type DESC, file.original_filename COLLATE natural_order DESC, file.id DESC
"#
);

/// The read of the files `request` names in its order: the file of each row.
fn release_files_read(request: &ReleaseFilesRequest) -> coven::ReadAs<Option<ReleaseFileRecord>> {
    let release = request.release_id.clone();
    match (request.order.column, request.order.descending) {
        (ReleaseFileColumn::Name, false) => {
            coven::ReadAs::new(ReleaseFilesByNameParams { release }, |row| row.file)
        }
        (ReleaseFileColumn::Name, true) => {
            coven::ReadAs::new(ReleaseFilesByNameBackwardParams { release }, |row| row.file)
        }
        (ReleaseFileColumn::Size, false) => {
            coven::ReadAs::new(ReleaseFilesBySizeParams { release }, |row| row.file)
        }
        (ReleaseFileColumn::Size, true) => {
            coven::ReadAs::new(ReleaseFilesBySizeBackwardParams { release }, |row| row.file)
        }
        (ReleaseFileColumn::Kind, false) => {
            coven::ReadAs::new(ReleaseFilesByKindParams { release }, |row| row.file)
        }
        (ReleaseFileColumn::Kind, true) => {
            coven::ReadAs::new(ReleaseFilesByKindBackwardParams { release }, |row| row.file)
        }
    }
}

/// The files `rows` hold; `None` when there are none, as the library holds no
/// such release.
fn release_files(rows: Vec<Option<ReleaseFileRecord>>) -> Result<Option<Vec<DbFile>>, DbError> {
    if rows.is_empty() {
        return Ok(None);
    }
    rows.into_iter().flatten().map(ReleaseFileRecord::into_file).collect::<Result<_, _>>().map(Some)
}

/// The statement reading a release's files in `order`.
#[cfg(test)]
pub(crate) fn release_files_sql(order: ReleaseFileOrder) -> &'static str {
    match (order.column, order.descending) {
        (ReleaseFileColumn::Name, false) => ReleaseFilesByName::SQL,
        (ReleaseFileColumn::Name, true) => ReleaseFilesByNameBackward::SQL,
        (ReleaseFileColumn::Size, false) => ReleaseFilesBySize::SQL,
        (ReleaseFileColumn::Size, true) => ReleaseFilesBySizeBackward::SQL,
        (ReleaseFileColumn::Kind, false) => ReleaseFilesByKind::SQL,
        (ReleaseFileColumn::Kind, true) => ReleaseFilesByKindBackward::SQL,
    }
}

impl LibraryViews {
    /// Follow the files of the release `initial` names, or none, in its
    /// order. Another release or order is a new request on the same query.
    pub fn subscribe_release_files(
        &self,
        initial: Option<ReleaseFilesRequest>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<ReleaseFilesRequest>,
        Option<Vec<DbFile>>,
        bae_store::DbError,
    > {
        self.store
            .subscribe_reconfigurable(initial, |request| {
                Ok(coven::Maybe(request.as_ref().map(release_files_read)))
            })
            .process(|_, rows| Ok(rows.map(release_files).transpose()?.flatten()))
    }
}

#[cfg(test)]
#[path = "release_files_tests.rs"]
mod tests;
