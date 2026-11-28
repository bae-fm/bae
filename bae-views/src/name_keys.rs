//! The trigger on bae-records' `name_sort` (`name_keys.sql`) that writes
//! every stored name key again when the device's language, or the version of
//! the collation data bae writes the keys with, changes. The keys are the
//! columns the listings' triggers write (`album/listing.sql`, `artist.sql`,
//! `composer.sql`, `composer_detail.sql`, `storage.sql`, `search.sql`).

#[cfg(test)]
use bae_store::WriteStatements;

#[cfg(test)]
coven::write!(PlantEarlierAlbumKeys, r#"UPDATE album_listing SET title_sort_key = x'01'"#);

#[cfg(test)]
coven::write!(
    PlantEarlierArtistKeys,
    r#"UPDATE artist_listing SET name_sort_key = x'01', sort_name_sort_key = x'01'"#
);

#[cfg(test)]
coven::write!(PlantEarlierLinkKeys, r#"UPDATE album_artist_link SET artist_sort_key = x'01'"#);

#[cfg(test)]
impl crate::LibraryViews {
    /// Write every stored name key as an earlier build's collation data
    /// wrote it: another key, in the listings and in the album links alike.
    /// The link keys are written directly, because the artist trigger would
    /// otherwise be the only thing that stales them, and a test could not tell
    /// that trigger apart from the rewrite it checks.
    pub(crate) async fn plant_keys_of_earlier_data(&self) -> Result<(), bae_store::DbError> {
        self.store
            .call(|sql| {
                sql.run(&PlantEarlierAlbumKeysParams {})?;
                sql.run(&PlantEarlierArtistKeysParams {})?;
                sql.run(&PlantEarlierLinkKeysParams {})?;
                Ok(())
            })
            .await
    }
}

// The albums of the release listing in title key order.
#[cfg(test)]
coven::read!(
    writer ListingAlbumsByTitle,
    r#"
    SELECT listing.album_id
    FROM release_listing AS listing
    ORDER BY listing.album_title_sort_key
"#
);

// The albums linked to the artist `artist`, in title key order.
#[cfg(test)]
coven::read!(
    writer ArtistLinkAlbumsByTitle(artist: String),
    r#"
    SELECT link.album_id
    FROM album_artist_link AS link
    WHERE link.artist_id = :artist
    ORDER BY link.title_sort_key
"#
);

// How many album listing rows, artist listing rows and album artist links hold
// a key other than their name's key with no language, and the version of the
// collation data the device's keys were written with.
#[cfg(test)]
coven::read!(
    writer KeysOtherThanUndetermined {
        stale_titles: i64,
        stale_names: i64,
        stale_links: i64,
        data_version: u32,
    },
    r#"
    SELECT (SELECT COUNT(*) FROM album_listing AS listing
            JOIN @bae_records::records::fragments::AlbumTitle AS album
                ON album.album_id = listing.album_id
            WHERE listing.title_sort_key IS NOT bae_name_key(album.title, 'und'))
               AS stale_titles,
           (SELECT COUNT(*) FROM artist_listing AS listing
            JOIN @bae_records::records::spread_fragments::ArtistRow AS artist
                ON artist.artist_id = listing.artist_id
            WHERE listing.name_sort_key IS NOT bae_name_key(artist.name, 'und')
               OR listing.sort_name_sort_key
                  IS NOT bae_name_key(COALESCE(artist.sort_name, artist.name), 'und'))
               AS stale_names,
           (SELECT COUNT(*) FROM album_artist_link AS link
            JOIN @bae_records::records::spread_fragments::ArtistRow AS artist
                ON artist.artist_id = link.artist_id
            WHERE link.artist_sort_key
                  IS NOT bae_name_key(COALESCE(artist.sort_name, artist.name), 'und'))
               AS stale_links,
           sort.data_version
    FROM @bae_records::records::name_sort::NameSortDataVersion AS sort
"#
);

#[cfg(test)]
#[path = "name_keys_tests.rs"]
mod tests;
