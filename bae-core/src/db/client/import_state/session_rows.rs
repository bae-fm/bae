//! The pane's per-candidate state between visits: which surface it shows,
//! the typed-search form, and the last command that failed.

use super::*;
use crate::import::{
    CandidateSession, MetadataPresentation, PaneCommand, PaneFailure, SearchForm, SearchTab,
};
use crate::ui::{GroupingBlockReason, UiError, UiErrorCategory};

fn presentation_column(presentation: MetadataPresentation) -> &'static str {
    match presentation {
        MetadataPresentation::Draft => "draft",
        MetadataPresentation::FindOnline => "find_online",
    }
}

fn presentation_of(column: &str) -> Result<MetadataPresentation, DbError> {
    match column {
        "draft" => Ok(MetadataPresentation::Draft),
        "find_online" => Ok(MetadataPresentation::FindOnline),
        other => Err(DbError::Message(format!(
            "unreadable session presentation {other:?}"
        ))),
    }
}

fn tab_column(tab: SearchTab) -> &'static str {
    match tab {
        SearchTab::General => "general",
        SearchTab::CatalogNumber => "catalog_number",
        SearchTab::Barcode => "barcode",
    }
}

fn tab_of(column: &str) -> Result<SearchTab, DbError> {
    match column {
        "general" => Ok(SearchTab::General),
        "catalog_number" => Ok(SearchTab::CatalogNumber),
        "barcode" => Ok(SearchTab::Barcode),
        other => Err(DbError::Message(format!(
            "unreadable session search tab {other:?}"
        ))),
    }
}

fn command_column(command: PaneCommand) -> &'static str {
    match command {
        PaneCommand::Import => "import",
        PaneCommand::CancelImport => "cancel_import",
        PaneCommand::MergeArtists => "merge_artists",
        PaneCommand::ReadFileTags => "read_file_tags",
        PaneCommand::ChangeLookups => "change_lookups",
        PaneCommand::ChangeSearchWords => "change_search_words",
        PaneCommand::ChangeAgreements => "change_agreements",
    }
}

fn command_of(column: &str) -> Result<PaneCommand, DbError> {
    Ok(match column {
        "import" => PaneCommand::Import,
        "cancel_import" => PaneCommand::CancelImport,
        "merge_artists" => PaneCommand::MergeArtists,
        "read_file_tags" => PaneCommand::ReadFileTags,
        "change_lookups" => PaneCommand::ChangeLookups,
        "change_search_words" => PaneCommand::ChangeSearchWords,
        "change_agreements" => PaneCommand::ChangeAgreements,
        other => {
            return Err(DbError::Message(format!(
                "unreadable pane command {other:?}"
            )))
        }
    })
}

fn block_column(reason: GroupingBlockReason) -> &'static str {
    match reason {
        GroupingBlockReason::SourceChanged => "source_changed",
        GroupingBlockReason::SourceGone => "source_gone",
        GroupingBlockReason::FolderFilesTaken => "folder_files_taken",
        GroupingBlockReason::FolderFilesContested => "folder_files_contested",
        GroupingBlockReason::FolderFilesDownloading => "folder_files_downloading",
    }
}

fn cloud_setup_column(failure: coven::CloudHomeSetupFailure) -> &'static str {
    use coven::CloudHomeSetupFailure as F;
    match failure {
        F::Authentication => "authentication",
        F::PermissionDenied => "permission_denied",
        F::ContainerNotFound => "container_not_found",
        F::RegionMismatch => "region_mismatch",
        F::QuotaExceeded => "quota_exceeded",
        F::InvalidConfiguration => "invalid_configuration",
        F::LocationOccupied => "location_occupied",
        F::Network => "network",
        F::DeviceIdentityMissing => "device_identity_missing",
        F::SecureStorage => "secure_storage",
        F::Internal => "internal",
    }
}

