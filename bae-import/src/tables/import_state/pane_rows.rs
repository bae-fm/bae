//! The draft's rows — the album fields, the tracks with their decisions, the
//! artist assignments — as one candidate's save writes them and the pane's
//! read draws them with the candidate's cover (`cover_rows`).
//!
//! Every row group hangs off the candidate by content hash, and the draft's
//! own provenance and tracks hang off `import_candidate_edit`. A save writes
//! the draft over the one stored (`draft_store`), changing only the rows that
//! differ; which groups a candidate save writes, and under what checks, is
//! `preparation_rows`'s to say.
//! Its tables are `pane_rows.sql`, and `values.sql` and `provenance.sql` for
//! the draft's values and provenance, which every device's library holds.
//!
//! The statements that write and read labels, barcodes, artist assignments,
//! mediums, roles and works are in `draft_values`.

mod provenance;
mod values;

use super::draft_values::{
    insert_album_artist_assignments, insert_barcode_rows, insert_draft_values, insert_label_rows,
    insert_medium_rows, insert_track_artist_assignments, insert_track_values, mediums_of, roles_of,
    works_of, FormColumns, StoredRole,
};
use crate::tables::draft_rows::{
    assignment_kind_column, assignments_of, decoded, StoredAssignment,
};
use bae_store::WriteStatements as _;

pub(crate) use provenance::{insert_provenance, store_provenance, StoredProvenance};

use super::*;
use crate::{
    AudioFile, CandidateDraft, CandidateTrack, MetadataAuthor, RawLabelEdit, RawPressingEdit,
    RawTrackEdit, TrackArtistAssignments,
};

/// The column a draft's author is stored as.
fn author_column(author: MetadataAuthor) -> &'static str {
    match author {
        MetadataAuthor::Nobody => "nobody",
        MetadataAuthor::Prefill => "prefill",
        MetadataAuthor::Identification => "identification",
        MetadataAuthor::Person => "person",
    }
}

pub(crate) fn author_of(stored: &str) -> Result<MetadataAuthor, DbError> {
    match stored {
        "nobody" => Ok(MetadataAuthor::Nobody),
        "prefill" => Ok(MetadataAuthor::Prefill),
        "identification" => Ok(MetadataAuthor::Identification),
        "person" => Ok(MetadataAuthor::Person),
        other => Err(DbError::unreadable("a draft's stored author", other)),
    }
}

