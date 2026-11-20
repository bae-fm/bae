//! What a save of a release editor's form writes of the layout of the
//! release's tracks: the layout the form gives them, the discs the person
//! picked for CUE sheets, which the layout statements of bae-records lay
//! out for the draft as well, and the audio files they picked for the
//! sheets' `FILE` lines.

use super::draft::FormParams;
use super::*;
use bae_records::records::layout::{honored_picks_on, laid_out_on, ReleaseLayout};

/// The layout a save of `form` writes over the release's tracks, as `sql`
/// finds the release; `None` where the form picks no disc for a CUE sheet,
/// since an applied catalog writes its own places with its rows.
pub fn release_edit_layout_on(
    sql: &impl coven::SqlRead,
    form: &ReleaseEditForm,
) -> Result<Option<ReleaseLayout>, DbError> {
    if form.sheet_discs.is_empty() {
        return Ok(None);
    }
    let params = FormParams::of(form)?;
    Ok(Some(laid_out_on(sql, &params.layout())?))
}

/// Store the discs the person picked for CUE sheets that a save of `form`
/// gives the sheets, each as the sheet's disc decision, and the audio files
/// they picked for the sheets' `FILE` lines, each as the line's binding.
pub fn store_release_edit_sheet_decisions_on(
    sql: &CovenWrite<'_>,
    form: &ReleaseEditForm,
) -> Result<(), DbError> {
    use super::super::release_sheet_decisions as decisions;
    decisions::store_sheet_binding_decisions_on(sql, &form.sheet_files)?;
    if form.sheet_discs.is_empty() {
        return Ok(());
    }
    let params = FormParams::of(form)?;
    let honored = honored_picks_on(sql, &params.layout())?;
    decisions::store_sheet_disc_decisions_on(sql, &honored)
}
