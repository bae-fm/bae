//! What a save of a release editor's form starts from, which one statement
//! reads: the release as an editor opening now reads it, the rows the save
//! writes over, the album its records file it under, what the form does to
//! its CUE sheets' tracks, their stored audio, its files and the person's
//! earlier decisions about its sheets. The save reads the CUE sheets and
//! the audio they play after that read, so its write runs the statement
//! again and refuses when the library holds any of it differently.

use super::draft::FormParams;
use super::*;
use crate::folder_scanner::CandidateFileEdits;
use bae_records::records::release_edit_rows::{DbReleaseSaveFacts, ReleaseToEdit, UNEDITED};
use bae_store::statement_optional;

// What a save of a form of the release `release` starts from. No row for a
// release the library does not hold.
coven::read!(
    writer SaveInputs(release: String, unedited: String, relinked: bool, records: String, edits: String),
    r#"
    SELECT shown.*, save.*, decided.*
    FROM @bae_records::records::release_edit_rows::EditorRelease(
             release: :release, unedited: :unedited) AS shown
    JOIN @bae_records::records::release_edit_rows::ReleaseSaveRows(
             release: :release, relinked: :relinked, records: :records,
             edits: :edits) AS save
    JOIN @crate::tables::release_sheet_decisions::ReleaseSheetDecisions(
             release: :release) AS decided
"#
);

/// The statement of the inputs of a save of `form`.
fn statement_of(form: &ReleaseEditForm) -> Result<SaveInputsParams, DbError> {
    let params = FormParams::of(form)?;
    Ok(SaveInputsParams {
        release: params.release,
        unedited: UNEDITED.to_owned(),
        relinked: params.relinked,
        records: params.records,
        edits: params.edits,
    })
}

/// What a save of a release editor's form starts from.
pub struct ReleaseSaveInputs {
    /// The seed an editor opening on the release now holds.
    pub seed: ReleaseEditSeed,
    /// The release, its album and its tracks in release order.
    pub release: ReleaseToEdit,
    /// The release's own records.
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
    /// The albums the save moves between, what the form does to the
    /// release's sheet tracks, their stored audio and the release's files.
    pub facts: DbReleaseSaveFacts,
    /// The person's earlier decisions about the release's CUE sheets.
    pub decisions: CandidateFileEdits,
}

impl ReleaseSaveInputs {
    /// The inputs of a save of the release `release_id` from the statement's
    /// row, or why there are none.
    fn of(
        release_id: &str,
        row: Option<SaveInputsRow>,
    ) -> Result<Result<Self, ReleaseEditSeedRefusal>, DbError> {
        let Some(SaveInputsRow { shown, save, decided }) = row else {
            return Ok(Err(ReleaseEditSeedRefusal::ReleaseMissing(release_id.to_string())));
        };
        let shown = shown.decoded()?;
        let release = shown.to_edit();
        let records = shown.detail.records.clone();
        let seed = match super::seed::release_edit_seed(shown.into()) {
            Ok(seed) => seed,
            Err(unreadable) => return Ok(Err(unreadable.into())),
        };
        Ok(Ok(Self {
            seed,
            release,
            records,
            facts: save.decoded()?,
            decisions: decided.decoded()?,
        }))
    }

    /// Whether `now` holds what these inputs hold. Whether coven has
    /// finished moving a file to the cloud is no change: a save reads each
    /// file from wherever coven has it.
    fn unchanged_in(&self, now: &Self) -> bool {
        let Self { seed, release, records, facts, decisions } = self;
        let DbReleaseSaveFacts { filed, dropped, added, first_medium, audio, files } = facts;
        *seed == now.seed
            && *release == now.release
            && *records == now.records
            && *decisions == now.decisions
            && *filed == now.facts.filed
            && *dropped == now.facts.dropped
            && *added == now.facts.added
            && *first_medium == now.facts.first_medium
            && *audio == now.facts.audio
            && files.len() == now.facts.files.len()
            && files.iter().zip(&now.facts.files).all(|(read, held)| {
                bae_records::DbFile { stored: held.stored, ..read.clone() } == *held
            })
    }
}

/// The check a save of `form` makes in its own write: the library holds
/// what the save read before it, `read`. The check refuses the save as gone
/// once the release is, and as stale once any of what it read changed.
pub fn release_save_inputs_unchanged_on(
    sql: &CovenWrite<'_>,
    form: &ReleaseEditForm,
    read: &ReleaseSaveInputs,
) -> Result<Result<(), bae_records::ReleaseEditRefusal>, DbError> {
    use bae_records::ReleaseEditRefusal as R;
    let row = statement_optional(sql, &statement_of(form)?)?;
    match ReleaseSaveInputs::of(&form.release_id, row)? {
        Ok(now) => Ok(if read.unchanged_in(&now) { Ok(()) } else { Err(R::Stale) }),
        Err(ReleaseEditSeedRefusal::ReleaseMissing(_)) => Ok(Err(R::Gone)),
        Err(refusal) => Err(DbError::unreadable(
            format!("release {} as an editor reads it", form.release_id),
            refusal,
        )),
    }
}

impl ImportTables {
    /// What a save of `form` starts from, from one read.
    pub async fn read_release_save_inputs(
        &self,
        form: &ReleaseEditForm,
    ) -> Result<Result<ReleaseSaveInputs, ReleaseEditSeedRefusal>, DbError> {
        let statement = statement_of(form)?;
        let release_id = form.release_id.clone();
        self.store
            .read(move |sql| statement_optional(&sql, &statement))
            .process(move |row| ReleaseSaveInputs::of(&release_id, row))
            .await
    }
}