sqlive_compose::write!(
    DeleteDraft(content_hash: &'p str),
    r#"
    DELETE FROM import_candidate_edit WHERE content_hash = :content_hash
"#
);

sqlive_compose::write!(
    InsertDraft(
        content_hash: &'p str,
        album_title: &'p str,
        album_year: &'p str,
        year: &'p str,
        country: Option<&'static str>,
        region: Option<&'static str>,
        media: &'p str,
        status: Option<&'static str>,
        packaging: Option<&'static str>,
        discogs_details: &'p str,
        author: &'static str,
        draft_blank: bool,
        draft_valid: bool,
    ),
    r#"
    INSERT INTO import_candidate_edit
        (content_hash, album_title, album_year, year, country, region, media, status, packaging,
        discogs_details, author, draft_blank, draft_valid)
    VALUES (:content_hash, :album_title, :album_year, :year, :country, :region, :media, :status,
        :packaging, :discogs_details, :author, :draft_blank, :draft_valid)
"#
);

sqlive_compose::write!(
    UpsertDraftTrack(
        content_hash: &'p str,
        track_id: &'p str,
        position: i64,
        title: &'p str,
        artist_assignment_kind: &'static str,
        medium: std::num::NonZeroU16,
        side: std::num::NonZeroU8,
        track_number: std::num::NonZeroU32,
        stated_length_ms: Option<i64>,
        file_kind: &'p str,
        file_id: &'p str,
        sheet_id: Option<&'p str>,
        slice_index: Option<i64>,
    ),
    r#"
    INSERT INTO import_candidate_track
        (content_hash, track_id, position, title, artist_assignment_kind, medium, side,
        track_number, stated_length_ms, file_kind, file_id, sheet_id, slice_index)
    VALUES (:content_hash, :track_id, :position, :title, :artist_assignment_kind, :medium,
        :side, :track_number, :stated_length_ms, :file_kind, :file_id, :sheet_id, :slice_index)
    ON CONFLICT (content_hash, track_id) DO UPDATE SET
        position = excluded.position, title = excluded.title, artist_assignment_kind =
        excluded.artist_assignment_kind, medium = excluded.medium, side = excluded.side,
        track_number = excluded.track_number, stated_length_ms = excluded.stated_length_ms,
        file_kind = excluded.file_kind, file_id = excluded.file_id, sheet_id =
        excluded.sheet_id, slice_index = excluded.slice_index
"#
);

// Damage a test plants: a draft read from a source that says nobody wrote
// it.
#[cfg(test)]
sqlive_compose::write!(
    DisownDraft(content_hash: &'p str),
    r#"
    UPDATE import_candidate_edit SET author = 'nobody' WHERE content_hash = :content_hash
"#
);

/// Delete the draft of `content_hash`: its source and every row below it
/// hang off the draft row and go with it.
pub(super) fn delete_draft(sql: &CovenWrite<'_>, content_hash: &str) -> Result<(), DbError> {
    sql.run(&DeleteDraftParams { content_hash })?;
    Ok(())
}

/// The draft row of a candidate that has none yet, with everything hanging
/// off it: its album artists, its mediums, its tracks and their values.
pub(super) fn insert_draft(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &CandidateDraft,
    author: MetadataAuthor,
) -> Result<(), DbError> {
    let row = EditRow::of(draft, author);
    sql.run(&InsertDraftParams {
        content_hash,
        album_title: &row.form.album_title,
        album_year: &row.form.album_year,
        year: &row.form.year,
        country: row.form.facts.country,
        region: row.form.facts.region,
        media: &row.form.media,
        status: row.form.facts.status,
        packaging: row.form.facts.packaging,
        discogs_details: &row.form.facts.discogs_details,
        author: row.author,
        draft_blank: row.draft_blank,
        draft_valid: row.draft_valid,
    })?;
    insert_label_rows(sql, content_hash, &draft.pressing.labels)?;
    insert_barcode_rows(sql, content_hash, &draft.pressing.barcodes)?;
    insert_album_artist_assignments(sql, content_hash, &draft.album_artist_assignments)?;
    insert_draft_body(sql, content_hash, draft)
}

/// What a candidate's draft row holds beside its key: the form's album-level
/// fields, who wrote the draft, and whether it is blank and would import.
#[derive(PartialEq)]
pub(super) struct EditRow {
    pub(super) form: FormColumns,
    pub(super) author: &'static str,
    pub(super) draft_blank: bool,
    pub(super) draft_valid: bool,
}

impl EditRow {
    pub(super) fn of(draft: &CandidateDraft, author: MetadataAuthor) -> Self {
        let release_edit = draft.release_edit();
        Self {
            form: FormColumns::of(&release_edit),
            author: author_column(author),
            draft_blank: release_edit.is_blank(),
            draft_valid: release_edit.shape().is_ok(),
        }
    }
}

/// The draft's mediums, tracks and the values hanging off them: the rows
/// below the draft row besides its album artists.
pub(super) fn insert_draft_body(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &CandidateDraft,
) -> Result<(), DbError> {
    insert_medium_rows(sql, content_hash, &draft.mediums)?;
    // The works first: a track names the works it performs.
    insert_draft_values(sql, content_hash, &draft.works, &draft.release_roles)?;
    for (position, track) in draft.tracks.iter().enumerate() {
        insert_track(sql, content_hash, position, track)?;
    }
    Ok(())
}

/// The track at `position` with the rows hanging off it, where none is
/// stored.
fn insert_track(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    position: usize,
    track: &CandidateTrack,
) -> Result<(), DbError> {
    store_track_row(sql, content_hash, position, track)?;
    insert_track_rows(sql, content_hash, track)
}

/// The track's own row at `position`, in place of the one stored with its id.
pub(super) fn store_track_row(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    position: usize,
    track: &CandidateTrack,
) -> Result<(), DbError> {
    let (file_kind, file_id, sheet_id, slice_index) = mapping_file_columns(&track.edit.file);
    let stated_length_ms = track
        .stated_length_ms
        .map(|length| to_i64(length, "a track's stated length"))
        .transpose()?;
    sql.run(&UpsertDraftTrackParams {
        content_hash,
        track_id: &track.edit.id,
        position: position as i64,
        title: &track.edit.title,
        artist_assignment_kind: assignment_kind_column(&track.edit.artist_assignments),
        medium: track.place.medium,
        side: track.place.side,
        track_number: track.place.number,
        stated_length_ms,
        file_kind,
        file_id,
        sheet_id,
        slice_index,
    })?;
    Ok(())
}

/// The rows hanging off a track: its own artists, its roles and its works.
pub(super) fn insert_track_rows(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    track: &CandidateTrack,
) -> Result<(), DbError> {
    if let TrackArtistAssignments::Explicit(assignments) = &track.edit.artist_assignments {
        insert_track_artist_assignments(sql, content_hash, &track.edit.id, assignments)?;
    }
    insert_track_values(sql, content_hash, &track.edit.id, &track.roles, &track.works)
}

fn mapping_file_columns(file: &AudioFile) -> (&str, &str, Option<&str>, Option<i64>) {
    match file {
        AudioFile::Standalone { file_id } => ("standalone", file_id.as_str(), None, None),
        AudioFile::SheetSlice { file_id, sheet_id, index } => {
            ("sheet_slice", file_id.as_str(), Some(sheet_id.as_str()), Some(i64::from(*index)))
        }
    }
}

/// The preparation of the candidate `content_hash`, which the caller knows
/// holds a state row and no import of it.
#[cfg(any(test, feature = "test-utils", debug_assertions))]
fn stored_for_test_on(
    sql: &impl bae_store::WriterRead,
    content_hash: &str,
) -> Result<crate::preparations::CandidatePreparation, DbError> {
    match super::preparation_rows::load_preparation_on(sql, content_hash)? {
        Some(super::preparation_rows::CandidatePreparationRead::Stored(stored)) => {
            Ok(stored.preparation)
        }
        _ => Err(DbError::unreadable("the preparation of a stored candidate", content_hash)),
    }
}

#[cfg(any(test, feature = "test-utils", debug_assertions))]
/// Everything one candidate's pane settled: its cover, its album fields, its
/// row edits, the failure its last import left, and the failure of the
/// pane's last command.
pub(crate) fn load_pane_rows_on(
    sql: &SqlReadContext<'_>,
    candidate: OneCandidate<'_>,
) -> Result<DbCandidatePaneRows, DbError> {
    let OneCandidate::Hash(content_hash) = candidate;
    let stored = stored_for_test_on(sql, content_hash)?;
    let (draft_blank, draft_valid) = load_draft_columns_on(sql, candidate)?;
    Ok(DbCandidatePaneRows {
        cover: stored.metadata.cover.map(|cover| cover.selection()),
        draft: stored.metadata.draft,
        draft_blank,
        draft_valid,
        failure: load_failure_on(sql, candidate)?,
        pane_failure: load_pane_failure_on(sql, candidate)?,
    })
}

impl ImportTables {
    /// Everything a person settled about one candidate through its pane, for
    /// a caller that knows it holds a state row and no import of it.
    #[cfg(any(test, feature = "test-utils", debug_assertions))]
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<DbCandidatePaneRows, DbError> {
        let content_hash = content_hash.to_string();
        self.store.read(move |sql| load_pane_rows_on(&sql, OneCandidate::Hash(&content_hash))).await
    }
}

#[cfg(any(test, feature = "test-utils", debug_assertions))]
/// Whether the draft of `candidate` is blank and whether it would import, as
/// its row stores them beside its fields, in the write that stored them.
fn load_draft_columns_on(
    sql: &impl bae_store::WriterRead,
    candidate: OneCandidate<'_>,
) -> Result<(bool, bool), DbError> {
    let filter = candidate.filter("content_hash");
    sql.query_row(
        &format!("SELECT draft_blank, draft_valid FROM import_candidate_edit {}", filter.clause),
        filter.params().as_slice(),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| DbError::unreadable("import_candidate_edit of a stored draft", candidate))
}

/// Whether the candidate `content_hash` holds a draft row.
pub(super) fn has_draft_row_on(
    sql: &impl bae_store::WriterRead,
    content_hash: &str,
) -> Result<bool, DbError> {
    Ok(sql
        .query_row(
            "SELECT 1 FROM import_candidate_edit WHERE content_hash = ?",
            [content_hash],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// The draft a pane's read selects, as
/// [`super::fragments::DraftRows`] names it.
pub(crate) fn draft_of(draft: &super::fragments::DraftRows) -> Result<CandidateDraft, DbError> {
    StoredDraftRow::of(draft)?.draft()
}

/// One `import_candidate_edit` row as SQLite hands it over, its lists still
/// the JSON arrays the draft read returns.
struct StoredDraftRow {
    album_title: String,
    album_year: String,
    year: String,
    labels: Vec<RawLabelEdit>,
    barcodes: Vec<String>,
    facts: bae_types::pressing::PressingFacts,
    album_assignments: String,
    mediums: String,
    tracks: String,
    release_roles: String,
    works: String,
    parts: String,
}

impl StoredDraftRow {
    /// The row a pane's read selects, whose lists are the JSON arrays
    /// [`super::fragments::DraftRows`] returns.
    fn of(draft: &super::fragments::DraftRows) -> Result<Self, DbError> {
        let mut facts = bae_records::records::pressing_columns::facts_without_media_of(
            draft.country.clone(),
            draft.region.clone(),
            draft.status.as_deref(),
            draft.packaging.as_deref(),
            decoded(&draft.discogs_details, "discogs details")?,
        )
        .map_err(|detail| DbError::unreadable("the facts of a stored draft", detail))?;
        facts.media = decoded(&draft.media, "media")?;
        Ok(Self {
            album_title: draft.album_title.clone(),
            album_year: draft.album_year.clone(),
            year: draft.year.clone(),
            labels: decoded(&draft.labels, "labels")?,
            barcodes: decoded(&draft.barcodes, "barcodes")?,
            facts,
            album_assignments: draft.album_assignments.clone(),
            mediums: draft.mediums.clone(),
            tracks: draft.tracks.clone(),
            release_roles: draft.release_roles.clone(),
            works: draft.works.clone(),
            parts: draft.draft_parts.clone(),
        })
    }

    fn draft(&self) -> Result<CandidateDraft, DbError> {
        let album_artist_assignments = assignments_of(&self.album_assignments, "album artists")?;
        let mediums = mediums_of(&self.mediums)?;
        let tracks = decoded::<Vec<StoredTrack>>(&self.tracks, "tracks")?
            .into_iter()
            .map(StoredTrack::track)
            .collect::<Result<_, _>>()?;
        Ok(CandidateDraft {
            album_title: self.album_title.clone(),
            album_artist_assignments,
            album_year: self.album_year.clone(),
            pressing: RawPressingEdit {
                year: self.year.clone(),
                labels: self.labels.clone(),
                barcodes: self.barcodes.clone(),
                facts: self.facts.clone(),
            },
            mediums,
            tracks,
            release_roles: roles_of(decoded(&self.release_roles, "release roles")?)?,
            works: works_of(decoded(&self.works, "works")?, decoded(&self.parts, "work parts")?)?,
        })
    }
}

/// One `import_candidate_track` row as the draft's read holds it, with its
/// artist assignments, roles and works.
#[derive(serde::Deserialize)]
pub(crate) struct StoredTrack {
    track_id: String,
    title: String,
    assignment_kind: String,
    medium: std::num::NonZeroU16,
    side: std::num::NonZeroU8,
    track_number: std::num::NonZeroU32,
    stated_length_ms: Option<i64>,
    file_kind: String,
    file_id: String,
    sheet_id: Option<String>,
    slice_index: Option<i64>,
    assignments: Vec<StoredAssignment>,
    roles: Vec<StoredRole>,
    works: Vec<String>,
}

impl StoredTrack {
    pub(crate) fn track(self) -> Result<CandidateTrack, DbError> {
        let missing = |what: &str| {
            DbError::unreadable(
                format!(
                    "the {what} of the track stored for {}, of a {} file",
                    self.track_id, self.file_kind
                ),
                None::<String>,
            )
        };
        let artist_assignments = match self.assignment_kind.as_str() {
            "album_artists" => TrackArtistAssignments::AlbumArtists,
            "explicit" => TrackArtistAssignments::Explicit(
                self.assignments
                    .into_iter()
                    .map(StoredAssignment::credited)
                    .collect::<Result<_, _>>()?,
            ),
            other => return Err(DbError::unreadable("a stored artist assignment's kind", other)),
        };
        let file = match self.file_kind.as_str() {
            "standalone" => AudioFile::Standalone { file_id: self.file_id },
            "sheet_slice" => AudioFile::SheetSlice {
                file_id: self.file_id,
                sheet_id: self.sheet_id.ok_or_else(|| missing("sheet"))?,
                index: to_u32(
                    self.slice_index.ok_or_else(|| missing("slice"))?,
                    "import_candidate_track.slice_index",
                )?,
            },
            other => return Err(DbError::unreadable("import_candidate_track.file_kind", other)),
        };
        let stated_length_ms = self
            .stated_length_ms
            .map(|length| to_u64(length, "import_candidate_track.stated_length_ms"))
            .transpose()?;
        Ok(CandidateTrack {
            edit: RawTrackEdit { id: self.track_id, title: self.title, artist_assignments, file },
            place: bae_types::mediums::TrackPlace {
                medium: self.medium,
                side: self.side,
                number: self.track_number,
            },
            stated_length_ms,
            roles: roles_of(self.roles)?,
            works: self.works,
        })
    }
}

/// Seed a candidate's draft from the folder's file tags, authored by the
/// pre-fill, with provenance naming the tags as its source.
pub(super) fn insert_file_tags_draft(
    sql: &CovenWrite<'_>,
    content_hash: &str,
    draft: &crate::CandidateDraft,
) -> Result<(), DbError> {
    pane_rows::insert_draft(sql, content_hash, draft, crate::MetadataAuthor::Prefill)?;
    insert_provenance(sql, content_hash, &crate::MetadataProvenance::FileMetadata)?;
    Ok(())
}

/// Everything a person settled about one candidate through its pane, keyed by
/// the same content hash the rest of its state is.
///
/// Read as a group by the per-candidate query, which draws the header, the
/// cover and the mapping table from them at once. Kept off
/// [`DbImportCandidateState`] because that one is read for the whole queue and
/// none of these are.
#[cfg(any(test, feature = "test-utils", debug_assertions))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCandidatePaneRows {
    /// The cover the user chose. `None` leaves the picked release's default
    /// cover standing.
    pub cover: Option<crate::CoverSelection>,
    /// The candidate's one stored draft: album fields and one row per audio
    /// slot, each carrying its physical decision.
    pub draft: crate::CandidateDraft,
    /// Whether the draft is blank, as its write stored it.
    pub draft_blank: bool,
    /// Whether the draft would import, as its write stored it.
    pub draft_valid: bool,
    /// The failure the pane states for its last command, when it failed.
    pub pane_failure: Option<crate::PaneFailure>,
    /// The last import of this candidate that failed.
    pub failure: Option<crate::ImportFailure>,
}

#[cfg(test)]
impl ImportTables {
    /// Plant damage for a test: every draft in the library under `directory`
    /// states itself valid, whatever its fields say. Every draft write stores
    /// the validity of the draft it writes, so no write leaves one that
    /// disagrees.
    pub(crate) async fn plant_draft_valid_for_test(directory: &std::path::Path) {
        let directory = directory.to_path_buf();
        bae_util::test_spawn::on_blocking_pool(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "import_candidate_edit",
                // The list's trigger that follows the column would run the
                // list's functions, which this connection lacks.
                "DROP TRIGGER IF EXISTS import_list_on_draft_changed; \
                 UPDATE import_candidate_edit SET draft_valid = 1;",
            )
        })
        .await;
    }

    /// Replace the stored draft of `content_hash` with `draft`, as a person's
    /// save does, though the tracks of `draft` may play files the folder does
    /// not hold.
    pub(crate) async fn replace_draft_for_test(&self, content_hash: &str, draft: &CandidateDraft) {
        let (content_hash, draft) = (content_hash.to_string(), draft.clone());
        self.store
            .write(move |sql| {
                let stored = stored_for_test_on(&sql, &content_hash)?.metadata.draft;
                super::draft_store::store_draft(
                    &sql,
                    &content_hash,
                    (&stored, MetadataAuthor::Nobody),
                    (&draft, MetadataAuthor::Nobody),
                )
            })
            .await
            .expect("store_draft stores the draft");
    }

    /// Plant damage for a test: move every draft row in the library under
    /// `directory` off its candidate's state, so the state stays with no
    /// draft although this device never imported its release. Every scan and
    /// the deletion of an imported release store the draft, so no write
    /// leaves a state without one.
    pub(crate) async fn plant_missing_drafts_for_test(directory: &std::path::Path) {
        let directory = directory.to_path_buf();
        bae_util::test_spawn::on_blocking_pool(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "import_candidate_edit",
                // The update changes each draft row's content hash instead
                // of deleting the row, because a delete reaches the track
                // indexes, whose collation this connection lacks. The
                // statement drops the list's trigger on the draft because
                // that trigger calls the list's functions, which this
                // connection also lacks.
                "PRAGMA foreign_keys = OFF; \
                 DROP TRIGGER IF EXISTS import_list_on_draft_changed; \
                 UPDATE import_candidate_edit SET content_hash = 'planted-' || content_hash;",
            )
        })
        .await;
    }

    /// Plant damage for a test: every draft track in the library under
    /// `directory` plays the file `file_id`. A draft is written over its
    /// folder's audio, so no write stores a track over a file the folder
    /// does not hold.
    pub(crate) async fn plant_draft_tracks_on_file_for_test(
        directory: &std::path::Path,
        file_id: &str,
    ) {
        let directory = directory.to_path_buf();
        let damage = format!("UPDATE import_candidate_track SET file_id = '{file_id}';");
        bae_util::test_spawn::on_blocking_pool(move || {
            bae_store::test_fixtures::planted_damage::plant_in_library_file(
                &directory,
                "import_candidate_track",
                &damage,
            )
        })
        .await;
    }
}

#[cfg(test)]
#[path = "pane_rows_tests.rs"]
mod tests;
