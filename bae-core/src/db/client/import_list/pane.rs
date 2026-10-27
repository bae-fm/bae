//! One candidate as the pane reads it, whole: its folder and files, its
//! draft and the releases it is linked to, what is running for it, and where
//! the list places it, which the pane reads from the list's placement as the
//! list's rows do.

use super::super::import_state::{
    load_album_pick_on, load_pane_rows_on, load_standing_on, load_states_on,
};
use super::super::records::{album_records_name_on, check_releases_in_library_on};
use super::super::release_groupings::{load_candidate_on, skipped_on};
use super::super::source_releases::load_source_release_on;
use super::listed::{ListedColumns, LISTED_COLUMNS};
use super::window::stored_embedded_cover;
use super::*;
use crate::identify::{TerminalVerdict, VerdictSummary};
use crate::import::cover_art::CoverChoice;
use crate::import::folder_scanner::{CategorizedFiles, FileVersion, FolderCandidate};
use crate::import::search::MetadataResult;
use crate::import::{CoverSelection, PressingLink, ReleaseLink};

#[derive(Clone, PartialEq)]
pub(super) struct CandidateDetailRows {
    candidate: FolderCandidate,
    source_error: Option<crate::import::GroupingBlock>,
    actionable: bool,
    skipped: bool,
    imported: Option<ImportedRelease>,
    current: Option<DbImportCandidateState>,
    pane_rows: DbCandidatePaneRows,
    artist_resolutions: Vec<crate::import::ResolvedCredit>,
    metadata_revision: u64,
    statuses: Option<Vec<LibraryStatus>>,
    records: Vec<crate::import::ReleaseRecord>,
    picked_library_status: Option<LibraryStatus>,
    album: crate::import::DraftAlbum,
    embedded_cover: Option<(FileVersion, Vec<u8>)>,
    standing: crate::import::TriageRuntimeFacts,
    listed: ListedColumns,
}

/// One candidate, whole, with its standing and where the list places it.
/// `None` when the key names no release the list lists — which is what
/// clears a selection.
pub(super) fn load_candidate_detail_on(
    sql: &SqlReadContext<'_>,
    key: &str,
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
    let stored = load_candidate_on(sql, key)?
        .ok_or_else(|| DbError::Message(format!("the list lists {key}, which no scan stored")))?;
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
            |row| {
                Ok(ImportedRelease {
                    release_id: row.get(0)?,
                    album_id: row.get(1)?,
                })
            },
        )
        .optional()?;

    let state = load_states_on(sql, Some(&content_hash))?.remove(&content_hash);
    let current = state.filter(|state| state.file_edits.revision == candidate.file_edit_revision);
    let identify = current.as_ref().and_then(|state| state.identify.as_ref());
    let release_link = current
        .as_ref()
        .and_then(|state| state.release_link.clone());
    let pane_rows = load_pane_rows_on(sql, &content_hash)?;
    // Read here, inside the pane's live query, so an artist another import
    // commits — or any artist row that changes — reads the credits again.
    let artist_resolutions = super::super::artist_identity_choice::resolve_import_credits_on(
        sql,
        &content_hash,
        pane_rows.draft.credits(),
    )?;
    let metadata_revision = sql.query_row(
        "SELECT s.metadata_revision \
             FROM scan_candidate c JOIN import_candidate_state s \
               ON s.content_hash = c.content_hash \
             WHERE c.watched_folder_path = ? AND c.path = ?",
        params![candidate.watched_folder_path, candidate.key()],
        |row| row.get::<_, i64>(0),
    )?;
    let metadata_revision = u64::try_from(metadata_revision).map_err(|error| {
        DbError::Message(format!("candidate metadata revision is negative: {error}"))
    })?;

    let statuses = identify
        .map(|identify| library_statuses(sql, &identify.verdict))
        .transpose()?;
    // The linked pressing's stored releases: what names its records and
    // checks it against the library.
    let claimed = claimed_releases_on(
        sql,
        &candidate,
        release_link.as_ref().and_then(ReleaseLink::pressing),
    )?;
    // Every catalog record of what the candidate is linked to: the linked
    // pressing's releases describe it, an album names its own.
    let records = match &release_link {
        Some(ReleaseLink::Album(album)) => album.records(),
        Some(ReleaseLink::Pressing(_)) | None => {
            crate::import::source_release::claimed_records(&claimed.iter().collect::<Vec<_>>())
        }
    };
    let picked_library_status = match claimed.first() {
        Some(release) => check_releases_in_library_on(sql, &[release.library_check()])?
            .into_iter()
            .next(),
        None => None,
    };
    let album = candidate_album_on(sql, &content_hash, &records)?;
    let embedded_cover = match pane_rows.cover.as_ref() {
        Some(CoverSelection::Embedded(source_file_id)) => Some(stored_embedded_cover(
            sql,
            &candidate.watched_folder_path,
            &candidate.key(),
            source_file_id,
        )?),
        _ => None,
    };
    Ok(Some(CandidateDetailRows {
        candidate,
        source_error,
        actionable,
        skipped,
        imported,
        current,
        pane_rows,
        artist_resolutions,
        metadata_revision,
        statuses,
        records,
        picked_library_status,
        album,
        embedded_cover,
        standing,
        listed,
    }))
}

