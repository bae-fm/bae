//! The trigger on bae-records' `name_sort` (`name_keys.sql`) that writes
//! every stored name key again when the device's language, or the version of
//! the collation data bae writes the keys with, changes. The keys are the
//! columns the listings' triggers write (`album/listing.sql`, `artist.sql`,
//! `composer.sql`, `composer_detail.sql`, `storage.sql`, `search.sql`).

#[cfg(test)]
use bae_store::WriteStatements;

#[cfg(test)]
sqlive_compose::write!(PlantEarlierAlbumKeys, r#"UPDATE album_listing SET title_sort_key = x'01'"#);

#[cfg(test)]
sqlive_compose::write!(
    PlantEarlierArtistKeys,
    r#"UPDATE artist_listing SET name_sort_key = x'01', sort_name_sort_key = x'01'"#
);

#[cfg(test)]
sqlive_compose::write!(
    PlantEarlierLinkKeys,
    r#"UPDATE album_artist_link SET artist_sort_key = x'01'"#
);

#[cfg(test)]
impl crate::LibraryViews {
    /// Write every stored name key as an earlier build's collation data
    /// wrote it: another key, in the listings and in the album links alike.
    /// The link keys are written directly, because the artist trigger would
    /// otherwise be the only thing that stales them, and a test could not tell
    /// that trigger apart from the rewrite it checks.
    pub(crate) async fn plant_keys_of_earlier_data(&self) -> Result<(), bae_store::DbError> {
        self.store
            .call_bulk(|sql| {
                sql.run(&PlantEarlierAlbumKeysParams {})?;
                sql.run(&PlantEarlierArtistKeysParams {})?;
                sql.run(&PlantEarlierLinkKeysParams {})?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
#[path = "name_keys_tests.rs"]
mod tests;
