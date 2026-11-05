//! The pane's per-candidate state between visits: which surface it shows,
//! the typed-search form, and the last command that failed.
//! Its table is `session_rows.sql`, which every device's library holds.

pub(super) const SQL: &str = include_str!("session_rows.sql");

#[cfg(desktop)]
use super::*;
#[cfg(desktop)]
use crate::{
    CandidateSession, FindOnlineSection, MetadataPresentation, PaneCommand, PaneFailure, PaneMove,
    SearchForm, SearchTab,
};
#[cfg(desktop)]
use bae_diagnostics::{GroupingBlockReason, UiError, UiErrorCategory};

#[cfg(desktop)]
fn presentation_column(presentation: MetadataPresentation) -> &'static str {
    match presentation {
        MetadataPresentation::Draft => "draft",
        MetadataPresentation::FindOnline => "find_online",
    }
}

#[cfg(desktop)]
fn presentation_of(column: &str) -> Result<MetadataPresentation, DbError> {
    match column {
        "draft" => Ok(MetadataPresentation::Draft),
        "find_online" => Ok(MetadataPresentation::FindOnline),
        other => Err(DbError::unreadable("import_candidate_session.presentation", other)),
    }
}

#[cfg(desktop)]
fn section_column(section: FindOnlineSection) -> &'static str {
    match section {
        FindOnlineSection::Automatic => "automatic",
        FindOnlineSection::Search => "search",
    }
}

#[cfg(desktop)]
fn section_of(column: &str) -> Result<FindOnlineSection, DbError> {
    match column {
        "automatic" => Ok(FindOnlineSection::Automatic),
        "search" => Ok(FindOnlineSection::Search),
        other => Err(DbError::unreadable("import_candidate_session.find_online_section", other)),
    }
}

#[cfg(desktop)]
fn tab_column(tab: SearchTab) -> &'static str {
    match tab {
        SearchTab::General => "general",
        SearchTab::CatalogNumber => "catalog_number",
        SearchTab::Barcode => "barcode",
    }
}

#[cfg(desktop)]
fn tab_of(column: &str) -> Result<SearchTab, DbError> {
    match column {
        "general" => Ok(SearchTab::General),
        "catalog_number" => Ok(SearchTab::CatalogNumber),
        "barcode" => Ok(SearchTab::Barcode),
        other => Err(DbError::unreadable("import_candidate_session.search_tab", other)),
    }
}

#[cfg(desktop)]
fn command_column(command: PaneCommand) -> &'static str {
    match command {
        PaneCommand::Import => "import",
        PaneCommand::ReadFileTags => "read_file_tags",
        PaneCommand::ChangeLookups => "change_lookups",
        PaneCommand::ChangeSearchWords => "change_search_words",
        PaneCommand::ChangeAgreements => "change_agreements",
        PaneCommand::KeepOwnDraft => "keep_own_draft",
        PaneCommand::LinkSharedAlbum => "link_shared_album",
        PaneCommand::Unlink => "unlink",
    }
}

#[cfg(desktop)]
fn command_of(column: &str) -> Result<PaneCommand, DbError> {
    Ok(match column {
        "import" => PaneCommand::Import,
        "read_file_tags" => PaneCommand::ReadFileTags,
        "change_lookups" => PaneCommand::ChangeLookups,
        "change_search_words" => PaneCommand::ChangeSearchWords,
        "change_agreements" => PaneCommand::ChangeAgreements,
        "keep_own_draft" => PaneCommand::KeepOwnDraft,
        "link_shared_album" => PaneCommand::LinkSharedAlbum,
        "unlink" => PaneCommand::Unlink,
        other => return Err(DbError::unreadable("import_candidate_session.error_command", other)),
    })
}

#[cfg(desktop)]
fn block_column(reason: GroupingBlockReason) -> &'static str {
    match reason {
        GroupingBlockReason::SourceChanged => "source_changed",
        GroupingBlockReason::SourceGone => "source_gone",
        GroupingBlockReason::FolderFilesTaken => "folder_files_taken",
        GroupingBlockReason::FolderFilesContested => "folder_files_contested",
        GroupingBlockReason::FolderFilesDownloading => "folder_files_downloading",
    }
}

