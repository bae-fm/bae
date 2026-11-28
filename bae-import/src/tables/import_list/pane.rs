//! One candidate as the pane reads it, whole: its folder and files, its
//! draft and the releases it is linked to, what is running for it, and where
//! the list places it, which the pane reads from the list's placement as the
//! list's rows do. And a folder that failed validation, read whole for the
//! pane that looks into it.

use super::listed::ListedColumns;
use super::*;
use crate::candidate_detail::{
    CandidateDraftPane, CandidateImportStatus, CandidatePanePlacement, CandidateStage,
    ImportCandidateDetail,
};
use crate::cover_art::CoverChoice;
use crate::folder_scanner::FolderCandidate;
use crate::tables::import_state::fragments::PickedLibraryStatus;
use crate::tables::import_state::{file_evidence_of_json, stored_verdict_view};
use crate::triage::{import_status_of, stated_folder_check, CandidateLiveState, TriagePlacement};
use crate::MetadataProvenance;
use crate::{ImportedRelease, TriageTab};
use bae_identify::IdentifyStateView;

use super::pane_read::{
    PaneDraftParams, PaneDraftRow, PaneDraftRows, PaneHeadParams, PaneHeadRow, PaneHeadRows,
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
    /// The stretch of a file the preview player holds, which marks the
    /// mapping row that plays it.
    previewed: Option<bae_track_audio::PreviewTarget>,
}