/// How a failure's class is stored: its name, and for a class that carries a
/// reason, the reason after a dot.
fn category_column(category: UiErrorCategory) -> String {
    use UiErrorCategory as C;
    match category {
        C::Database => "database".to_string(),
        C::Config => "config".to_string(),
        C::Internal => "internal".to_string(),
        C::SyncUpdateRequired => "sync_update_required".to_string(),
        C::Import => "import".to_string(),
        C::ImportData => "import_data".to_string(),
        C::CandidateImportInProgress => "candidate_import_in_progress".to_string(),
        C::CandidateBeingIdentified => "candidate_being_identified".to_string(),
        C::CandidateAlreadyImported => "candidate_already_imported".to_string(),
        C::MetadataTrackCount => "metadata_track_count".to_string(),
        C::GroupingBlocked(reason) => format!("grouping_blocked.{}", block_column(reason)),
        C::Export => "export".to_string(),
        C::Save => "save".to_string(),
        C::CloudSetup(failure) => format!("cloud_setup.{}", cloud_setup_column(failure)),
        C::DeviceIdentityMissing => "device_identity_missing".to_string(),
        C::Credentials => "credentials".to_string(),
        C::Network => "network".to_string(),
        C::Keyring => "keyring".to_string(),
        C::KeyringLocked => "keyring_locked".to_string(),
        C::Membership => "membership".to_string(),
    }
}