/// How a failure's class is stored: its name, and for a class that carries a
/// reason or counts, each after a dot.
#[cfg(desktop)]
fn category_column(category: UiErrorCategory) -> String {
    use UiErrorCategory as C;
    let class = category.class().name();
    match category {
        C::MetadataTrackCount { folder_tracks, release_tracks } => {
            format!("{class}.{folder_tracks}.{release_tracks}")
        }
        C::TiedMediums { mediums } => {
            format!("{class}.{}", mediums.iter().map(u32::to_string).collect::<Vec<_>>().join("."))
        }
        C::GroupingBlocked(reason) => format!("{class}.{}", block_column(reason)),
        C::Storage(failure) | C::S3VersioningUnknown(failure) => {
            format!("{class}.{}", storage_failure_json(&failure))
        }
        C::StorageCheck { check, failure } => {
            format!("{class}.{}.{}", check_column(check), storage_failure_json(&failure))
        }
        C::S3VersioningUnsupported(state) => format!("{class}.{}", versioning_column(state)),
        C::S3Prefix(error) => format!(
            "{class}.{}",
            serde_json::to_string(&error).expect("S3 prefix error serializes")
        ),
        _ => class.to_string(),
    }
}

#[cfg(desktop)]
fn storage_failure_json(failure: &coven::StorageFailure) -> String {
    serde_json::to_string(failure).expect("storage failure serializes")
}

#[cfg(desktop)]
fn check_column(value: coven::StorageCheck) -> &'static str {
    match value {
        coven::StorageCheck::Create => "Create",
        coven::StorageCheck::CreateOnce => "CreateOnce",
        coven::StorageCheck::ConditionalReplace => "ConditionalReplace",
        coven::StorageCheck::Read => "Read",
        coven::StorageCheck::ReadRange => "ReadRange",
        coven::StorageCheck::List => "List",
        coven::StorageCheck::Delete => "Delete",
        coven::StorageCheck::MultipartCreateOnce => "MultipartCreateOnce",
        coven::StorageCheck::PublicationTime => "PublicationTime",
        coven::StorageCheck::Behavior => "Behavior",
    }
}
#[cfg(desktop)]
fn check_of(value: &str) -> Option<coven::StorageCheck> {
    Some(match value {
        "Create" => coven::StorageCheck::Create,
        "CreateOnce" => coven::StorageCheck::CreateOnce,
        "ConditionalReplace" => coven::StorageCheck::ConditionalReplace,
        "Read" => coven::StorageCheck::Read,
        "ReadRange" => coven::StorageCheck::ReadRange,
        "List" => coven::StorageCheck::List,
        "Delete" => coven::StorageCheck::Delete,
        "MultipartCreateOnce" => coven::StorageCheck::MultipartCreateOnce,
        "PublicationTime" => coven::StorageCheck::PublicationTime,
        "Behavior" => coven::StorageCheck::Behavior,
        _ => return None,
    })
}
#[cfg(desktop)]
fn versioning_column(value: coven::S3BucketVersioning) -> &'static str {
    match value {
        coven::S3BucketVersioning::Disabled => "Disabled",
        coven::S3BucketVersioning::Enabled => "Enabled",
        coven::S3BucketVersioning::Suspended => "Suspended",
        coven::S3BucketVersioning::Unavailable => "Unavailable",
    }
}
#[cfg(desktop)]
fn versioning_of(value: &str) -> Option<coven::S3BucketVersioning> {
    Some(match value {
        "Disabled" => coven::S3BucketVersioning::Disabled,
        "Enabled" => coven::S3BucketVersioning::Enabled,
        "Suspended" => coven::S3BucketVersioning::Suspended,
        "Unavailable" => coven::S3BucketVersioning::Unavailable,
        _ => return None,
    })
}

