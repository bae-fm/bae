//! The reads that draw a candidate's pane and prepare its import.

use super::*;

impl ImportTables {
    /// The rows an import of the candidate `content_hash` consumes, or the
    /// release this device already imported from its files, read in one
    /// snapshot; `None` with no state stored for it.
    pub async fn load_import_candidate_preparation(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbPreparationRead>, DbError> {
        let content_hash = content_hash.to_string();
        self.database
            .read(move |sql| {
                let Some(state) =
                    super::rows::load_states_on(&sql, CandidateRows::One(&content_hash))?
                        .remove(&content_hash)
                else {
                    return Ok(None);
                };
                if let Some(release_id) = sql
                    .query_row(
                        "SELECT release_id FROM local_release_origin WHERE content_hash = ?",
                        [&content_hash],
                        |row| row.get(0),
                    )
                    .optional()?
                {
                    return Ok(Some(crate::db::DbPreparationRead::Imported { release_id }));
                }
                let rows = load_pane_rows_on(&sql, &content_hash)?;
                let assets = super::prepared_asset_rows::load_prepared_assets_on(
                    &sql,
                    &content_hash,
                    rows.cover.as_ref(),
                )?;
                Ok(Some(crate::db::DbPreparationRead::Prepared(
                    crate::db::DbCandidateImportPreparation {
                        file_edit_revision: state.file_edits.revision,
                        metadata_revision: state.metadata_revision,
                        metadata_provenance: state.metadata_provenance,
                        release_link: state.release_link,
                        cover: rows.cover,
                        draft: rows.draft,
                        assets,
                    },
                )))
            })
            .await
    }

    /// Everything a person settled about one candidate through its pane.
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<DbCandidatePaneRows, DbError> {
        let content_hash = content_hash.to_string();
        self.database.read(move |sql| load_pane_rows_on(&sql, &content_hash)).await
    }

    pub async fn load_import_candidate_prepared_assets(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::CandidatePreparedAssets, DbError> {
        let content_hash = content_hash.to_string();
        self.database
            .read(move |sql| {
                let rows = load_pane_rows_on(&sql, &content_hash)?;
                super::prepared_asset_rows::load_prepared_assets_on(
                    &sql,
                    &content_hash,
                    rows.cover.as_ref(),
                )
            })
            .await
    }
}
