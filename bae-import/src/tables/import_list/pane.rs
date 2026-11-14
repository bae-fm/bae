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
use crate::tables::folder_scans::stored_cover_choice;
use crate::tables::import_state::{
    load_album_pick_on, load_pane_rows_on, load_standing_on, load_states_on,
};
use crate::tables::release_groupings::{load_candidate_on, skipped_on};
use crate::tables::source_releases::{claimed_records_on, load_source_release_on};
use crate::{DraftProjection, ProjectedStage};
use crate::{ImportedRelease, ReleaseLink};
use bae_identify::PressingLink;
use bae_identify::VerdictSummary;
use bae_records::records::release_records::{album_records_name_on, check_releases_in_library_on};

#[derive(Clone, PartialEq)]
pub(super) struct CandidateDetailRows {
    candidate: FolderCandidate,
    source_error: Option<crate::GroupingBlock>,
    actionable: bool,
    skipped: bool,
    current: Option<DbImportCandidateState>,
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
                "SELECT {LISTED_COLUMNS} FROM import_list_placement AS p \
                 WHERE p.key = ? AND p.item = 'candidate'"
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
    let actionable = stored.actionable;
    let source_error = stored.error;
    let candidate = stored.candidate;
    let content_hash = candidate.files.content_hash();

    let skipped = skipped_on(sql, &candidate)?;
    let standing = load_standing_on(sql, key)?.facts();

    let imported = sql
        .query_row(
            "SELECT r.id, r.album_id \
             FROM local_release_origin o JOIN releases r ON r.id = o.release_id \
             WHERE o.content_hash = ?",
            params![content_hash],
            |row| Ok(ImportedRelease { release_id: row.get(0)?, album_id: row.get(1)? }),
        )
        .optional()?;

    let state =
        load_states_on(sql, crate::tables::import_state::CandidateRows::One(&content_hash))?
            .remove(&content_hash);
    let current =
        state.filter(|state| state.file_edits.is_at_revision(candidate.file_edit_revision));
    // An imported candidate holds no draft: its pane reads the library
    // release, and the folder's files from its scan.
    let stage = match imported {
        Some(release) => StageRows::Imported(release),
        None => StageRows::Drafting(Box::new(draft_rows_on(
            sql,
            &candidate,
            &content_hash,
            current.as_ref(),
        )?)),
    };
    Ok(Some(CandidateDetailRows {
        candidate,
        source_error,
        actionable,
        skipped,
        current,
        standing,
        listed,
        stage,
    }))
}

/// The draft of the candidate `content_hash` and what its pane draws beside
/// it: the linked releases' records and covers, the album an import files
/// under, and what the library holds for the draft's credits.
fn draft_rows_on(
    sql: &SqlReadContext<'_>,
    candidate: &FolderCandidate,
    content_hash: &str,
    current: Option<&DbImportCandidateState>,
) -> Result<DraftRows, DbError> {
    let release_link = current.and_then(|state| state.release_link.clone());
    let pane_rows = load_pane_rows_on(sql, content_hash)?;
    // Read here, inside the pane's live query, so an artist another import
    // commits — or any artist row that changes — reads the credits again.
    let artist_resolutions = bae_records::records::artist_resolution::resolve_credits_with(
        pane_rows.draft.credits(),
        |credit| bae_records::records::artist_resolution::resolve_credit_on(sql, credit),
    )?;
    // The linked pressing's stored releases: what names its records and
    // checks it against the library.
    let claimed =
        claimed_releases_on(sql, candidate, release_link.as_ref().and_then(ReleaseLink::pressing))?;
    // Every catalog record of what the candidate is linked to, merged in one
    // statement over the stored releases: the linked pressing's releases
    // describe it, an album names its own.
    let records = match current {
        Some(_) => claimed_records_on(sql, content_hash)?,
        None => Vec::new(),
    };
    let cover_options = offered_covers(current, release_link.as_ref(), &claimed);
    let picked_library_status = match claimed.first() {
        Some(release) => {
            check_releases_in_library_on(sql, &[bae_records::LibraryCheck::of_release(release)])?
                .into_iter()
                .next()
        }
        None => None,
    };
    let album = candidate_album_on(sql, content_hash, &records)?;
    let cover = pane_rows
        .cover
        .as_ref()
        .map(|selection| {
            stored_cover_choice(sql, &candidate.watched_folder_path, &candidate.key(), selection)
        })
        .transpose()?;
    Ok(DraftRows {
        pane_rows,
        cover_options,
        artist_resolutions,
        records,
        picked_library_status,
        album,
        cover,
    })
}

