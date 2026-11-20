//! The import's refusal of a catalog release it cannot read onto a folder's
//! audio, which names the release's tied sides and discs by their headings.

use super::ImportTables;
use crate::ImportError;
use bae_catalogs::reading::ReadingRefusal;
use bae_catalogs::source_release::SourceRelease;

impl ImportTables {
    /// The refusal `refusal` of `release` as the import's error. A tie names
    /// every side and disc of the tied mediums by its heading on `release`,
    /// which bae-records' placement statement gives over the release's
    /// mediums, so the error names them as a stored release's tracklist
    /// would.
    pub async fn reading_refusal(
        &self,
        release: &SourceRelease,
        refusal: ReadingRefusal,
    ) -> ImportError {
        match refusal {
            ReadingRefusal::TrackCount { folder, release } => {
                ImportError::MetadataTrackCount { folder_tracks: folder, release_tracks: release }
            }
            ReadingRefusal::Tied { mediums } => {
                let held = release.held_mediums();
                match self
                    .store
                    .read(move |sql| bae_records::records::side_headings_on(&sql, &held, &mediums))
                    .await
                {
                    Ok(sides) => ImportError::TiedMediums { sides },
                    Err(error) => ImportError::Database(error),
                }
            }
            ReadingRefusal::Unreadable { catalog, detail } => {
                bae_catalogs::CatalogError::SourceData { catalog, detail }.into()
            }
        }
    }
}

#[cfg(test)]
#[path = "reading_refusals_tests.rs"]
mod tests;
