use super::*;

/// Owned SQL columns; JSON decoding runs after the read connection is released.
#[derive(Clone, PartialEq)]
pub struct AlbumSummaryRow {
    pub(crate) id: String,
    pub(super) title: String,
    pub(super) year: Option<i32>,
    pub(super) is_compilation: bool,
    pub(super) album_credits: String,
    pub(super) release_ids_json: String,
    pub(super) primary_release_id: String,
    pub(super) primary_cover_version: Option<String>,
}

impl AlbumSummaryRow {
    pub fn process(self) -> Result<DbAlbumSummary, DbError> {
        let credits = self.credits()?;
        self.process_with(&credits)
    }

    /// The album's credits, in list order.
    pub fn credits(&self) -> Result<Vec<crate::credits::LibraryCredit>, DbError> {
        super::read::credits_from_json(&self.album_credits)
    }

    /// The summary, its artist line printed from `credits`, the album's own.
    pub fn process_with(
        self,
        credits: &[crate::credits::LibraryCredit],
    ) -> Result<DbAlbumSummary, DbError> {
        let release_ids = serde_json::from_str(&self.release_ids_json).map_err(|error| {
            DbError::unreadable_json("release_ids_json", &self.release_ids_json, error)
        })?;
        Ok(DbAlbumSummary {
            id: self.id,
            title: self.title,
            year: self.year,
            is_compilation: self.is_compilation,
            artist_line: crate::credits::library_line(credits),
            release_ids,
            primary_release_id: self.primary_release_id,
            cover_version: self.primary_cover_version,
        })
    }
}
