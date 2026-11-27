//! The Storage Manager list's reads that depend on its order: its window, the
//! rows before a key and the key at a position. Each is the compiled read of
//! the list's sort and scope; a list a search narrows has one of each, which
//! takes the sort as a parameter.

use super::found_reads::*;
use super::kept_window_reads::*;
use super::key_reads::*;
use super::order::{StorageLead, StorageList, StorageScope};
use super::position_reads::*;
use super::row::StorageRowParts;
use super::window_reads::*;
use super::*;
use bae_store::list_windows::sql_bound;
use bae_store::{statement_optional, statement_row, statement_rows};

/// The key a row of a read with the columns `key_0` on says it is at.
macro_rules! key_of {
    ($row:ident, $($column:ident),+) => {
        OrderKey::new(vec![$($row.$column),+])
    };
}

/// A window's rows from the rows its read returns, each with its key.
macro_rules! keyed_rows {
    ($sql:ident, $statement:expr, $($column:ident),+) => {
        statement_rows($sql, &$statement)?
            .into_iter()
            .map(|row| {
                let key = key_of!(row, $($column),+);
                Ok((row.shown.into_parts()?, key))
            })
            .collect::<Result<Vec<_>, DbError>>()?
    };
}

/// The list's rows in `window`, each with its key.
pub(crate) fn storage_window_on(
    sql: &SqlReadContext<'_>,
    list: &StorageList,
    window: &ListWindow,
) -> Result<WindowRead<StorageRowParts>, DbError> {
    let limit = sql_bound(window.limit);
    macro_rules! window {
        ($params:ident { $($field:ident: $value:expr)? }) => {{
            let [key_0, key_1, key_2, key_3] = match &window.from {
                Some(key) => StorageList::values_of(key)?,
                None => list.top(),
            };
            keyed_rows!(
                sql,
                $params { $($field: $value,)? limit, key_0, key_1, key_2, key_3 },
                key_0, key_1, key_2, key_3
            )
        }};
    }
    let rows = match &list.scope {
        StorageScope::Found { pattern, stored } => {
            let [key_0, key_1, key_2, key_3, key_4] = match &window.from {
                Some(key) => StorageList::values_of(key)?,
                None => StorageList::found_top(),
            };
            let statement = FoundWindowParams {
                sort: list.lead.code().to_string(),
                search: Some(pattern.clone()),
                stored: *stored,
                limit,
                key_0,
                key_1,
                key_2,
                key_3,
                key_4,
            };
            keyed_rows!(sql, statement, key_0, key_1, key_2, key_3, key_4)
        }
        StorageScope::All => match list.lead {
            StorageLead::Title => window!(AllWindowTitleParams {}),
            StorageLead::TitleBack => window!(AllWindowTitleBackParams {}),
            StorageLead::Artist => window!(AllWindowArtistParams {}),
            StorageLead::ArtistBack => window!(AllWindowArtistBackParams {}),
            StorageLead::Media => window!(AllWindowMediaParams {}),
            StorageLead::MediaBack => window!(AllWindowMediaBackParams {}),
            StorageLead::Files => window!(AllWindowFilesParams {}),
            StorageLead::FilesBack => window!(AllWindowFilesBackParams {}),
            StorageLead::Size => window!(AllWindowSizeParams {}),
            StorageLead::SizeBack => window!(AllWindowSizeBackParams {}),
            StorageLead::Storage => window!(AllWindowStorageParams {}),
            StorageLead::StorageBack => window!(AllWindowStorageBackParams {}),
            StorageLead::Added => window!(AllWindowAddedParams {}),
            StorageLead::AddedBack => window!(AllWindowAddedBackParams {}),
        },
        StorageScope::Kept(stored) => match list.lead {
            StorageLead::Title => window!(KeptWindowTitleParams { stored: *stored }),
            StorageLead::TitleBack => window!(KeptWindowTitleBackParams { stored: *stored }),
            StorageLead::Artist => window!(KeptWindowArtistParams { stored: *stored }),
            StorageLead::ArtistBack => window!(KeptWindowArtistBackParams { stored: *stored }),
            StorageLead::Media => window!(KeptWindowMediaParams { stored: *stored }),
            StorageLead::MediaBack => window!(KeptWindowMediaBackParams { stored: *stored }),
            StorageLead::Files => window!(KeptWindowFilesParams { stored: *stored }),
            StorageLead::FilesBack => window!(KeptWindowFilesBackParams { stored: *stored }),
            StorageLead::Size => window!(KeptWindowSizeParams { stored: *stored }),
            StorageLead::SizeBack => window!(KeptWindowSizeBackParams { stored: *stored }),
            StorageLead::Storage => window!(KeptWindowStorageParams { stored: *stored }),
            StorageLead::StorageBack => window!(KeptWindowStorageBackParams { stored: *stored }),
            StorageLead::Added => window!(KeptWindowAddedParams { stored: *stored }),
            StorageLead::AddedBack => window!(KeptWindowAddedBackParams { stored: *stored }),
        },
    };
    Ok(WindowRead { window: window.clone(), rows })
}

