//! One candidate as the pane reads it, whole: its folder and files, its
//! draft and the releases it is linked to, what is running for it, and where
//! the list places it, which the pane reads from the list's placement as the
//! list's rows do. And a folder that failed validation, read whole for the
//! pane that looks into it.

use super::listed::ListedColumns;
use super::*;
use crate::candidate_detail::ImportCandidateDetailProjection;
use crate::cover_art::CoverChoice;
use crate::folder_scanner::FolderCandidate;
use crate::tables::import_state::fragments::PickedLibraryStatus;
use crate::tables::import_state::{file_evidence_of_json, stored_verdict_view};
use crate::ImportedRelease;
use crate::{DraftProjection, ProjectedStage};
use bae_store::{Bound, ReadStatement};

use super::pane_read::{
    PaneDraft, PaneDraftParams, PaneDraftRow, PaneHead, PaneHeadParams, PaneHeadRow,
};

/// The candidate the pane is set to, as the scan, the list and the candidate's
/// state hold it: everything the pane shows of it but what it files under.
struct HeadRows {
    candidate: FolderCandidate,
    files: crate::PaneFiles,
    /// How many folders the candidate's files come from, when they come
    /// from several; 0 when they come from one folder.
    part_count: u64,
    source_error: Option<crate::GroupingBlock>,
    actionable: bool,
    skipped: bool,
    current: Option<DbImportCandidateState<bae_identify::signals::PaneSignals>>,
    /// The Disc IDs and barcodes that name a file of the candidate.
    file_evidence: Vec<crate::FileEvidence>,
    /// The cards the stored verdict's rows hold.
    cards: bae_identify::VerdictCards,
    standing: crate::TriageRuntimeFacts,
    listed: ListedColumns,
}

/// A candidate's draft and the rows its pane draws beside it.
struct DraftRows {
    draft: crate::CandidateDraft,
    draft_blank: bool,
    draft_valid: bool,
    failure: Option<crate::ImportFailure>,
    pane_failure: Option<crate::PaneFailure>,
    mapping: super::mapping_rows::MappingRows,
    album_artists: Option<crate::ArtistField>,
    records: Vec<bae_catalogs::records::ReleaseRecord>,
    picked_library_status: Option<LibraryStatus>,
    album: crate::DraftAlbum,
    /// The cover the candidate commits with: its stored selection, with the
    /// image its scan stored for it.
    cover: Option<CoverChoice>,
}

/// One candidate, whole, with its standing and where the list places it.
struct CandidateDetailRows {
    head: HeadRows,
    stage: StageRows,
}

/// What the candidate is past its folder: the release this device imported
/// from its files, or the draft it holds.
enum StageRows {
    Imported(ImportedRelease),
    Drafting(Box<DraftRows>),
}

impl HeadRows {
    /// The candidate the pane's head read returned.
    fn of(row: PaneHeadRow) -> Result<Self, DbError> {
        let PaneHeadRow {
            listed,
            scan,
            grouping,
            files,
            standing,
            imported: _,
            state,
            evidence,
            cards,
        } = row;
        let key = listed.listed_key.clone();
        let unlisted =
            || DbError::unreadable(format!("the scan_candidate rows of listed {key}"), 0);
        let part_count = u64::try_from(listed.part_count).map_err(|error| {
            DbError::unreadable("the part count of a listed candidate", error.to_string())
        })?;
        let listed_columns = ListedColumns::of(&listed);
        let scan = folder_scans::CandidateRow::of(scan.ok_or_else(unlisted)?)?;
        let grouping = grouping.ok_or_else(unlisted)?;
        let root = scan.root().to_path_buf();
        let stored = release_groupings::stored_release_candidate(
            &root,
            scan.item()?,
            grouping.taken_in,
            release_groupings::block_of(
                grouping.blocked,
                grouping.blocked_subject,
                grouping.blocked_holder,
            )?,
        )?
        .ok_or_else(unlisted)?;
        let skipped = grouping.skipped.ok_or_else(|| {
            DbError::unreadable(
                format!("the release_grouping row of {key}"),
                &stored.candidate.grouping,
            )
        })?;
        let files = files
            .ok_or_else(|| DbError::unreadable(format!("the files of {key}"), None::<String>))?;
        Ok(Self {
            candidate: stored.candidate,
            files: pane_files::pane_files_of_json(&files.pane_files)?,
            part_count,
            source_error: stored.error,
            actionable: stored.actionable,
            skipped,
            current: state.map(import_state::current_state_of).transpose()?,
            file_evidence: file_evidence_of_json(
                &evidence
                    .ok_or_else(|| {
                        DbError::unreadable(
                            format!("the evidence in the files of {key}"),
                            None::<String>,
                        )
                    })?
                    .file_evidence,
            )?,
            cards: import_state::verdict_cards_of_column(
                cards.and_then(|cards| cards.verdict_cards).as_deref(),
            )?,
            standing: import_state::standing_of(standing, &key)?.facts(),
            listed: listed_columns,
        })
    }
}

