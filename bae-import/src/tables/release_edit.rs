//! A library release as its editor reads it: the seed an editor opens on,
//! the draft it shows for the form it holds, and the check a save makes in
//! its own write that the library still holds the release as the editor
//! opened on it.
//!
//! The editor holds its form ([`ReleaseEditForm`]); no table holds it. Each
//! read here takes the form in its request, and the draft's live query
//! reruns after each write that changes what it read: an artist merged in
//! the library, the album renamed, a cover picked, the release deleted.

use super::import_list::{
    album_artist_standing_on, artist_field_standing_of_json, artist_standing_clauses,
    track_artist_standing_sql, CreditSource,
};
use super::*;
use crate::{ReleaseEditDraft, ReleaseEditForm, ReleaseEditLoadError, ReleaseEditSeed};
pub(crate) use draft::{release_edit_draft_rows_on, ReleaseEditDraftRows};
pub(crate) use form_assignments::assignment_rows_sql;
pub use seed::ReleaseEditContext;
use seed::{release_edit_rows_on, release_edit_seed, ReleaseEditRows};

mod draft;
mod form_assignments;
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
    rows: Option<ReleaseEditRows>,
) -> Result<ReleaseEditSeed, ReleaseEditSeedRefusal> {
    match rows {
        None => Err(ReleaseEditSeedRefusal::ReleaseMissing(release_id.to_string())),
        Some(rows) => release_edit_seed(rows.processed()).map_err(ReleaseEditSeedRefusal::from),
    }
}

/// The seed of the release `release_id` as the write `sql` finds it.
fn seed_on(
    sql: &CovenWrite<'_>,
    release_id: &str,
) -> Result<Result<ReleaseEditSeed, ReleaseEditSeedRefusal>, DbError> {
    Ok(seed_of(release_id, release_edit_rows_on(sql, release_id)?))
}

/// The check a save of `form` makes in its own write: the library still
/// holds the form's release as the editor opened on it. The check refuses
/// the save as gone once the release is, and as stale once it changed.
pub fn release_edit_still_as_opened_on(
    sql: &CovenWrite<'_>,
    form: &ReleaseEditForm,
) -> Result<Result<(), bae_records::ReleaseEditRefusal>, DbError> {
    use bae_records::ReleaseEditRefusal as R;
    match seed_on(sql, &form.release_id)? {
        Ok(now) => Ok(if now == form.opened { Ok(()) } else { Err(R::Stale) }),
        Err(ReleaseEditSeedRefusal::ReleaseMissing(_)) => Ok(Err(R::Gone)),
        Err(refusal) => Err(DbError::unreadable(
            format!("release {} as an editor reads it", form.release_id),
            refusal,
        )),
    }
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
            .read(move |sql| release_edit_rows_on(&sql, &read_id))
            .process(move |rows| {
                Ok(seed_of(&release_id, rows)
                    .map(|seed| ReleaseEditForm::opened_on(release_id.clone(), seed)))
            })
            .await
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
            .subscribe_reconfigurable(None, |form, sql| match form {
                None => Ok(None),
                Some(form) => Ok(Some(release_edit_draft_rows_on(&sql, form)?)),
            })
            .process(|rows| {
                rows.map(|rows| rows.map(ReleaseEditDraftRows::draft).transpose()).transpose()
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
            .read(move |sql| release_edit_rows_on(&sql, &release_id))
            .process(|rows| Ok(rows.map(ReleaseEditRows::processed)))
            .await
    }
}