/// How many rows of the list come before the row at `key`.
pub(crate) fn storage_rows_before_on(
    sql: &SqlReadContext<'_>,
    list: &StorageList,
    key: &OrderKey,
) -> Result<u64, DbError> {
    macro_rules! before {
        ($params:ident { $($field:ident: $value:expr)? }) => {{
            let [key_0, key_1, key_2, key_3] = StorageList::values_of(key)?;
            statement_row(sql, &$params { $($field: $value,)? key_0, key_1, key_2, key_3 })?
                .rows_before
        }};
    }
    Ok(match &list.scope {
        StorageScope::Found { pattern, stored } => {
            let [key_0, key_1, key_2, key_3, key_4] = StorageList::values_of(key)?;
            let statement = FoundBeforeParams {
                sort: list.lead.code().to_string(),
                search: Some(pattern.clone()),
                stored: *stored,
                key_0,
                key_1,
                key_2,
                key_3,
                key_4,
            };
            statement_row(sql, &statement)?.rows_before
        }
        StorageScope::All => match list.lead {
            StorageLead::Title => before!(AllBeforeTitleParams {}),
            StorageLead::TitleBack => before!(AllBeforeTitleBackParams {}),
            StorageLead::Artist => before!(AllBeforeArtistParams {}),
            StorageLead::ArtistBack => before!(AllBeforeArtistBackParams {}),
            StorageLead::Media => before!(AllBeforeMediaParams {}),
            StorageLead::MediaBack => before!(AllBeforeMediaBackParams {}),
            StorageLead::Files => before!(AllBeforeFilesParams {}),
            StorageLead::FilesBack => before!(AllBeforeFilesBackParams {}),
            StorageLead::Size => before!(AllBeforeSizeParams {}),
            StorageLead::SizeBack => before!(AllBeforeSizeBackParams {}),
            StorageLead::Storage => before!(AllBeforeStorageParams {}),
            StorageLead::StorageBack => before!(AllBeforeStorageBackParams {}),
            StorageLead::Added => before!(AllBeforeAddedParams {}),
            StorageLead::AddedBack => before!(AllBeforeAddedBackParams {}),
        },
        StorageScope::Kept(stored) => match list.lead {
            StorageLead::Title => before!(KeptBeforeTitleParams { stored: *stored }),
            StorageLead::TitleBack => before!(KeptBeforeTitleBackParams { stored: *stored }),
            StorageLead::Artist => before!(KeptBeforeArtistParams { stored: *stored }),
            StorageLead::ArtistBack => before!(KeptBeforeArtistBackParams { stored: *stored }),
            StorageLead::Media => before!(KeptBeforeMediaParams { stored: *stored }),
            StorageLead::MediaBack => before!(KeptBeforeMediaBackParams { stored: *stored }),
            StorageLead::Files => before!(KeptBeforeFilesParams { stored: *stored }),
            StorageLead::FilesBack => before!(KeptBeforeFilesBackParams { stored: *stored }),
            StorageLead::Size => before!(KeptBeforeSizeParams { stored: *stored }),
            StorageLead::SizeBack => before!(KeptBeforeSizeBackParams { stored: *stored }),
            StorageLead::Storage => before!(KeptBeforeStorageParams { stored: *stored }),
            StorageLead::StorageBack => before!(KeptBeforeStorageBackParams { stored: *stored }),
            StorageLead::Added => before!(KeptBeforeAddedParams { stored: *stored }),
            StorageLead::AddedBack => before!(KeptBeforeAddedBackParams { stored: *stored }),
        },
    })
}