#[cfg(desktop)]
fn category_of(column: &str) -> Result<UiErrorCategory, DbError> {
    use bae_diagnostics::UiErrorClass as K;
    use GroupingBlockReason as B;
    use UiErrorCategory as C;
    const WHAT: &str = "import_candidate_session.error_category";
    let unreadable = || DbError::unreadable(WHAT, column);
    let unreadable_because =
        |error: &dyn std::fmt::Display| DbError::unreadable(WHAT, format!("{column}: {error}"));
    let (name, stored) = match column.split_once('.') {
        Some((name, stored)) => (name, Some(stored)),
        None => (column, None),
    };
    let class = bae_diagnostics::UiErrorClass::named(name).ok_or_else(unreadable)?;
    let Some(stored) = stored else {
        return UiErrorCategory::plain(class).ok_or_else(unreadable);
    };
    Ok(match class {
        K::GroupingBlocked => C::GroupingBlocked(
            [
                B::SourceChanged,
                B::SourceGone,
                B::FolderFilesTaken,
                B::FolderFilesContested,
                B::FolderFilesDownloading,
            ]
            .into_iter()
            .find(|reason| block_column(*reason) == stored)
            .ok_or_else(unreadable)?,
        ),
        K::Storage => {
            C::Storage(serde_json::from_str(stored).map_err(|error| unreadable_because(&error))?)
        }
        K::StorageCheck => {
            let (check, failure) = stored.split_once('.').ok_or_else(unreadable)?;
            C::StorageCheck {
                check: check_of(check).ok_or_else(unreadable)?,
                failure: serde_json::from_str(failure)
                    .map_err(|error| unreadable_because(&error))?,
            }
        }
        K::S3VersioningUnknown => C::S3VersioningUnknown(
            serde_json::from_str(stored).map_err(|error| unreadable_because(&error))?,
        ),
        K::S3Prefix => {
            C::S3Prefix(serde_json::from_str(stored).map_err(|error| unreadable_because(&error))?)
        }
        K::S3VersioningUnsupported => {
            C::S3VersioningUnsupported(versioning_of(stored).ok_or_else(unreadable)?)
        }
        K::TiedMediums => C::TiedMediums {
            mediums: stored
                .split('.')
                .map(|medium| medium.parse().map_err(|error| unreadable_because(&error)))
                .collect::<Result<_, _>>()?,
        },
        K::MetadataTrackCount => {
            let (folder_tracks, release_tracks) = stored.split_once('.').ok_or_else(unreadable)?;
            C::MetadataTrackCount {
                folder_tracks: folder_tracks.parse().map_err(|error| unreadable_because(&error))?,
                release_tracks: release_tracks
                    .parse()
                    .map_err(|error| unreadable_because(&error))?,
            }
        }
        _ => return Err(unreadable()),
    })
}

/// The stored session's columns, as one read gives them.
#[cfg(desktop)]
type SessionRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

#[cfg(desktop)]
const SESSION_SELECT: &str = "SELECT presentation, search_tab, search_artist, search_album, \
            search_catalog, search_barcode, \
            error_command, error_category, error_detail, find_online_section \
     FROM import_candidate_session WHERE content_hash = ?";

#[cfg(desktop)]
fn session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

#[cfg(desktop)]
fn session_of(row: SessionRow) -> Result<CandidateSession, DbError> {
    let (presentation, tab, artist, album, catalog, barcode, command, category, detail, section) =
        row;
    let error = match (command, category, detail) {
        (Some(command), Some(category), Some(detail)) => Some(PaneFailure {
            command: command_of(&command)?,
            error: UiError::Diagnostic { category: category_of(&category)?, detail },
        }),
        (None, None, None) => None,
        _ => return Err(DbError::Message("a pane failure is stored in part".to_string())),
    };
    Ok(CandidateSession {
        presentation: presentation_of(&presentation)?,
        find_online_section: section_of(&section)?,
        search: SearchForm { tab: tab_of(&tab)?, artist, album, catalog, barcode },
        error,
    })
}

/// The session the pane left for `content_hash`, or `None` before it has
/// touched the candidate.
#[cfg(desktop)]
pub(super) fn load_session_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Option<CandidateSession>, DbError> {
    sql.query_row(SESSION_SELECT, [content_hash], session_row)
        .optional()?
        .map(session_of)
        .transpose()
}

