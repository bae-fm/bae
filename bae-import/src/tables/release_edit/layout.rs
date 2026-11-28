//! What a save of a release editor's form writes of the layout of the
//! release's tracks: the layout the form gives them, the sides the person
//! picked for CUE sheets, which the layout statements of bae-records lay
//! out for the draft as well, and the audio files they picked for the
//! sheets' `FILE` lines.

use super::draft::FormParams;
use super::*;
use crate::folder_scanner::SheetPlace;
use bae_records::records::layout::{laid_out_on, LaidRelease};
use bae_types::mediums::MediumSide;

/// The layout a save of `form` writes over the release's tracks.
pub fn release_edit_layout_on(
    sql: &impl bae_store::WriterRead,
    form: &ReleaseEditForm,
) -> Result<LaidRelease, DbError> {
    let params = FormParams::of(form)?;
    laid_out_on(sql, &params.layout())
}

/// Store the sheet places and the bindings the form picked for its `FILE`
/// lines: `honored` holds the picked sides the save's layout
/// ([`release_edit_layout_on`]) gave their sheets.
pub fn store_release_edit_sheet_decisions_on(
    sql: &WriteContext<'_>,
    form: &ReleaseEditForm,
    bindings: &[bae_records::records::sheet_files::SheetFilePick],
    honored: &[(String, MediumSide)],
) -> Result<(), DbError> {
    use super::super::release_sheet_decisions as decisions;
    decisions::store_sheet_binding_decisions_on(sql, bindings)?;
    if form.sheet_places.is_empty() {
        return Ok(());
    }
    let places = honored
        .iter()
        .map(|(sheet, side)| (sheet.as_str(), SheetPlace::On(*side)))
        .chain(form.ignored_sheet_ids().map(|sheet| (sheet, SheetPlace::Ignored)));
    decisions::store_sheet_place_decisions_on(sql, places)
}