/// The key of the row at `position` of the list, or of its last row when it
/// ends before it; `None` for an empty list.
pub(crate) fn storage_key_at_on(
    sql: &SqlReadContext<'_>,
    list: &StorageList,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let position = sql_bound(position);
    macro_rules! key_at {
        ($params:ident { $($field:ident: $value:expr)? }) => {
            statement_optional(sql, &$params { $($field: $value,)? position })?
                .map(|row| key_of!(row, key_0, key_1, key_2, key_3))
        };
    }
    Ok(match &list.scope {
        StorageScope::Found { pattern, stored } => {
            let statement = FoundKeyAtParams {
                sort: list.lead.code().to_string(),
                search: Some(pattern.clone()),
                stored: *stored,
                position,
            };
            statement_optional(sql, &statement)?
                .map(|row| key_of!(row, key_0, key_1, key_2, key_3, key_4))
        }
        StorageScope::All => match list.lead {
            StorageLead::Title => key_at!(AllKeyAtTitleParams {}),
            StorageLead::TitleBack => key_at!(AllKeyAtTitleBackParams {}),
            StorageLead::Artist => key_at!(AllKeyAtArtistParams {}),
            StorageLead::ArtistBack => key_at!(AllKeyAtArtistBackParams {}),
            StorageLead::Media => key_at!(AllKeyAtMediaParams {}),
            StorageLead::MediaBack => key_at!(AllKeyAtMediaBackParams {}),
            StorageLead::Files => key_at!(AllKeyAtFilesParams {}),
            StorageLead::FilesBack => key_at!(AllKeyAtFilesBackParams {}),
            StorageLead::Size => key_at!(AllKeyAtSizeParams {}),
            StorageLead::SizeBack => key_at!(AllKeyAtSizeBackParams {}),
            StorageLead::Storage => key_at!(AllKeyAtStorageParams {}),
            StorageLead::StorageBack => key_at!(AllKeyAtStorageBackParams {}),
            StorageLead::Added => key_at!(AllKeyAtAddedParams {}),
            StorageLead::AddedBack => key_at!(AllKeyAtAddedBackParams {}),
        },
        StorageScope::Kept(stored) => match list.lead {
            StorageLead::Title => key_at!(KeptKeyAtTitleParams { stored: *stored }),
            StorageLead::TitleBack => key_at!(KeptKeyAtTitleBackParams { stored: *stored }),
            StorageLead::Artist => key_at!(KeptKeyAtArtistParams { stored: *stored }),
            StorageLead::ArtistBack => key_at!(KeptKeyAtArtistBackParams { stored: *stored }),
            StorageLead::Media => key_at!(KeptKeyAtMediaParams { stored: *stored }),
            StorageLead::MediaBack => key_at!(KeptKeyAtMediaBackParams { stored: *stored }),
            StorageLead::Files => key_at!(KeptKeyAtFilesParams { stored: *stored }),
            StorageLead::FilesBack => key_at!(KeptKeyAtFilesBackParams { stored: *stored }),
            StorageLead::Size => key_at!(KeptKeyAtSizeParams { stored: *stored }),
            StorageLead::SizeBack => key_at!(KeptKeyAtSizeBackParams { stored: *stored }),
            StorageLead::Storage => key_at!(KeptKeyAtStorageParams { stored: *stored }),
            StorageLead::StorageBack => key_at!(KeptKeyAtStorageBackParams { stored: *stored }),
            StorageLead::Added => key_at!(KeptKeyAtAddedParams { stored: *stored }),
            StorageLead::AddedBack => key_at!(KeptKeyAtAddedBackParams { stored: *stored }),
        },
    })
}