/// Store `session` whole for `content_hash`, inside the caller's write.
/// Forget where the pane of `content_hash` was.
#[cfg(desktop)]
pub(super) fn delete_session(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute("DELETE FROM import_candidate_session WHERE content_hash = ?", [content_hash])?;
    Ok(())
}

#[cfg(desktop)]
fn save_session_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    session: &CandidateSession,
) -> Result<(), DbError> {
    let (error_command, error_category, error_detail) = match &session.error {
        Some(failure) => {
            let UiError::Diagnostic { category, detail } = &failure.error;
            (
                Some(command_column(failure.command)),
                Some(category_column(category.clone())),
                Some(detail.clone()),
            )
        }
        None => (None, None, None),
    };
    let affected = sql.execute(
        "INSERT INTO import_candidate_session (\
             content_hash, presentation, find_online_section, search_tab, \
             search_artist, search_album, search_catalog, search_barcode, \
             error_command, error_category, error_detail) \
         SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
         WHERE EXISTS (SELECT 1 FROM import_candidate_state WHERE content_hash = ?) \
         ON CONFLICT (content_hash) DO UPDATE SET \
             presentation = excluded.presentation, \
             find_online_section = excluded.find_online_section, \
             search_tab = excluded.search_tab, \
             search_artist = excluded.search_artist, \
             search_album = excluded.search_album, \
             search_catalog = excluded.search_catalog, \
             search_barcode = excluded.search_barcode, \
             error_command = excluded.error_command, \
             error_category = excluded.error_category, \
             error_detail = excluded.error_detail",
        params![
            content_hash,
            presentation_column(session.presentation),
            section_column(session.find_online_section),
            tab_column(session.search.tab),
            session.search.artist,
            session.search.album,
            session.search.catalog,
            session.search.barcode,
            error_command,
            error_category,
            error_detail,
            content_hash,
        ],
    )?;
    if affected == 0 {
        return Err(DbError::Message(
            "the pane's session has no candidate state row to hang off".to_string(),
        ));
    }
    Ok(())
}

/// Move the candidate's pane by the one rule, [`CandidateSession::moved`],
/// leaving the rest of its session as it is. A candidate with no session yet
/// gets a fresh one moved the same way; one with no state row gets nothing.
#[cfg(desktop)]
pub(crate) fn move_pane_on(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    pane_move: PaneMove,
) -> Result<(), DbError> {
    let has_state = sql
        .query_row(
            "SELECT 1 FROM import_candidate_state WHERE content_hash = ?",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_state {
        return Ok(());
    }
    // Every move names where the pane goes, so a session nobody has touched
    // moves the same from any starting surface.
    let session = sql
        .query_row(SESSION_SELECT, [content_hash], session_row)
        .optional()?
        .map(session_of)
        .transpose()?
        .unwrap_or_else(|| CandidateSession::initial(false, false))
        .moved(pane_move);
    save_session_on(sql, content_hash, &session)
}

#[cfg(desktop)]
impl ImportTables {
    /// Move the pane of every one of these candidates by `pane_move`, in one
    /// call, so an admission is one act and the pane of a candidate a person
    /// is looking at follows it in the same read as the rest.
    pub async fn move_import_candidate_panes(
        &self,
        content_hashes: Vec<String>,
        pane_move: PaneMove,
    ) -> Result<(), DbError> {
        if content_hashes.is_empty() {
            return Ok(());
        }
        self.database
            .call(move |sql| {
                for content_hash in &content_hashes {
                    move_pane_on(sql, content_hash, pane_move)?;
                }
                Ok(())
            })
            .await
    }

    /// Record the pane's state for a candidate, whole: the row is the session,
    /// and the caller hands over the next value of all of it.
    pub async fn save_import_candidate_session(
        &self,
        content_hash: &str,
        session: &CandidateSession,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        let session = session.clone();
        self.database.call(move |sql| save_session_on(sql, &content_hash, &session)).await
    }
}

#[cfg(test)]
#[path = "session_rows_tests.rs"]
mod tests;
