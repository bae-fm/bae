//! The import pane's last command for a candidate, when it failed: which
//! command, and why. Its table is `pane_failure_rows.sql`, which every
//! device's library holds.

pub(super) const SQL: &str = include_str!("pane_failure_rows.sql");

use super::*;
use crate::{PaneCommand, PaneFailure};
use bae_diagnostics::{GroupingBlockReason, UiError, UiErrorCategory};
use bae_types::edit_validation::EditValidationError;
use bae_types::mediums::SideHeading;

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
        other => return Err(DbError::unreadable("import_candidate_pane_failure.command", other)),
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

fn edit_column(reason: EditValidationError) -> &'static str {
    match reason {
        EditValidationError::EmptyAlbumTitle => "empty_album_title",
        EditValidationError::NoAlbumArtist => "no_album_artist",
        EditValidationError::EmptyArtistName => "empty_artist_name",
        EditValidationError::InvalidYear => "invalid_year",
    }
}

/// How a failure's class is stored: its name, and for a class that carries a
/// reason, counts or a receiver's name, each after a dot. A receiver's name
/// comes last, since it may hold dots itself.
fn category_column(category: UiErrorCategory) -> String {
    use UiErrorCategory as C;
    let class = category.class().name();
    match category {
        C::MetadataTrackCount { folder_tracks, release_tracks } => {
            format!("{class}.{folder_tracks}.{release_tracks}")
        }
        C::TiedMediums { sides } => {
            let sides: Vec<String> = sides
                .iter()
                .map(|side| match side {
                    SideHeading::Sided { side_letter } => format!("side:{side_letter}"),
                    SideHeading::Disc { disc } => format!("disc:{disc}"),
                })
                .collect();
            format!("{class}.{}", sides.join("."))
        }
        C::AirPlayBackOff { receiver, seconds } => format!("{class}.{seconds}.{receiver}"),
        C::AirPlayPairingFailed { receiver }
        | C::AirPlayLockedOut { receiver }
        | C::AirPlayBusy { receiver }
        | C::AirPlayNotPairing { receiver }
        | C::AirPlayFull { receiver }
        | C::AirPlayUnreachable { receiver }
        | C::AirPlayHomeOnly { receiver }
        | C::AirPlayAdminsOnly { receiver }
        | C::AirPlayOwnerOnly { receiver } => format!("{class}.{receiver}"),
        C::GroupingBlocked(reason) => format!("{class}.{}", block_column(reason)),
        C::InvalidEdit(reason) => format!("{class}.{}", edit_column(reason)),
        C::Storage(failure) | C::S3VersioningUnknown(failure) => {
            format!("{class}.{}", storage_failure_json(&failure))
        }
        C::StorageCheck { check, failure } => {
            format!("{class}.{}.{}", check_column(check), storage_failure_json(&failure))
        }
        C::S3VersioningUnsupported(state) => format!("{class}.{}", versioning_column(state)),
        C::ReplacementUnfinished(by) => {
            format!("{class}.{}", serde_json::to_string(&by).expect("a replacing call serializes"))
        }
        C::UnfinishedInstallationUnreadable { library_id } => format!("{class}.{library_id}"),
        _ => class.to_string(),
    }
}

fn storage_failure_json(failure: &coven::StorageFailure) -> String {
    serde_json::to_string(failure).expect("storage failure serializes")
}

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
fn versioning_column(value: coven::S3BucketVersioning) -> &'static str {
    match value {
        coven::S3BucketVersioning::Disabled => "Disabled",
        coven::S3BucketVersioning::Enabled => "Enabled",
        coven::S3BucketVersioning::Suspended => "Suspended",
        coven::S3BucketVersioning::Unavailable => "Unavailable",
    }
}
fn versioning_of(value: &str) -> Option<coven::S3BucketVersioning> {
    Some(match value {
        "Disabled" => coven::S3BucketVersioning::Disabled,
        "Enabled" => coven::S3BucketVersioning::Enabled,
        "Suspended" => coven::S3BucketVersioning::Suspended,
        "Unavailable" => coven::S3BucketVersioning::Unavailable,
        _ => return None,
    })
}

