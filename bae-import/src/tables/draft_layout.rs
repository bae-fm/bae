//! Moving an import draft's CUE sheets onto the sides the files put them on,
//! by the library's layout statement, so that the import pane and the
//! release editor move a sheet's tracks by one rule.

use super::layout_bindings::{json, mediums_json, moves_json, places_json};
use super::*;
use bae_records::records::layout::{laid_out_on, LayoutParams, ReleaseLayout};

/// A track of the draft that a CUE sheet carves, as `:carving` binds it.
#[derive(serde::Serialize)]
struct CarvingJson<'a> {
    sheet_file_id: &'a str,
    track_id: &'a str,
}

impl ImportTables {
    /// The layout the library's layout statement gives `redrawn`'s draft
    /// with its sheet moves made; a draft that moves no sheet keeps its
    /// layout.
    pub(crate) async fn lay_out_sheet_moves(
        &self,
        redrawn: &crate::pane::RedrawnDraft,
    ) -> Result<ReleaseLayout, DbError> {
        let draft = Some(&redrawn.draft);
        let carving: Vec<_> = redrawn
            .draft
            .tracks
            .iter()
            .filter_map(|track| match &track.edit.file {
                crate::AudioFile::SheetSlice { sheet_id, .. } => {
                    Some(CarvingJson { sheet_file_id: sheet_id, track_id: &track.edit.id })
                }
                crate::AudioFile::Standalone { .. } => None,
            })
            .collect();
        let mediums = mediums_json(draft, "an import draft's mediums")?;
        let places = places_json(draft, "an import draft's places")?;
        let carving = json(&carving, "an import draft's carved tracks")?;
        let moves = moves_json(
            redrawn.sheet_moves.iter().map(|(sheet, side)| (sheet.as_str(), *side)),
            "an import draft's sheet moves",
        )?;
        self.store
            .read(move |sql| {
                laid_out_on(
                    &sql,
                    &LayoutParams {
                        release_id: None,
                        applied: true,
                        mediums: &mediums,
                        places: &places,
                        carving: &carving,
                        moves: &moves,
                        edits: "{}",
                    },
                )
            })
            .await
    }
}
