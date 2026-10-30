//! A release's files in the order a person sorts them by (see
//! [`ReleaseFileOrder`]), each order read from an index of each release's
//! files in it.

use super::*;
use crate::library::{ReleaseFileColumn, ReleaseFileOrder, ReleaseFilesRequest};

/// The statement reading a release's files in `order`: the terms of the
/// index serving it, every one of them backward when the order is.
pub(super) fn release_files_in(order: ReleaseFileOrder) -> String {
    let terms: &[&str] = match order.column {
        ReleaseFileColumn::Name => &["original_filename COLLATE natural_order", "id"],
        ReleaseFileColumn::Size => &["file_size", "original_filename COLLATE natural_order", "id"],
        ReleaseFileColumn::Kind => &[
            "content_type",
            "original_filename COLLATE natural_order",
            "id",
        ],
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
        release_file_columns!()
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

impl Database {
    /// Follow the files of the release `initial` names, or none, in its
    /// order. Another release or order is a new request on the same query.
    pub(crate) fn subscribe_release_files(
        &self,
        initial: Option<ReleaseFilesRequest>,
    ) -> coven::ReconfigurableLiveQuery<Option<ReleaseFilesRequest>, Option<Vec<DbFile>>> {
        self.inner
            .handle
            .subscribe_reconfigurable(initial, |request, sql| {
                let Some(request) = request else {
                    return Ok(None);
                };
                release_files_on(&sql, request).map_err(CovenError::from)
            })
            .process(Ok)
    }
}