/// The album an import of the candidate `content_hash`, linked to what
/// `records` describe, files under: the one picked, else the one its records
/// name, else a new one.
fn candidate_album_on(
    sql: &SqlReadContext<'_>,
    content_hash: &str,
    records: &[crate::import::ReleaseRecord],
) -> Result<crate::import::DraftAlbum, DbError> {
    use crate::import::DraftAlbum;
    let title = |album_id: &str| -> Result<String, DbError> {
        sql.query_row(
            "SELECT title FROM albums WHERE id = ?",
            params![album_id],
            |row| row.get(0),
        )
        .map_err(DbError::from)
    };
    if let Some(album_id) = load_album_pick_on(sql, content_hash)? {
        return Ok(DraftAlbum::Picked {
            title: title(&album_id)?,
            album_id,
        });
    }
    Ok(match album_records_name_on(sql, records, &[])? {
        Some(album_id) => DraftAlbum::Named {
            title: title(&album_id)?,
            album_id,
        },
        None => DraftAlbum::New,
    })
}

impl CandidateDetailRows {
    pub(super) fn process(self) -> Result<ImportCandidateDetailProjection, DbError> {
        let Self {
            candidate,
            source_error,
            actionable,
            skipped,
            imported,
            current,
            pane_rows,
            artist_resolutions,
            metadata_revision,
            statuses,
            records,
            picked_library_status,
            album,
            embedded_cover,
            standing,
            listed,
        } = self;
        let listed = listed.listed(&candidate.key())?;
        let metadata_provenance = current
            .as_ref()
            .and_then(|state| state.metadata_provenance.clone());
        let release_link = current
            .as_ref()
            .and_then(|state| state.release_link.clone());
        let metadata_author = current
            .as_ref()
            .map_or(crate::import::MetadataAuthor::Nobody, |state| {
                state.metadata_author
            });
        let signals = current.as_ref().and_then(|state| state.signals.clone());
        let lookup_choices = current
            .as_ref()
            .map(|state| state.lookup_choices.clone())
            .unwrap_or_default();

        let audio = crate::signals::AudioFacts::of_files(&candidate.files)
            .map_err(|error| DbError::Message(error.to_string()))?;
        let durations = &audio.durations;
        let mut verdict = None;
        let mut resumed_identify_state = crate::identify::IdentifyState::Idle;
        let identify = current.as_ref().and_then(|state| state.identify.as_ref());
        if let Some(identify) = identify {
            let statuses = statuses
                .as_ref()
                .expect("identification has fetched library statuses");
            // The check answers for every release the verdict names, so a lookup
            // cannot miss: `check_releases_in_library_on` returns one status per
            // check and the checks are exactly those releases.
            let status_of = |result: &MetadataResult| {
                statuses
                    .iter()
                    .find(|status| status.release_id == result.release_id)
                    .expect("the library check covers every release the verdict names")
                    .clone()
            };
            verdict = Some(VerdictSummary::of(
                &identify.verdict,
                identify.kept_own_draft,
            ));
            // The candidate's own text is what the rows are judged and ordered
            // against, live or resumed: striking a number out re-orders the
            // rows the next time they are read, with nothing asked again.
            let text = crate::identify::CandidateText::of_stored(signals.as_ref(), &lookup_choices);
            resumed_identify_state =
                identify
                    .verdict
                    .clone()
                    .resume_state(&status_of, text, audio.clone());
        }
        let pane = crate::import::pane::draft_pane(&candidate.files, durations, &pane_rows.draft)
            .map_err(|error| {
            DbError::Message(format!(
                "candidate {} cannot be drawn: {error}",
                candidate.key()
            ))
        })?;
        let cover = chosen_cover(&candidate.files, pane_rows.cover.as_ref(), embedded_cover);
        Ok(ImportCandidateDetailProjection {
            is_added: imported.is_some(),
            candidate,
            source_error,
            actionable,
            skipped,
            resumed_identify_state,
            verdict,
            metadata_provenance,
            release_link,
            metadata_author,
            metadata_revision,
            imported,
            records,
            picked_library_status,
            album,
            metadata_draft: pane.edit,
            artist_resolutions,
            mapping: pane.mapping,
            cover,
            remote_covers: pane_rows.cover_options,
            signals,
            lookup_choices,
            failure: pane_rows.failure,
            session: pane_rows.session,
            standing,
            listed,
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
) -> Result<Vec<crate::import::source_release::SourceRelease>, DbError> {
    let Some(link) = link else {
        return Ok(Vec::new());
    };
    link.claimed()
        .map(|release| {
            load_source_release_on(sql, release)?.ok_or_else(|| {
                DbError::Message(format!(
                    "{} is claimed for {} but nothing fetched it",
                    release.key,
                    candidate.key()
                ))
            })
        })
        .collect()
}

/// The cover the candidate commits with: its stored selection, read as it
/// stands. A selection naming an image the folder no longer holds describes
/// the candidate no longer, and no other image stands in for it.
fn chosen_cover(
    files: &CategorizedFiles,
    chosen: Option<&CoverSelection>,
    embedded_cover: Option<(FileVersion, Vec<u8>)>,
) -> Option<CoverChoice> {
    match chosen {
        None => None,
        Some(CoverSelection::Local(file_id)) => {
            let image = files
                .artwork()
                .find(|image| &image.relative_path == file_id);
            if image.is_none() {
                tracing::warn!(
                    file_id,
                    "the selected cover is no longer among the candidate's images"
                );
            }
            image.map(|image| CoverChoice::local(file_id.clone(), image.version()))
        }
        Some(CoverSelection::Embedded(source_file_id)) => embedded_cover
            .map(|(source, data)| CoverChoice::embedded(source_file_id.clone(), source, data)),
        Some(CoverSelection::Remote(image, source)) => {
            Some(CoverChoice::remote(image.clone(), *source))
        }
    }
}

/// The live library status of every release the verdict names. A release the
/// check does not answer for is a read that must fail rather than a release
/// silently resumed as "not in the library".
fn library_statuses(
    sql: &(impl super::super::query::QueryOne + super::super::query::QueryRows),
    verdict: &TerminalVerdict,
) -> Result<Vec<LibraryStatus>, DbError> {
    let mut seen = HashSet::new();
    let checks: Vec<LibraryCheck> = verdict
        .named_releases()
        .into_iter()
        .filter(|result| seen.insert(result.release_id.clone()))
        .map(LibraryCheck::from)
        .collect();
    check_releases_in_library_on(sql, &checks)
}
