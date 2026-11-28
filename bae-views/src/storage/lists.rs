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
use super::top_reads::*;
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

/// The list's rows in `window`, each with its key. A window from the top
/// reads from the order's first row, and one from a key from the key's row.
pub(crate) fn storage_window_on(
    sql: &SqlReadContext<'_>,
    list: &StorageList,
    window: &ListWindow,
) -> Result<WindowRead<StorageRowParts>, DbError> {
    let limit = sql_bound(window.limit);
    let rows = match (&list.scope, &window.from) {
        (StorageScope::Found { pattern, stored }, None) => {
            let statement = FoundTopParams {
                sort: list.lead.code().to_string(),
                search: Some(pattern.clone()),
                stored: *stored,
                limit,
            };
            keyed_rows!(sql, statement, key_0, key_1, key_2, key_3, key_4)
        }
        (StorageScope::Found { pattern, stored }, Some(from)) => {
            let [key_0, key_1, key_2, key_3, key_4] = StorageList::values_of(from)?;
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
        (StorageScope::All, None) => {
            macro_rules! top {
                ($params:ident) => {
                    keyed_rows!(sql, $params { limit }, key_0, key_1, key_2, key_3)
                };
            }
            match list.lead {
                StorageLead::Title => top!(AllTopTitleParams),
                StorageLead::TitleBack => top!(AllTopTitleBackParams),
                StorageLead::Artist => top!(AllTopArtistParams),
                StorageLead::ArtistBack => top!(AllTopArtistBackParams),
                StorageLead::Media => top!(AllTopMediaParams),
                StorageLead::MediaBack => top!(AllTopMediaBackParams),
                StorageLead::Files => top!(AllTopFilesParams),
                StorageLead::FilesBack => top!(AllTopFilesBackParams),
                StorageLead::Size => top!(AllTopSizeParams),
                StorageLead::SizeBack => top!(AllTopSizeBackParams),
                StorageLead::Storage => top!(AllTopStorageParams),
                StorageLead::StorageBack => top!(AllTopStorageBackParams),
                StorageLead::Added => top!(AllTopAddedParams),
                StorageLead::AddedBack => top!(AllTopAddedBackParams),
            }
        }
        (StorageScope::Kept(stored), None) => {
            let stored = *stored;
            macro_rules! top {
                ($params:ident) => {
                    keyed_rows!(sql, $params { stored, limit }, key_0, key_1, key_2, key_3)
                };
            }
            match list.lead {
                StorageLead::Title => top!(KeptTopTitleParams),
                StorageLead::TitleBack => top!(KeptTopTitleBackParams),
                StorageLead::Artist => top!(KeptTopArtistParams),
                StorageLead::ArtistBack => top!(KeptTopArtistBackParams),
                StorageLead::Media => top!(KeptTopMediaParams),
                StorageLead::MediaBack => top!(KeptTopMediaBackParams),
                StorageLead::Files => top!(KeptTopFilesParams),
                StorageLead::FilesBack => top!(KeptTopFilesBackParams),
                StorageLead::Size => top!(KeptTopSizeParams),
                StorageLead::SizeBack => top!(KeptTopSizeBackParams),
                StorageLead::Storage => top!(KeptTopStorageParams),
                StorageLead::StorageBack => top!(KeptTopStorageBackParams),
                StorageLead::Added => top!(KeptTopAddedParams),
                StorageLead::AddedBack => top!(KeptTopAddedBackParams),
            }
        }
        (StorageScope::All, Some(from)) => {
            let [key_0, key_1, key_2, key_3] = StorageList::values_of(from)?;
            macro_rules! from_key {
                ($params:ident) => {
                    keyed_rows!(
                        sql,
                        $params { limit, key_0, key_1, key_2, key_3 },
                        key_0,
                        key_1,
                        key_2,
                        key_3
                    )
                };
            }
            match list.lead {
                StorageLead::Title => from_key!(AllWindowTitleParams),
                StorageLead::TitleBack => from_key!(AllWindowTitleBackParams),
                StorageLead::Artist => from_key!(AllWindowArtistParams),
                StorageLead::ArtistBack => from_key!(AllWindowArtistBackParams),
                StorageLead::Media => from_key!(AllWindowMediaParams),
                StorageLead::MediaBack => from_key!(AllWindowMediaBackParams),
                StorageLead::Files => from_key!(AllWindowFilesParams),
                StorageLead::FilesBack => from_key!(AllWindowFilesBackParams),
                StorageLead::Size => from_key!(AllWindowSizeParams),
                StorageLead::SizeBack => from_key!(AllWindowSizeBackParams),
                StorageLead::Storage => from_key!(AllWindowStorageParams),
                StorageLead::StorageBack => from_key!(AllWindowStorageBackParams),
                StorageLead::Added => from_key!(AllWindowAddedParams),
                StorageLead::AddedBack => from_key!(AllWindowAddedBackParams),
            }
        }
        (StorageScope::Kept(stored), Some(from)) => {
            let stored = *stored;
            let [key_0, key_1, key_2, key_3] = StorageList::values_of(from)?;
            macro_rules! from_key {
                ($params:ident) => {
                    keyed_rows!(
                        sql,
                        $params { stored, limit, key_0, key_1, key_2, key_3 },
                        key_0,
                        key_1,
                        key_2,
                        key_3
                    )
                };
            }
            match list.lead {
                StorageLead::Title => from_key!(KeptWindowTitleParams),
                StorageLead::TitleBack => from_key!(KeptWindowTitleBackParams),
                StorageLead::Artist => from_key!(KeptWindowArtistParams),
                StorageLead::ArtistBack => from_key!(KeptWindowArtistBackParams),
                StorageLead::Media => from_key!(KeptWindowMediaParams),
                StorageLead::MediaBack => from_key!(KeptWindowMediaBackParams),
                StorageLead::Files => from_key!(KeptWindowFilesParams),
                StorageLead::FilesBack => from_key!(KeptWindowFilesBackParams),
                StorageLead::Size => from_key!(KeptWindowSizeParams),
                StorageLead::SizeBack => from_key!(KeptWindowSizeBackParams),
                StorageLead::Storage => from_key!(KeptWindowStorageParams),
                StorageLead::StorageBack => from_key!(KeptWindowStorageBackParams),
                StorageLead::Added => from_key!(KeptWindowAddedParams),
                StorageLead::AddedBack => from_key!(KeptWindowAddedBackParams),
            }
        }
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
