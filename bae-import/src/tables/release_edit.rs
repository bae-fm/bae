//! A library release as its editor reads it: the seed an editor opens on,
//! the draft it shows for the form it holds, and what a save of the form
//! reads, before it and again in its own write ([`ReleaseSaveInputs`]).
//!
//! The editor holds its form ([`ReleaseEditForm`]); no table holds it. Each
//! read here takes the form in its request, and the draft's live query
//! reruns after each write that changes what it read: an artist merged in
//! the library, the album renamed, a cover picked, the release deleted.

use super::import_list::artist_field_of_json;
use super::*;
use crate::{ReleaseEditDraft, ReleaseEditForm, ReleaseEditLoadError, ReleaseEditSeed};
use bae_records::records::{editor_release_on, release_edit_rows::EditorRelease};
pub(crate) use draft::{release_edit_draft_read, release_edit_draft_rows_of, ReleaseEditDraftRows};
pub use layout::{release_edit_layout_on, store_release_edit_sheet_decisions_on};
pub use save_inputs::{release_save_inputs_unchanged_on, ReleaseSaveInputs};
use seed::release_edit_seed;
pub use seed::ReleaseEditContext;

mod artist_fields;
mod draft;
mod form_assignments;
mod layout;
mod save_inputs;
mod seed;

/// Why bae can't read the seed of a release, so an editor can't open on it.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseEditSeedRefusal {
    /// The library holds no release of this id.
    #[error("release {0} is not in the library")]
    ReleaseMissing(String),
    /// The library holds the release in a shape the editor can't read.
    #[error(transparent)]
    Unreadable(#[from] ReleaseEditLoadError),
}

/// The seed of the release `release_id` from its rows, or why there is none.
fn seed_of(
    release_id: &str,
    rows: Option<EditorRelease>,
) -> Result<Result<ReleaseEditSeed, ReleaseEditSeedRefusal>, DbError> {
    Ok(match rows {
        None => Err(ReleaseEditSeedRefusal::ReleaseMissing(release_id.to_string())),
        Some(rows) => {
            release_edit_seed(rows.decoded()?.into()).map_err(ReleaseEditSeedRefusal::from)
        }
    })
}

/// The seed of the release `release_id` as the write `sql` finds it.
fn seed_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<Result<ReleaseEditSeed, ReleaseEditSeedRefusal>, DbError> {
    seed_of(release_id, editor_release_on(sql, release_id)?)
}

/// The form an editor opens on the release `release_id` as the write `sql`
/// holds it: what a save hands its editor once it has written the release.
pub fn release_edit_form_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<ReleaseEditForm, DbError> {
    match seed_on(sql, release_id)? {
        Ok(seed) => Ok(ReleaseEditForm::opened_on(release_id.to_string(), seed)),
        Err(refusal) => {
            Err(DbError::unreadable(format!("release {release_id} as an editor reads it"), refusal))
        }
    }
}

impl ImportTables {
    /// The form an editor opens with on the release `release_id` as the
    /// library holds it now: nothing edited, applied or picked.
    pub async fn read_release_edit_form(
        &self,
        release_id: &str,
    ) -> Result<Result<ReleaseEditForm, ReleaseEditSeedRefusal>, DbError> {
        let release_id = release_id.to_string();
        let read_id = release_id.clone();
        self.store
            .read(move |sql| editor_release_on(&sql, &read_id))
            .process(move |rows| {
                Ok(seed_of(&release_id, rows)?
                    .map(|seed| ReleaseEditForm::opened_on(release_id.clone(), seed)))
            })
            .await
    }

    /// The values clearing `form` puts in its place: no album title, artist
    /// or year, a blank pressing, and each track untitled, credited to the
    /// album's artists and with no listed length, role or work, on the
    /// mediums and at the places the library holds for the release.
    pub async fn cleared_release_edit_values(
        &self,
        form: &ReleaseEditForm,
    ) -> Result<crate::LibraryDraft, DbError> {
        let release_id = form.release_id.clone();
        let layout = self
            .store
            .read(move |sql| {
                bae_records::records::layout::laid_out_on(
                    &sql,
                    &bae_records::records::layout::LibraryLayout {
                        release_id: release_id.clone(),
                        applied: false,
                        mediums: "[]".to_owned(),
                        places: "[]".to_owned(),
                        moves: "[]".to_owned(),
                        edits: bae_records::records::sheet_files::NO_SHEET_EDITS.to_owned(),
                    },
                )
            })
            .await?
            .layout;
        Ok(crate::LibraryDraft {
            album_title: String::new(),
            album_artist_assignments: Vec::new(),
            album_year: String::new(),
            pressing: crate::RawPressingEdit::from_pressing(&bae_types::pressing::Pressing::blank()),
            mediums: layout.mediums,
            tracks: layout
                .tracks
                .into_iter()
                .map(|track| crate::DraftTrack {
                    edit: crate::RawTrackEdit {
                        id: track.track_id.clone(),
                        title: String::new(),
                        artist_assignments: crate::TrackArtistAssignments::AlbumArtists,
                        file: crate::LibraryTrack(track.track_id),
                    },
                    place: track.place,
                    stated_length_ms: None,
                    roles: Vec::new(),
                    works: Vec::new(),
                })
                .collect(),
            release_roles: Vec::new(),
            works: Default::default(),
        })
    }

    /// The draft of the form the request names, live: it reruns after each
    /// write that changes what it read. It reads nothing until a form is
    /// set, and `None` once the form's release is gone.
    pub fn subscribe_release_edit_draft(
        &self,
    ) -> coven::ReconfigurableLiveQuery<
        Option<ReleaseEditForm>,
        Option<Option<ReleaseEditDraft>>,
        DbError,
    > {
        self.store
            .subscribe_reconfigurable(None, |form| {
                Ok(coven::Maybe(form.as_ref().map(release_edit_draft_read).transpose()?))
            })
            .process(|form, reads| {
                let (Some(form), Some(reads)) = (form, reads) else {
                    return Ok(None);
                };
                let rows = release_edit_draft_rows_of(form, reads)?;
                Ok(Some(rows.map(ReleaseEditDraftRows::draft).transpose()?))
            })
    }

    /// The release `release_id` as the editor reads it, from one read.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn find_release_edit_context(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseEditContext>, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| editor_release_on(&sql, &release_id))
            .process(|rows| rows.map(|rows| Ok(rows.decoded()?.into())).transpose())
            .await
    }
}
