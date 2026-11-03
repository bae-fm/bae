//! The reads that draw a candidate's pane and its prepared cover bytes.

use super::*;

impl ImportTables {
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
    ) -> Result<crate::CandidatePreparedAssets, DbError> {
        let content_hash = content_hash.to_string();
        self.database
            .read(move |sql| super::prepared_asset_rows::load_asset_rows_on(&sql, &content_hash))
            .await
    }
}