/// One candidate, whole, with its standing and where the list places it.
pub(super) struct CandidateDetailRows {
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
    /// The candidate the pane's head returned.
    fn of(row: PaneHeadRows) -> Result<Self, DbError> {
        let PaneHeadRows {
            listed,
            scan,
            grouping,
            files,
            standing,
            imported,
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
        let listed_columns = ListedColumns::of(&listed)?;
        let entry = release_groupings::CandidateEntry::of(
            scan.ok_or_else(unlisted)?,
            grouping,
            standing,
            imported,
        )?
        .ok_or_else(unlisted)?;
        let actionable = entry.actionable();
        let files = files
            .and_then(|files| files.pane_files)
            .ok_or_else(|| DbError::unreadable(format!("the files of {key}"), None::<String>))?;
        let (current, cards) = import_state::current_state_and_cards_of(state, cards)?;
        Ok(Self {
            candidate: entry.candidate,
            files: pane_files::pane_files_of_json(&files)?,
            part_count,
            source_error: entry.block,
            actionable,
            skipped: entry.skipped,
            current,
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
            cards,
            standing: entry.runtime,
            listed: listed_columns,
        })
    }
}

impl DraftRows {
    /// The draft the pane's draft returned.
    fn of(row: PaneDraftRows) -> Result<Self, DbError> {
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
            failure: row
                .failure
                .map(|failure| import_state::failure_of(failure.failure_error, &failure.failed_at))
                .transpose()?,
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
            previewed: row.previewed.map(|previewed| previewed.target()).transpose()?,
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

/// What the pane's two reads answered at its one key, as that candidate's
/// rows; `None` when the key names no release the list lists.
fn pane_rows(
    (head, draft): (Vec<PaneHeadRow>, Vec<PaneDraftRow>),
) -> Result<Option<CandidateDetailRows>, crate::ImportError> {
    let Some(head) = head.into_iter().next() else {
        return Ok(None);
    };
    detail_rows(head.head, draft.into_iter().next().map(|row| row.draft)).map(Some)
}

/// One candidate's rows, from its head and its draft, which a candidate not
/// imported here holds.
pub(super) fn detail_rows(
    head: PaneHeadRows,
    draft: Option<PaneDraftRows>,
) -> Result<CandidateDetailRows, crate::ImportError> {
    // An imported candidate holds no draft: its pane reads the library
    // release, and the folder's files from its scan.
    let imported = head.imported.clone();
    let head = HeadRows::of(head)?;
    let stage = match imported {
        Some(imported) => {
            let (release_id, album_id) = imported.release.into_ids();
            StageRows::Imported(ImportedRelease { release_id, album_id })
        }
        None => StageRows::Drafting(Box::new(DraftRows::of(draft.ok_or_else(|| {
            DbError::unreadable("the draft rows of a candidate not imported here", None::<String>)
        })?)?)),
    };
    Ok(CandidateDetailRows { head, stage })
}

impl CandidateDetailRows {
    /// The pane's value, shaped from the rows of one candidate's reads: the
    /// candidate with the commands and state its standing gives it.
    pub(super) fn process(self) -> Result<ImportCandidateDetail, crate::ImportError> {
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
        // The verdict write stores each row's judgement against the
        // candidate's own text, so the rows are already in their new order
        // after the person discounts a catalog number.
        let stored_verdict = stored_verdict_view(current.as_ref(), cards);
        let import_step = standing.import;
        let (stored_status, placement, stage) = match stage {
            StageRows::Imported(release) => (
                import_status_of(Some(&release), source_error.as_ref(), None),
                CandidatePanePlacement::Done,
                CandidateStage::Imported { release },
            ),
            StageRows::Drafting(draft) => {
                let status = import_status_of(
                    None,
                    source_error.as_ref(),
                    draft.failure.as_ref().map(|failure| &failure.reason),
                );
                let (placement, pane) = draft.pane(PaneStanding {
                    placement: listed.basis.placement,
                    actionable,
                    identifying: standing.identifying(),
                    importing: standing.importing(),
                    current,
                    stored_verdict: stored_verdict.as_ref(),
                })?;
                (status, placement, CandidateStage::Drafting(Box::new(pane)))
            }
        };
        let import_status = match import_step {
            Some(step) => Some(CandidateImportStatus::Importing { step }),
            None => stored_status.map(CandidateImportStatus::of),
        };
        Ok(ImportCandidateDetail {
            candidate,
            files,
            part_count,
            actionable,
            skipped,
            // A candidate with no stored verdict shows no run.
            resumed_identify_state: stored_verdict.unwrap_or(IdentifyStateView::Idle),
            placement,
            live: CandidateLiveState::of(listed.actions, standing, listed.standing),
            import_status,
            file_evidence,
            signals,
            stage,
        })
    }
}

/// What the rest of a candidate's rows say about the draft it holds.
struct PaneStanding<'a> {
    placement: TriagePlacement,
    actionable: bool,
    identifying: bool,
    importing: bool,
    current: Option<DbImportCandidateState<bae_identify::signals::PaneSignals>>,
    stored_verdict: Option<&'a IdentifyStateView>,
}

impl DraftRows {
    /// Where the pane places a candidate that holds this draft, and what it
    /// draws of the draft.
    fn pane(
        self,
        standing: PaneStanding<'_>,
    ) -> Result<(CandidatePanePlacement, CandidateDraftPane), crate::ImportError> {
        let Self {
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
            previewed,
        } = self;
        let PaneStanding { placement, actionable, identifying, importing, current, stored_verdict } =
            standing;
        let (metadata_provenance, release_link, metadata_author, lookup_choices) = match current {
            Some(state) => (
                state.metadata_provenance,
                state.release_link,
                state.metadata_author,
                state.lookup_choices,
            ),
            None => (None, None, crate::MetadataAuthor::Nobody, Default::default()),
        };
        // The check against the folder the stored verdict's found release
        // failed, as the verdict's write stored it.
        let verdict_folder_check = match stored_verdict {
            Some(IdentifyStateView::Found { decision, .. }) => decision.folder_check().cloned(),
            _ => None,
        }
        .filter(|_| actionable);
        let opens_on = crate::MetadataPresentation::opening(
            identifying,
            stored_verdict.is_some(),
            metadata_provenance.is_some() || release_link.is_some(),
            stated_folder_check(metadata_author, draft_valid, verdict_folder_check.clone())
                .is_some(),
        );
        // The catalogs the linked release is described in, as the row names
        // them: only a linked candidate names any.
        let linked_records = (actionable && release_link.is_some()).then_some(records);
        let pane_placement = match placement.tab() {
            TriageTab::Pending => CandidatePanePlacement::Pending {
                folder_check: (placement == TriagePlacement::Pending)
                    .then(|| {
                        stated_folder_check(metadata_author, draft_valid, verdict_folder_check)
                    })
                    .flatten(),
                records: linked_records,
            },
            TriageTab::Skipped => CandidatePanePlacement::Skipped { records: linked_records },
            TriageTab::Done => CandidatePanePlacement::Done,
        };
        let super::mapping_rows::MappingRead { table: mapping, track_length_ms, left_out_audio } =
            mapping.read(album_artists.is_some(), previewed.as_ref())?;
        let metadata_draft = draft.release_edit();
        let pane = CandidateDraftPane {
            picked_library_status,
            album,
            pressing_offers: metadata_draft.pressing.offers(),
            // A draft read from the files' own tags states no edition or copy.
            release_lines: (!matches!(metadata_provenance, Some(MetadataProvenance::FileMetadata)))
                .then(|| metadata_draft.pressing.release_lines(track_length_ms)),
            metadata_draft,
            album_artists,
            metadata_draft_is_blank: draft_blank,
            metadata_provenance,
            release_link,
            mapping,
            left_out_audio,
            cover,
            lookup_choices,
            // The attempt running now is what the pane shows; an earlier
            // failure is behind it.
            failure: if importing { None } else { failure },
            pane_failure,
            opens_on,
        };
        Ok((pane_placement, pane))
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
        Option<ImportCandidateDetail>,
        crate::ImportError,
    > {
        self.store
            .subscribe_reconfigurable(initial, |key| {
                Ok(coven::Both(PaneHeadParams::from(key), PaneDraftParams::from(key)))
            })
            .process(|_, rows| pane_rows(rows)?.map(CandidateDetailRows::process).transpose())
    }

    /// The mapping rows and album artists of the pane's draft read, once.
    pub(super) async fn load_pane_mapping(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<(super::mapping_rows::MappingRows, Option<crate::ArtistField>), crate::ImportError>
    {
        let request = PaneDraftParams::from(&Some(key.clone()));
        let row = self.store.read_row(request).await?;
        let draft = DraftRows::of(row.draft)?;
        Ok((draft.mapping, draft.album_artists))
    }

    /// The folder that failed validation at the key the read is set to, as
    /// the pane that shows its details reads it, live; `None` for no key, or
    /// a key naming no such folder, which clears a selection. Each statement
    /// in `invalid_detail_read` keeps only rows whose folder the scan stored
    /// as invalid, so the read returns no rows for a key of any other kind.
    pub(crate) fn subscribe_invalid_candidate(
        &self,
        initial: Option<crate::CandidateKey>,
    ) -> coven::ReconfigurableLiveQuery<
        Option<crate::CandidateKey>,
        Option<crate::InvalidCandidateDetail>,
        bae_store::DbError,
    > {
        self.store
            .subscribe_reconfigurable(initial, |key| {
                Ok(coven::Maybe(key.as_ref().map(folder_scans::invalid_detail_read)))
            })
            .process(|key, reads| match bae_store::requested(key, reads) {
                Some((key, reads)) => folder_scans::invalid_detail_of(key, reads),
                None => Ok(None),
            })
    }

    /// One candidate as the pane reads it, once.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn load_import_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<ImportCandidateDetail>, crate::ImportError> {
        let key = Some(key.clone());
        self.store
            .read(move |sql| {
                Ok((
                    bae_store::statement_rows(&sql, &PaneHeadParams::from(&key))?,
                    bae_store::statement_rows(&sql, &PaneDraftParams::from(&key))?,
                ))
            })
            .process(|rows| pane_rows(rows)?.map(CandidateDetailRows::process).transpose())
            .await
    }
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
