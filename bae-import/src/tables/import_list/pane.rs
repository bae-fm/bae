//! One candidate as the pane reads it, whole: its folder and files, its
//! draft and the releases it is linked to, what is running for it, and where
//! the list places it, which the pane reads from the list's placement as the
//! list's rows do. And a folder that failed validation, read whole for the
//! pane that looks into it.

use super::listed::{ListedColumns, LISTED_COLUMNS};
use super::*;
use crate::candidate_detail::ImportCandidateDetailProjection;
use crate::cover_art::CoverChoice;
use crate::folder_scanner::FolderCandidate;
use crate::tables::folder_scans::candidate_cover_choice_on;
use crate::tables::import_state::{
    load_candidate_verdict_cards_on, load_current_state_on, load_pane_rows_on, load_standing_on,
    OneCandidate, SCANNED_HASH,
};
use crate::tables::release_groupings::{load_candidate_on, skipped_on};
use crate::tables::source_releases::{
    claimed_records_of_json, claimed_records_sql, offered_covers_of_json, offered_covers_sql,
};
use crate::ImportedRelease;
use crate::{DraftProjection, ProjectedStage};
use bae_records::records::release_records::{
    album_records_name_sql, library_status_of, library_status_sql,
};

#[derive(Clone, PartialEq)]
pub(super) struct CandidateDetailRows {
    candidate: FolderCandidate,
    files: crate::PaneFiles,
    source_error: Option<crate::GroupingBlock>,
    actionable: bool,
    skipped: bool,
    current: Option<DbImportCandidateState>,
    /// The cards the stored verdict's rows hold.
    cards: bae_identify::VerdictCards,
    standing: crate::TriageRuntimeFacts,
    listed: ListedColumns,
    stage: StageRows,
}

/// What the pane reads past the candidate's folder: its draft and what goes
/// with it, or the release this device imported from its files.
#[derive(Clone, PartialEq)]
enum StageRows {
    Drafting(Box<DraftRows>),
    Imported(ImportedRelease),
}

/// A candidate's draft and the rows its pane draws beside it.
#[derive(Clone, PartialEq)]
struct DraftRows {
    pane_rows: DbCandidatePaneRows,
    mapping: super::mapping_rows::MappingRows,
    cover_options: Vec<bae_catalogs::remote_image::RemoteCover>,
    artist_resolutions: Vec<bae_records::ResolvedCredit>,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    picked_library_status: Option<LibraryStatus>,
    album: crate::DraftAlbum,
    /// The cover the candidate commits with: its stored selection, with the
    /// image its scan stored for it.
    cover: Option<CoverChoice>,
}

