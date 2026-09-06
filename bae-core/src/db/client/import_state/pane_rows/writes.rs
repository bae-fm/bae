//! The reads that draw a candidate's pane and prepare its import.

use super::*;

impl Database {
    pub async fn load_import_candidate_preparation(
        &self,
        content_hash: &str,
    ) -> Result<Option<crate::db::DbCandidateImportPreparation>, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            let Some(state) =
                super::rows::load_states_on(&sql, Some(&content_hash))?.remove(&content_hash)
            else {
                return Ok(None);
            };
            let rows = load_pane_rows_on(&sql, &content_hash)?;
            let source_discogs_artist_ids =
                super::prepared_asset_rows::load_source_artist_ids_on(&sql, &content_hash)?;
            let assets = super::prepared_asset_rows::load_prepared_assets_on(
                &sql,
                &content_hash,
                rows.cover.as_ref(),
            )?;
            Ok(Some(crate::db::DbCandidateImportPreparation {
                file_edit_revision: state.file_edits.revision,
                metadata_revision: state.metadata_revision,
                metadata_provenance: state.metadata_provenance,
                cover: rows.cover,
                draft: rows.draft,
                source_discogs_artist_ids,
                assets,
            }))
        })
        .await
    }

    /// Everything a person settled about one candidate through its pane.
    pub async fn load_import_candidate_pane_rows(
        &self,
        content_hash: &str,
    ) -> Result<DbCandidatePaneRows, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| load_pane_rows_on(&sql, &content_hash))
            .await
    }

    pub async fn load_import_candidate_prepared_assets(
        &self,
        content_hash: &str,
    ) -> Result<crate::import::CandidatePreparedAssets, DbError> {
        let content_hash = content_hash.to_string();
        self.read(move |sql| {
            let rows = load_pane_rows_on(&sql, &content_hash)?;
            super::prepared_asset_rows::load_prepared_assets_on(
                &sql,
                &content_hash,
                rows.cover.as_ref(),
            )
        })
        .await
    }

    /// Forget the last failure — what queueing an import of this candidate
    /// does before the worker takes it.
    pub async fn clear_import_candidate_failure(&self, content_hash: &str) -> Result<(), DbError> {
        let content_hash = content_hash.to_string();
        self.call(move |sql| {
            sql.execute(
                "DELETE FROM import_candidate_failure WHERE content_hash = ?",
                [&content_hash],
            )?;
            Ok(())
        })
        .await
    }
}