fn category_of(column: &str) -> Result<UiErrorCategory, DbError> {
    use bae_diagnostics::UiErrorClass as K;
    use GroupingBlockReason as B;
    use UiErrorCategory as C;
    const WHAT: &str = "import_candidate_pane_failure.category";
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
        K::InvalidEdit => C::InvalidEdit(
            [
                EditValidationError::EmptyAlbumTitle,
                EditValidationError::NoAlbumArtist,
                EditValidationError::EmptyArtistName,
                EditValidationError::InvalidYear,
            ]
            .into_iter()
            .find(|reason| edit_column(*reason) == stored)
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
        K::S3VersioningUnsupported => {
            C::S3VersioningUnsupported(versioning_of(stored).ok_or_else(unreadable)?)
        }
        K::ReplacementUnfinished => C::ReplacementUnfinished(
            serde_json::from_str(stored).map_err(|error| unreadable_because(&error))?,
        ),
        K::UnfinishedInstallationUnreadable => {
            C::UnfinishedInstallationUnreadable { library_id: stored.to_owned() }
        }
        K::TiedMediums => C::TiedMediums {
            sides: stored
                .split('.')
                .map(|side| match side.split_once(':') {
                    Some(("side", side_letter)) => {
                        Ok(SideHeading::Sided { side_letter: side_letter.to_owned() })
                    }
                    Some(("disc", disc)) => disc
                        .parse()
                        .map(|disc| SideHeading::Disc { disc })
                        .map_err(|error| unreadable_because(&error)),
                    _ => Err(unreadable()),
                })
                .collect::<Result<_, _>>()?,
        },
        K::AirPlayBackOff => {
            let (seconds, receiver) = stored.split_once('.').ok_or_else(unreadable)?;
            C::AirPlayBackOff {
                receiver: receiver.to_owned(),
                seconds: seconds.parse().map_err(|error| unreadable_because(&error))?,
            }
        }
        K::AirPlayPairingFailed => C::AirPlayPairingFailed { receiver: stored.to_owned() },
        K::AirPlayLockedOut => C::AirPlayLockedOut { receiver: stored.to_owned() },
        K::AirPlayBusy => C::AirPlayBusy { receiver: stored.to_owned() },
        K::AirPlayNotPairing => C::AirPlayNotPairing { receiver: stored.to_owned() },
        K::AirPlayFull => C::AirPlayFull { receiver: stored.to_owned() },
        K::AirPlayUnreachable => C::AirPlayUnreachable { receiver: stored.to_owned() },
        K::AirPlayHomeOnly => C::AirPlayHomeOnly { receiver: stored.to_owned() },
        K::AirPlayAdminsOnly => C::AirPlayAdminsOnly { receiver: stored.to_owned() },
        K::AirPlayOwnerOnly => C::AirPlayOwnerOnly { receiver: stored.to_owned() },
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

/// The failure the pane states for `candidate`'s last command, or `None`
/// when that command did not fail.
pub(super) fn load_pane_failure_on(
    sql: &SqlReadContext<'_>,
    candidate: OneCandidate<'_>,
) -> Result<Option<PaneFailure>, DbError> {
    let filter = candidate.filter("content_hash");
    sql.query_row(
        &format!(
            "SELECT command, category, detail FROM import_candidate_pane_failure {}",
            filter.clause
        ),
        filter.params().as_slice(),
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
    )
    .optional()?
    .map(|(command, category, detail)| {
        Ok(PaneFailure {
            command: command_of(&command)?,
            error: UiError::Diagnostic { category: category_of(&category)?, detail },
        })
    })
    .transpose()
}

/// Forget the failure the pane states for `content_hash`'s last command.
pub(super) fn delete_pane_failure(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.execute(
        "DELETE FROM import_candidate_pane_failure WHERE content_hash = ?",
        [content_hash],
    )?;
    Ok(())
}

impl ImportTables {
    /// Store `failure` as the failure the pane states for `content_hash`'s
    /// last command, or none.
    pub async fn save_import_pane_failure(
        &self,
        content_hash: &str,
        failure: Option<PaneFailure>,
    ) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        self.store
            .call(move |sql| {
                let Some(PaneFailure { command, error }) = failure else {
                    return delete_pane_failure(sql, &content_hash);
                };
                let UiError::Diagnostic { category, detail } = error;
                let affected = sql.execute(
                    "INSERT INTO import_candidate_pane_failure \
                         (content_hash, command, category, detail) \
                     SELECT ?1, ?2, ?3, ?4 \
                     WHERE EXISTS (SELECT 1 FROM import_candidate_state WHERE content_hash = ?1) \
                     ON CONFLICT (content_hash) DO UPDATE SET \
                         command = excluded.command, \
                         category = excluded.category, \
                         detail = excluded.detail",
                    params![
                        content_hash,
                        command_column(command),
                        category_column(category),
                        detail
                    ],
                )?;
                // Written under the folder-state lock, after a read under it
                // found the candidate listed, and a listed candidate holds a
                // state row.
                assert!(affected == 1, "the pane's failure has no candidate state row to hang off");
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "pane_failure_rows_tests.rs"]
mod tests;