/// The album an import of the candidate `content_hash`, linked to what
/// `records` describe, files under: the one picked, else the one its records
/// name, else a new one.
fn candidate_album_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
    records: &[bae_catalogs::records::ReleaseRecord],
) -> Result<crate::DraftAlbum, DbError> {
    use crate::DraftAlbum;
    let title = |album_id: &str| -> Result<String, DbError> {
        sql.query_row("SELECT title FROM albums WHERE id = ?", params![album_id], |row| row.get(0))
            .map_err(DbError::from)
    };
    if let Some(album_id) = load_album_pick_on(sql, content_hash)? {
        return Ok(DraftAlbum::Picked { title: title(&album_id)?, album_id });
    }
    Ok(match album_records_name_on(sql, records, &[])? {
        Some(album_id) => DraftAlbum::Named { title: title(&album_id)?, album_id },
        None => DraftAlbum::New,
    })
}

impl CandidateDetailRows {
    pub(super) fn process(self) -> Result<ImportCandidateDetailProjection, crate::ImportError> {
        let Self { candidate, source_error, actionable, skipped, current, standing, listed, stage } =
            self;
        let listed = listed.listed(&candidate.key())?;
        let signals = current.as_ref().and_then(|state| state.signals.clone());
        let lookup_choices =
            current.as_ref().map(|state| state.lookup_choices.clone()).unwrap_or_default();

        let audio = crate::probe::audio_facts(&candidate.files)?;
        let mut verdict = None;
        let mut resumed_identify_state = bae_identify::IdentifyState::Idle;
        let identify = current.as_ref().and_then(|state| state.identify.as_ref());
        if let Some(identify) = identify {
            verdict = Some(VerdictSummary::of(&identify.verdict, identify.kept_own_draft));
            // The candidate's own text is what the rows are judged and ordered
            // against, live or resumed: striking a number out re-orders the
            // rows the next time they are read, with nothing asked again.
            resumed_identify_state = identify.verdict.clone().resume_state(
                signals.as_ref(),
                &lookup_choices,
                audio.clone(),
            );
        }
        let stage = match stage {
            StageRows::Imported(release) => ProjectedStage::Imported {
                release,
                images: crate::mapping::folder_images(&candidate.files),
            },
            StageRows::Drafting(draft) => {
                let DraftRows {
                    pane_rows,
                    cover_options,
                    artist_resolutions,
                    records,
                    picked_library_status,
                    album,
                    cover,
                } = *draft;
                let durations = crate::probe::source_durations(&candidate.files)?;
                let pane = crate::pane::draft_pane(&candidate.files, &durations, &pane_rows.draft)?;
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
                    metadata_draft: pane.edit,
                    artist_resolutions,
                    mapping: pane.mapping,
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
            source_error,
            actionable,
            skipped,
            resumed_identify_state,
            verdict,
            signals,
            standing,
            listed,
            stage,
        })
    }
}

/// Read every release the linked pressing claims in the pane's SQL snapshot,
/// the primary first and then its partners. None for a candidate linked to no
/// pressing.
///
/// The primary is what the draft was read from and what the pane leads with;
/// together with the partners it is what the release's records are read off.
/// A stored pick always has its releases — the pick write fetches them first,
/// for the primary and every partner alike — so a missing one is stated
/// rather than served as half a pane.
fn claimed_releases_on(
    sql: &SqlReadContext<'_>,
    candidate: &FolderCandidate,
    link: Option<&PressingLink>,
) -> Result<Vec<bae_catalogs::source_release::SourceRelease>, DbError> {
    let Some(link) = link else {
        return Ok(Vec::new());
    };
    link.claimed()
        .map(|release| {
            load_source_release_on(sql, release)?.ok_or_else(|| {
                DbError::unreadable(
                    format!("the source release {} claimed for {}", release.key, candidate.key()),
                    None::<String>,
                )
            })
        })
        .collect()
}

/// The covers the picker offers: those the linked pressing's releases offer
/// (see [`bae_catalogs::source_release::pick_covers`]), read from the stored
/// releases as they stand, while the draft is the one read from that
/// pressing. A draft read from the files' tags, typed in or cleared, or a
/// candidate linked to an album or to nothing, is offered none.
fn offered_covers(
    current: Option<&DbImportCandidateState>,
    link: Option<&ReleaseLink>,
    claimed: &[bae_catalogs::source_release::SourceRelease],
) -> Vec<bae_catalogs::remote_image::RemoteCover> {
    let read_from_link = match (
        current.and_then(|state| state.metadata_provenance.as_ref()),
        link.and_then(ReleaseLink::pressing),
    ) {
        (Some(crate::MetadataProvenance::ExternalRelease { record }), Some(pressing)) => {
            *record == pressing.record
        }
        _ => false,
    };
    match claimed.split_first() {
        Some((primary, partners)) if read_from_link => {
            bae_catalogs::source_release::pick_covers(primary, partners)
        }
        _ => Vec::new(),
    }
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
