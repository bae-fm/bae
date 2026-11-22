//! What a save of a release editor's form writes of the layout of the
//! release's tracks: the layout the form gives them, the sides the person
//! picked for CUE sheets, which the layout statements of bae-records lay
//! out for the draft as well, and the audio files they picked for the
//! sheets' `FILE` lines.

use super::draft::FormParams;
use super::*;
use crate::folder_scanner::SheetPlace;
use bae_records::records::layout::{honored_picks_on, laid_out_on, ReleaseLayout};

/// The layout a save of `form` writes over the release's tracks, as `sql`
/// finds the release; `None` where the form picks no place for a CUE sheet
/// and binds no line to no audio, since an applied catalog writes its own
/// places with its rows.
pub fn release_edit_layout_on(
    sql: &impl coven::SqlRead,
    form: &ReleaseEditForm,
) -> Result<Option<ReleaseLayout>, DbError> {
    let deletes_tracks = form.sheet_files.iter().any(|pick| pick.audio_file_id.is_none());
    if form.sheet_places.is_empty() && !deletes_tracks {
        return Ok(None);
    }
    let params = FormParams::of(form)?;
    Ok(Some(laid_out_on(sql, &params.layout())?))
}

/// Store the sheet places and the bindings the form picked for its `FILE`
/// lines.
pub fn store_release_edit_sheet_decisions_on(
    sql: &CovenWrite<'_>,
    form: &ReleaseEditForm,
) -> Result<(), DbError> {
    use super::super::release_sheet_decisions as decisions;
    decisions::store_sheet_binding_decisions_on(sql, &form.sheet_files)?;
    if form.sheet_places.is_empty() {
        return Ok(());
    }
    let params = FormParams::of(form)?;
    let honored = honored_picks_on(sql, &params.layout())?;
    let places = honored.iter().map(|(sheet, side)| (sheet.as_str(), SheetPlace::On(*side))).chain(
        form.sheet_places.iter().filter_map(|(sheet, place)| {
            (*place == SheetPlace::Ignored).then_some((sheet.as_str(), *place))
        }),
    );
    decisions::store_sheet_place_decisions_on(sql, places)
}
