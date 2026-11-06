//! A release's files in the order a person sorts the Storage sheet's file
//! table by, read through one live subscription whose release and order
//! change in place.

use super::*;

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

/// The statement reading a release's files in `order`: the terms of the
/// index serving it, every one of them backward when the order is.
pub(crate) fn release_files_in(order: ReleaseFileOrder) -> String {
    let terms: &[&str] = match order.column {
        ReleaseFileColumn::Name => &["original_filename COLLATE natural_order", "id"],
        ReleaseFileColumn::Size => &["file_size", "original_filename COLLATE natural_order", "id"],
        ReleaseFileColumn::Kind => {
            &["content_type", "original_filename COLLATE natural_order", "id"]
        }
    };
    let terms = terms
        .iter()
        .map(|term| match order.descending {
            true => format!("{term} DESC"),
            false => (*term).to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT {} FROM release_files AS f WHERE f.release_id = ?1 ORDER BY {terms}",
        bae_records::release_file_columns!()
    )
}

/// The files `request` names in its order; `None` when the library holds no
/// such release.
fn release_files_on(
    sql: &SqlReadContext<'_>,
    request: &ReleaseFilesRequest,
) -> Result<Option<Vec<DbFile>>, DbError> {
    if find_release_by_id_on(sql, &request.release_id)?.is_none() {
        return Ok(None);
    }
    Ok(Some(sql.query(
        &release_files_in(request.order),
        params![request.release_id],
        row_to_file,
    )?))
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
        coven::CovenError,
    > {
        self.store
            .subscribe_reconfigurable(initial, |request, sql| {
                let Some(request) = request else {
                    return Ok(None);
                };
                release_files_on(&sql, request).map_err(CovenError::from)
            })
            .process(Ok)
    }
}

#[cfg(test)]
#[path = "release_files_tests.rs"]
mod tests;