/// One candidate, whole, with its standing and where the list places it.
/// `None` when the key names no release the list lists — which is what
/// clears a selection.
pub(super) fn load_candidate_detail_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<Option<CandidateDetailRows>, DbError> {
    let Some(listed) = sql
        .query_row(
            &format!(
                "SELECT {LISTED_COLUMNS}, {actions} FROM import_list_placement AS p \
                 LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
                 WHERE p.key = ? AND p.item = 'candidate'",
                actions = super::actions::action_columns(),
            ),
            [key],
            |row| ListedColumns::read(row, 0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let stored = load_candidate_on(sql, key)?.ok_or_else(|| {
        DbError::unreadable(format!("the scan_candidate rows of listed {key}"), 0)
    })?;
    let skipped = skipped_on(sql, key)?;
    let files = super::pane_files::load_pane_files_on(sql, key)?;
    let standing = load_standing_on(sql, key)?.facts();
    let imported = imported_on(sql, key)?;
    let current = load_current_state_on(sql, key)?;
    let cards = load_candidate_verdict_cards_on(sql, key)?;
    let candidate = stored.candidate;
    // An imported candidate holds no draft: its pane reads the library
    // release, and the folder's files from its scan.
    let stage = match imported {
        Some(release) => StageRows::Imported(release),
        None => StageRows::Drafting(Box::new(draft_rows_on(sql, key)?)),
    };
    Ok(Some(CandidateDetailRows {
        candidate,
        files,
        source_error: stored.error,
        actionable: stored.actionable,
        skipped,
        current,
        cards,
        standing,
        listed,
        stage,
    }))
}

/// The release this device imported from the files of the release the scan
/// stores at `key`.
fn imported_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<Option<ImportedRelease>, DbError> {
    Ok(sql
        .query_row(
            "SELECT r.id, r.album_id FROM scan_candidate AS scan \
             JOIN local_release_origin AS o ON o.content_hash = scan.content_hash \
             JOIN releases AS r ON r.id = o.release_id \
             WHERE scan.path = ?",
            [key],
            |row| Ok(ImportedRelease { release_id: row.get(0)?, album_id: row.get(1)? }),
        )
        .optional()?)
}

/// The draft of the release the scan stores at `key` and what its pane draws
/// beside it: the linked releases' records and covers, the album an import
/// files under, and what the library holds for the draft's credits. Each
/// statement binds only `key` and reads the candidate's content hash from
/// the scan's row itself.
fn draft_rows_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<DraftRows, DbError> {
    let pane_rows = load_pane_rows_on(sql, OneCandidate::Scanned(key))?;
    let mapping = super::mapping_rows::load_mapping_rows_on(sql, key)?;
    // Read here, inside the pane's live query, so an artist another import
    // commits — or any artist row that changes — reads the credits again.
    let artist_resolutions =
        super::credit_resolutions::credit_resolutions_on(sql, OneCandidate::Scanned(key))?;
    // Every catalog record of what the candidate is linked to, merged in one
    // statement over the stored releases: the linked pressing's releases
    // describe it, an album names its own.
    let records = claimed_records_of_json(&while_current_on(
        sql,
        key,
        &claimed_records_sql(SCANNED_HASH),
    )?)?;
    let cover_options =
        offered_covers_of_json(&while_current_on(sql, key, &offered_covers_sql(SCANNED_HASH))?)?;
    let picked_library_status = picked_library_status_on(sql, key)?;
    let album = candidate_album_on(sql, key)?;
    let cover = candidate_cover_choice_on(sql, key)?;
    Ok(DraftRows {
        pane_rows,
        mapping,
        cover_options,
        artist_resolutions,
        records,
        picked_library_status,
        album,
        cover,
    })
}

/// The JSON array that the statement `array` returns for the release the
/// scan stores at `key`, while the candidate's state is the one its scanned
/// files hold (its edit revision is the scan's), and `[]` otherwise: the
/// pane shows a link's records and covers only for the current state.
fn while_current_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
    array: &str,
) -> Result<String, DbError> {
    Ok(sql.query_row(
        &format!(
            "SELECT CASE WHEN EXISTS ( \
                 SELECT 1 FROM import_candidate_state AS state \
                 JOIN scan_candidate AS scan \
                     ON (scan.content_hash, scan.file_edit_revision) \
                      = (state.content_hash, state.edit_revision) \
                 WHERE scan.path = :key) \
             THEN ({array}) ELSE '[]' END"
        ),
        named_params! { ":key": key },
        |row| row.get(0),
    )?)
}

/// The album an import of the candidate `key` files under, read in one
/// statement: the one picked, else the one its link's records name, else a
/// new one.
fn candidate_album_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<crate::DraftAlbum, DbError> {
    use crate::DraftAlbum;
    let named = album_records_name_sql(
        &format!(
            "SELECT key, value ->> 'catalog', value ->> 'kind', value ->> 'key', \
                    value ->> 'album' \
             FROM json_each(({claimed}))",
            claimed = claimed_records_sql(SCANNED_HASH),
        ),
        "'[]'",
    );
    let (kind, album_id, title, credits): (String, Option<String>, Option<String>, String) = sql
        .query_row(
            &format!(
                "WITH pick AS (SELECT album_id FROM import_candidate_album_pick \
                               WHERE content_hash = {SCANNED_HASH}), \
                 choice (kind, album_id) AS ( \
                     SELECT CASE WHEN album_id IS NULL THEN 'picked_new' ELSE 'picked' END, \
                            album_id \
                     FROM pick \
                     UNION ALL \
                     SELECT 'named', named.id FROM ({named}) AS named \
                     WHERE NOT EXISTS (SELECT 1 FROM pick)) \
                 SELECT COALESCE(choice.kind, 'new'), a.id, a.title, {credits} \
                 FROM (SELECT 1) LEFT JOIN choice LEFT JOIN albums AS a ON a.id = choice.album_id",
                credits = bae_records::records::query::album_credits_json_sql(),
            ),
            named_params! { ":key": key },
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let album = |kind: &str| -> Result<(String, String, bae_types::credits::CreditLine), DbError> {
        match (album_id.clone(), title.clone()) {
            (Some(album_id), Some(title)) => {
                Ok((album_id, title, bae_records::records::read::credit_line_of_json(&credits)?))
            }
            _ => Err(DbError::unreadable(format!("the {kind} album of {key}"), None::<String>)),
        }
    };
    Ok(match kind.as_str() {
        "new" => DraftAlbum::New,
        "picked_new" => DraftAlbum::PickedNew,
        "named" => {
            let (album_id, title, artist_line) = album("named")?;
            DraftAlbum::Named { album_id, title, artist_line }
        }
        "picked" => {
            let (album_id, title, artist_line) = album("picked")?;
            DraftAlbum::Picked { album_id, title, artist_line }
        }
        other => return Err(DbError::unreadable("the kind of a candidate's album", other)),
    })
}

impl CandidateDetailRows {
    pub(super) fn process(self) -> Result<ImportCandidateDetailProjection, crate::ImportError> {
        let Self {
            candidate,
            files,
            source_error,
            actionable,
            skipped,
            current,
            cards,
            standing,
            listed,
            stage,
        } = self;
        let listed = listed.listed(&candidate.key())?;
        let signals = current.as_ref().and_then(|state| state.signals.clone());
        let lookup_choices =
            current.as_ref().map(|state| state.lookup_choices.clone()).unwrap_or_default();

        // The verdict write stores each row's judgement against the
        // candidate's own text, so the rows are already in their new order
        // after the person discounts a catalog number.
        let stored_verdict =
            current.as_ref().and_then(|state| state.identify.as_ref()).map(|identify| {
                bae_identify::IdentifyStateView::of(
                    bae_identify::RunOutcome::Answered(identify.verdict.clone()),
                    cards,
                )
            });
        let stage = match stage {
            StageRows::Imported(release) => ProjectedStage::Imported { release },
            StageRows::Drafting(draft) => {
                let DraftRows {
                    pane_rows,
                    mapping,
                    cover_options,
                    artist_resolutions,
                    records,
                    picked_library_status,
                    album,
                    cover,
                } = *draft;
                let mapping = mapping.table()?;
                ProjectedStage::Drafting(Box::new(DraftProjection {
                    metadata_provenance: current
                        .as_ref()
                        .and_then(|state| state.metadata_provenance.clone()),
                    release_link: current.as_ref().and_then(|state| state.release_link.clone()),
                    metadata_author: current
                        .as_ref()
                        .map_or(crate::MetadataAuthor::Nobody, |state| state.metadata_author),
                    records,
                    picked_library_status,
                    album,
                    metadata_draft: pane_rows.draft.release_edit(),
                    draft_blank: pane_rows.draft_blank,
                    draft_valid: pane_rows.draft_valid,
                    artist_resolutions,
                    mapping,
                    cover,
                    remote_covers: cover_options,
                    lookup_choices,
                    failure: pane_rows.failure,
                    session: pane_rows.session,
                }))
            }
        };
        Ok(ImportCandidateDetailProjection {
            candidate,
            files,
            source_error,
            actionable,
            skipped,
            stored_verdict,
            signals,
            standing,
            listed,
            stage,
        })
    }
}

/// Which library albums hold the release the candidate `key` is linked to,
/// the pressing its link names first; `None` for a candidate linked to no
/// pressing. The link's foreign key keeps the release it names stored.
fn picked_library_status_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<Option<LibraryStatus>, DbError> {
    Ok(sql
        .query_row(
            &library_status_sql(
                "SELECT 0, link.release_id, link.source, release.source_group_id \
                 FROM scan_candidate AS scan \
                 JOIN import_candidate_release_link AS link \
                     ON link.content_hash = scan.content_hash AND link.kind = 'pressing' \
                 JOIN source_release AS release \
                     ON (release.catalog, release.release_id) = (link.source, link.release_id) \
                 WHERE scan.path = :key",
            ),
            named_params! { ":key": key },
            library_status_of,
        )
        .optional()?)
}

impl ImportTables {
    /// The candidate the request names, as the pane reads it, live; `None` for
    /// no key or a key naming no release the list lists, which clears a
    /// selection.
    pub(crate) fn subscribe_import_candidate(
        &self,
        initial: Option<crate::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<ImportCandidateDetailProjection>,
        crate::ImportError,
    > {
        self.store
            .subscribe_reconfigurable(initial, |key, sql| match key {
                None => Ok(None),
                Some(key) => Ok(pane::load_candidate_detail_on(&sql, key)?),
            })
            .process(|process| process.map(pane::CandidateDetailRows::process).transpose())
    }

    /// The folder that failed validation at the key the read is set to, as
    /// the pane looking into it reads it, live; `None` for no key, or a key
    /// naming no such folder, which clears a selection. Only a folder stored
    /// as invalid is read whole: any other key is answered by its kind.
    pub(crate) fn subscribe_invalid_candidate(
        &self,
        initial: Option<crate::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<crate::InvalidCandidateDetail>,
        bae_store::DbError,
    > {
        self.store.subscribe_reconfigurable(initial, |key, sql| match key {
            None => Ok(None),
            Some(key) => {
                let invalid: bool = sql.query_row(
                    "SELECT EXISTS (SELECT 1 FROM scan_candidate \
                                        WHERE path = ? AND kind = 'invalid')",
                    [key],
                    |row| row.get(0),
                )?;
                if !invalid {
                    return Ok(None);
                }
                let stored =
                    folder_scans::load_item_by_key(&sql, key, folder_scans::RowSources::Any)?;
                Ok(match stored.map(|(_, stored)| stored.item) {
                    Some(crate::folder_scanner::ScanItem::Invalid(candidate)) => {
                        Some(crate::InvalidCandidateDetail::of(candidate))
                    }
                    _ => None,
                })
            }
        })
    }

    /// One candidate as the pane reads it, once.
    pub(crate) async fn load_import_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<ImportCandidateDetailProjection>, crate::ImportError> {
        let key = key.clone();
        self.store
            .read(move |sql| pane::load_candidate_detail_on(&sql, &key))
            .process(|process| process.map(pane::CandidateDetailRows::process).transpose())
            .await
    }
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