fn category_of(column: &str) -> Result<UiErrorCategory, DbError> {
    use coven::CloudHomeSetupFailure as F;
    use GroupingBlockReason as B;
    use UiErrorCategory as C;
    let unreadable = || DbError::Message(format!("unreadable failure class {column:?}"));
    Ok(match column.split_once('.') {
        Some(("grouping_blocked", stored)) => C::GroupingBlocked(
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
        Some(("cloud_setup", stored)) => C::CloudSetup(
            [
                F::Authentication,
                F::PermissionDenied,
                F::ContainerNotFound,
                F::RegionMismatch,
                F::QuotaExceeded,
                F::InvalidConfiguration,
                F::LocationOccupied,
                F::Network,
                F::DeviceIdentityMissing,
                F::SecureStorage,
                F::Internal,
            ]
            .into_iter()
            .find(|failure| cloud_setup_column(*failure) == stored)
            .ok_or_else(unreadable)?,
        ),
        Some(_) => return Err(unreadable()),
        None => match column {
            "database" => C::Database,
            "config" => C::Config,
            "internal" => C::Internal,
            "sync_update_required" => C::SyncUpdateRequired,
            "import" => C::Import,
            "import_data" => C::ImportData,
            "candidate_import_in_progress" => C::CandidateImportInProgress,
            "candidate_being_identified" => C::CandidateBeingIdentified,
            "candidate_already_imported" => C::CandidateAlreadyImported,
            "metadata_track_count" => C::MetadataTrackCount,
            "export" => C::Export,
            "save" => C::Save,
            "device_identity_missing" => C::DeviceIdentityMissing,
            "credentials" => C::Credentials,
            "network" => C::Network,
            "keyring" => C::Keyring,
            "keyring_locked" => C::KeyringLocked,
            "membership" => C::Membership,
            _ => return Err(unreadable()),
        },
    })
}

/// The session the pane left for `content_hash`, or `None` before it has
/// touched the candidate.
pub(super) fn load_session_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
) -> Result<Option<CandidateSession>, DbError> {
    let row = sql
        .query_row(
            "SELECT presentation, search_tab, search_artist, search_album, \
                    search_catalog, search_barcode, \
                    error_command, error_category, error_detail \
             FROM import_candidate_session WHERE content_hash = ?",
            [content_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(presentation, tab, artist, album, catalog, barcode, command, category, detail)| {
            let error = match (command, category, detail) {
                (Some(command), Some(category), Some(detail)) => Some(PaneFailure {
                    command: command_of(&command)?,
                    error: UiError::Diagnostic {
                        category: category_of(&category)?,
                        detail,
                    },
                }),
                (None, None, None) => None,
                _ => {
                    return Err(DbError::Message(
                        "a pane failure is stored in part".to_string(),
                    ))
                }
            };
            Ok(CandidateSession {
                presentation: presentation_of(&presentation)?,
                search: SearchForm {
                    tab: tab_of(&tab)?,
                    artist,
                    album,
                    catalog,
                    barcode,
                },
                error,
            })
        },
    )
    .transpose()
}

/// Show `presentation` in the candidate's pane, leaving the rest of its
/// session as it is. A candidate with no session yet gets a fresh one with
/// this surface showing; one with no state row gets nothing.
pub(super) fn present_on(
    sql: &SqlContext<'_, '_>,
    content_hash: &str,
    presentation: MetadataPresentation,
) -> Result<(), DbError> {
    let search = SearchForm::default();
    sql.execute(
        "INSERT INTO import_candidate_session (\
             content_hash, presentation, search_tab, search_artist, \
             search_album, search_catalog, search_barcode, \
             error_command, error_category, error_detail) \
         SELECT ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL \
         WHERE EXISTS (SELECT 1 FROM import_candidate_state WHERE content_hash = ?) \
         ON CONFLICT (content_hash) DO UPDATE SET \
             presentation = excluded.presentation",
        params![
            content_hash,
            presentation_column(presentation),
            tab_column(search.tab),
            search.artist,
            search.album,
            search.catalog,
            search.barcode,
            content_hash,
        ],
    )?;
    Ok(())
}

impl Database {
    /// Open the pane on Find online for every one of these candidates, leaving
    /// the rest of each session as it is. A candidate with no session yet gets
    /// the one its pane opens on, with this surface showing.
    ///
    /// One statement per candidate in one call, so an admission is one act
    /// and the pane of a candidate a person is looking at follows it in the
    /// same read as the rest.
    pub async fn open_import_candidate_sessions_on_find_online(
        &self,
        content_hashes: Vec<String>,
    ) -> Result<(), DbError> {
        if content_hashes.is_empty() {
            return Ok(());
        }
        self.call(move |sql| {
            for content_hash in &content_hashes {
                present_on(sql, content_hash, MetadataPresentation::FindOnline)?;
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
        let error = session.error.as_ref().map(|failure| {
            let UiError::Diagnostic { category, detail } = &failure.error;
            (
                command_column(failure.command),
                category_column(*category),
                detail.clone(),
            )
        });
        let (error_command, error_category, error_detail) = match error {
            Some((command, category, detail)) => (Some(command), Some(category), Some(detail)),
            None => (None, None, None),
        };
        self.call(move |sql| {
            let affected = sql.execute(
                "INSERT INTO import_candidate_session (\
                     content_hash, presentation, search_tab, search_artist, \
                     search_album, search_catalog, search_barcode, \
                     error_command, error_category, error_detail) \
                 SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ? \
                 WHERE EXISTS (SELECT 1 FROM import_candidate_state WHERE content_hash = ?) \
                 ON CONFLICT (content_hash) DO UPDATE SET \
                     presentation = excluded.presentation, \
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
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every failure class reads back as the class it was stored as.
    #[test]
    fn every_failure_class_reads_back_as_itself() {
        use coven::CloudHomeSetupFailure as F;
        use GroupingBlockReason as B;
        use UiErrorCategory as C;
        let classes = [
            C::Database,
            C::Config,
            C::Internal,
            C::SyncUpdateRequired,
            C::Import,
            C::ImportData,
            C::CandidateImportInProgress,
            C::CandidateBeingIdentified,
            C::CandidateAlreadyImported,
            C::MetadataTrackCount,
            C::GroupingBlocked(B::SourceChanged),
            C::GroupingBlocked(B::SourceGone),
            C::GroupingBlocked(B::FolderFilesTaken),
            C::GroupingBlocked(B::FolderFilesContested),
            C::GroupingBlocked(B::FolderFilesDownloading),
            C::Export,
            C::Save,
            C::CloudSetup(F::Authentication),
            C::CloudSetup(F::PermissionDenied),
            C::CloudSetup(F::ContainerNotFound),
            C::CloudSetup(F::RegionMismatch),
            C::CloudSetup(F::QuotaExceeded),
            C::CloudSetup(F::InvalidConfiguration),
            C::CloudSetup(F::LocationOccupied),
            C::CloudSetup(F::Network),
            C::CloudSetup(F::DeviceIdentityMissing),
            C::CloudSetup(F::SecureStorage),
            C::CloudSetup(F::Internal),
            C::DeviceIdentityMissing,
            C::Credentials,
            C::Network,
            C::Keyring,
            C::KeyringLocked,
            C::Membership,
        ];
        for class in classes {
            assert_eq!(category_of(&category_column(class)).unwrap(), class);
        }
        assert!(category_of("grouping_blocked.nowhere").is_err());
        assert!(category_of("nothing").is_err());
    }
}