impl DraftRows {
    /// The draft the pane's draft read returned.
    fn of(row: PaneDraftRow) -> Result<Self, DbError> {
        let key = row
            .pane_key
            .ok_or_else(|| DbError::unreadable("the pane's draft read", None::<String>))?;
        let missing = |what: &str| DbError::unreadable(format!("{what} of {key}"), None::<String>);
        let draft = row
            .draft
            .ok_or_else(|| missing("import_candidate_edit of a candidate not imported here"))?;
        let flags = row.flags.ok_or_else(|| missing("import_candidate_edit of a stored draft"))?;
        let claimed = row
            .claimed
            .and_then(|claimed| claimed.claimed_records)
            .ok_or_else(|| missing("the claimed records"))?;
        Ok(Self {
            draft: import_state::draft_of(&draft)?,
            draft_blank: flags.draft_blank,
            draft_valid: flags.draft_valid,
            failure: row.failure.map(import_state::failure_of).transpose()?,
            pane_failure: row
                .pane_failure
                .map(|failure| {
                    import_state::pane_failure_of(
                        &failure.pane_failure_command,
                        &failure.pane_failure_category,
                        failure.pane_failure_detail,
                    )
                })
                .transpose()?,
            mapping: super::mapping_rows::MappingRows::of_document(
                row.mapping
                    .and_then(|mapping| mapping.mapping)
                    .ok_or_else(|| missing("the mapping"))?,
            ),
            album_artists: row
                .artist_field
                .and_then(|field| field.artist_field)
                .as_deref()
                .map(artist_field_of_json)
                .transpose()?,
            records: import_state::claimed_records_of_json(&claimed)?,
            picked_library_status: match row.status {
                Some(PickedLibraryStatus {
                    picked_status_release_id: Some(release_id),
                    picked_status_albums,
                    ..
                }) => {
                    let albums = picked_status_albums
                        .ok_or_else(|| missing("the library albums of the picked release"))?;
                    Some(bae_records::records::fragments::library_status_of_columns(
                        release_id, &albums,
                    )?)
                }
                _ => None,
            },
            album: crate::tables::draft_rows::DraftAlbumRow::of(
                row.album.ok_or_else(|| missing("the album"))?,
            )?
            .album(&key.to_string())?,
            cover: row.cover.map(|cover| folder_scans::cover_choice_of(cover, &key)).transpose()?,
        })
    }
}

impl From<&Option<crate::CandidateKey>> for PaneHeadParams {
    fn from(key: &Option<crate::CandidateKey>) -> Self {
        Self { key: key.clone() }
    }
}

impl From<&Option<crate::CandidateKey>> for PaneDraftParams {
    fn from(key: &Option<crate::CandidateKey>) -> Self {
        Self { key: key.clone() }
    }
}

