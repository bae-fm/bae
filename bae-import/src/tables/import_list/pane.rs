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
    album_pick_sql, current_at_scan_sql, load_candidate_verdict_cards_on, load_current_state_on,
    load_pane_rows_on, load_standing_on, stored_verdict_view, OneCandidate, SCANNED_HASH,
};
use crate::tables::import_state::{claimed_records_of_json, claimed_records_sql};
use crate::tables::release_groupings::{load_candidate_on, skipped_on};
use crate::ImportedRelease;
use crate::{DraftProjection, ProjectedStage};
use bae_records::records::release_records::{
    album_records_name_sql, library_status_of, library_status_sql,
};

#[derive(Clone, PartialEq)]
pub(super) struct CandidateDetailRows {
    candidate: FolderCandidate,
    files: crate::PaneFiles,
    /// How many folders the candidate's files come from, when they come
    /// from several; 0 when they come from one folder.
    part_count: u64,
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
    album_artist_standing: Option<crate::ArtistFieldStanding>,
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
    let Some((listed, part_count)) = sql
        .query_row(
            &format!(
                "SELECT {LISTED_COLUMNS}, {actions}, \
                        (SELECT COUNT(*) FROM scan_candidate AS candidate \
                         JOIN scan_candidate_part AS part \
                           ON part.watched_folder_path = candidate.watched_folder_path \
                          AND part.candidate_path = candidate.path \
                         WHERE candidate.path = p.key) AS part_count \
                 FROM import_list_placement AS p \
                 LEFT JOIN import_candidate_standing AS st ON st.candidate_key = p.key \
                 WHERE p.key = ? AND p.item = 'candidate'",
                actions = super::actions::action_columns(),
            ),
            [key],
            |row| {
                Ok((
                    ListedColumns::read(row, 0)?,
                    bae_store::read::count_column::<u64>(row, "part_count")?,
                ))
            },
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
        part_count,
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
/// beside it: the linked releases' records, its cover, the album an import
/// files under, and how the draft's album artists stand to the library. Each
/// statement binds only `key` and reads the candidate's content hash from
/// the scan's row itself.
fn draft_rows_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
) -> Result<DraftRows, DbError> {
    let pane_rows = load_pane_rows_on(sql, OneCandidate::Scanned(key))?;
    let mapping = super::mapping_rows::load_mapping_rows_on(sql, key)?;
    // Read here, inside the pane's live query, so an artist another import
    // commits, or any artist row that changes, reads the credits again. The
    // mapping statement reads each track's own artists' standing the same way.
    let album_artist_standing = super::artist_standings::album_artist_standing_on(
        sql,
        super::artist_standings::CreditSource::Candidate(OneCandidate::Scanned(key)),
    )?;
    // Every catalog record of what the candidate is linked to, as the write
    // that stored the link, or a claimed release, merged them: the linked
    // pressing's releases describe it, an album names its own.
    let records =
        claimed_records_of_json(&while_current_on(sql, key, &claimed_records_sql(SCANNED_HASH))?)?;
    let picked_library_status = picked_library_status_on(sql, key)?;
    let album = candidate_album_on(sql, key)?;
    let cover = candidate_cover_choice_on(sql, key)?;
    Ok(DraftRows {
        pane_rows,
        mapping,
        album_artist_standing,
        records,
        picked_library_status,
        album,
        cover,
    })
}

/// The JSON array that the statement `array` returns for the release the
/// scan stores at `key`, while the candidate's state is the one its scanned
/// files hold (its edit revision is the scan's), and `[]` otherwise: the
/// pane shows a link's records only for the current state.
fn while_current_on(
    sql: &SqlReadContext<'_>,
    key: &crate::CandidateKey,
    array: &str,
) -> Result<String, DbError> {
    Ok(sql.query_row(
        &format!(
            "SELECT CASE WHEN EXISTS ( \
                 SELECT 1 FROM import_candidate_state AS state WHERE {current}) \
             THEN ({array}) ELSE '[]' END",
            current = current_at_scan_sql("state"),
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
    let named = album_records_name_sql(
        &format!(
            "SELECT key, value ->> 'catalog', value ->> 'kind', value ->> 'key', \
                    value ->> 'album' \
             FROM json_each(({claimed}))",
            claimed = claimed_records_sql(SCANNED_HASH),
        ),
        "'[]'",
    );
    let row = sql.query_row(
        &format!(
            "WITH pick AS ({pick}), \
             choice (kind, album_id) AS ( \
                 SELECT pick.pick, pick.album_id FROM pick WHERE pick.pick <> 'records' \
                 UNION ALL \
                 SELECT 'named', named.id \
                 FROM ({named}) AS named JOIN pick ON pick.pick = 'records') \
             SELECT COALESCE(choice.kind, 'none'), a.id, a.title, {credits} \
             FROM (SELECT 1) LEFT JOIN choice LEFT JOIN albums AS a ON a.id = choice.album_id",
            pick = album_pick_sql(SCANNED_HASH),
            credits = bae_records::records::query::album_credits_json_sql(),
        ),
        named_params! { ":key": key },
        crate::tables::draft_rows::DraftAlbumRow::of_row,
    )?;
    row.album(&key.to_string())
}

impl CandidateDetailRows {
    pub(super) fn process(self) -> Result<ImportCandidateDetailProjection, crate::ImportError> {
        let Self {
            candidate,
            files,
            part_count,
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
        let stored_verdict = stored_verdict_view(current.as_ref(), cards);
        let stage = match stage {
            StageRows::Imported(release) => ProjectedStage::Imported { release },
            StageRows::Drafting(draft) => {
                let DraftRows {
                    pane_rows,
                    mapping,
                    album_artist_standing,
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
                    album_artist_standing,
                    mapping,
                    cover,
                    lookup_choices,
                    failure: pane_rows.failure,
                    pane_failure: pane_rows.pane_failure,
                }))
            }
        };
        Ok(ImportCandidateDetailProjection {
            candidate,
            files,
            part_count,
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
    /// the pane that shows its details reads it, live; `None` for no key, or
    /// a key naming no such folder, which clears a selection. Each statement
    /// in `load_invalid_detail` keeps only rows whose folder the scan stored
    /// as invalid, so the read returns no rows for a key of any other kind.
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
            Some(key) => folder_scans::load_invalid_detail(&sql, key),
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