fn head_params(request: &PaneHeadParams) -> Bound<'_> {
    request.params()
}

fn draft_params(request: &PaneDraftParams) -> Bound<'_> {
    request.params()
}

/// The pane's head read, for the candidate its request names or none.
static HEAD: ReadStatement<PaneHeadParams, PaneHeadRow> =
    ReadStatement::new(PaneHead::SQL, head_params, PaneHeadRow::read);

/// The pane's draft read, for the same request.
static DRAFT: ReadStatement<PaneDraftParams, PaneDraftRow> =
    ReadStatement::new(PaneDraft::SQL, draft_params, PaneDraftRow::read);

/// What the two reads answered, as one candidate's rows; `None` when the key
/// names no release the list lists.
fn detail_rows(
    (head, draft): (Vec<PaneHeadRow>, Vec<PaneDraftRow>),
) -> Result<Option<CandidateDetailRows>, crate::ImportError> {
    let Some(mut head) = head.into_iter().next() else {
        return Ok(None);
    };
    // An imported candidate holds no draft: its pane reads the library
    // release, and the folder's files from its scan.
    let imported = head.imported.take();
    let head = HeadRows::of(head)?;
    let stage = match imported {
        Some(imported) => {
            let (release_id, album_id) = imported.release.into_ids();
            StageRows::Imported(ImportedRelease { release_id, album_id })
        }
        None => StageRows::Drafting(Box::new(DraftRows::of(
            draft.into_iter().next().ok_or_else(|| {
                DbError::unreadable(
                    "the draft rows of a candidate not imported here",
                    None::<String>,
                )
            })?,
        )?)),
    };
    Ok(Some(CandidateDetailRows { head, stage }))
}

impl CandidateDetailRows {
    pub(super) fn process(self) -> Result<ImportCandidateDetailProjection, crate::ImportError> {
        let Self { head, stage } = self;
        let HeadRows {
            candidate,
            files,
            part_count,
            source_error,
            actionable,
            skipped,
            current,
            file_evidence,
            cards,
            standing,
            listed,
        } = head;
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
                    draft,
                    draft_blank,
                    draft_valid,
                    failure,
                    pane_failure,
                    mapping,
                    album_artists,
                    records,
                    picked_library_status,
                    album,
                    cover,
                } = *draft;
                let super::mapping_rows::MappingRead { table: mapping, track_length_ms } =
                    mapping.read(album_artists.is_some())?;
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
                    metadata_draft: draft.release_edit(),
                    draft_blank,
                    draft_valid,
                    album_artists,
                    mapping,
                    track_length_ms,
                    cover,
                    lookup_choices,
                    failure,
                    pane_failure,
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
            file_evidence,
            standing,
            listed,
            stage,
        })
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
            .subscribe_reconfigurable_statements(initial, &HEAD, &DRAFT)
            .process(|rows| detail_rows(rows)?.map(CandidateDetailRows::process).transpose())
    }

    /// The mapping rows and album artists of the pane's draft read, for a
    /// test.
    #[cfg(test)]
    pub(super) async fn load_pane_mapping_for_test(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<(super::mapping_rows::MappingRows, Option<crate::ArtistField>), crate::ImportError>
    {
        let request = PaneDraftParams::from(&Some(key.clone()));
        let row = self
            .store
            .read(move |sql| DRAFT.run(&sql, &request))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| DbError::unreadable("the pane's draft read", None::<String>))?;
        let draft = DraftRows::of(row)?;
        Ok((draft.mapping, draft.album_artists))
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
        let key = Some(key.clone());
        self.store
            .read(move |sql| {
                Ok((
                    HEAD.run(&sql, &PaneHeadParams::from(&key))?,
                    DRAFT.run(&sql, &PaneDraftParams::from(&key))?,
                ))
            })
            .process(|rows| detail_rows(rows)?.map(CandidateDetailRows::process).transpose())
            .await
    }
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
