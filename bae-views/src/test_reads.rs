//! The reads the views' tests check the rows of the views' own tables with,
//! and the records rows they compare them to, through the records' fragments.
//! Only the tests build them.

// The title of the work `work`.
#[cfg(test)]
coven::read!(
    writer WorkTitle(work: String),
    r#"
    SELECT work.title
    FROM @bae_records::records::browse_fragments::WorkSummary AS work
    WHERE work.work_id = :work
"#
);

// The title of the album that holds the release `release`.
#[cfg(test)]
coven::read!(
    writer AlbumTitleOfRelease(release: String),
    r#"
    SELECT album.title
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
    JOIN @bae_records::records::fragments::AlbumTitle AS album
        ON album.album_id = release.album_id
    WHERE release.release_id = :release
"#
);

// For each of the album `album`'s release listing rows, whether its title key
// is the key of `title` in the language this device orders names in.
#[cfg(test)]
coven::read!(
    writer ListingTitleKeyIs(album: String, title: String) { current: bool },
    r#"
    SELECT listing.album_title_sort_key = bae_name_key(:title, sort.language) AS current
    FROM release_listing AS listing
    JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
    WHERE listing.album_id = :album
"#
);

// The album the release `release`'s listing row names, none while it has no
// row.
#[cfg(test)]
coven::read!(
    writer ListedAlbum(release: String) { album_id: Option<String> },
    r#"
    SELECT (SELECT listing.album_id FROM release_listing AS listing
            WHERE listing.release_id = :release) AS album_id
"#
);

// The release `release`'s listing row's media columns.
#[cfg(test)]
coven::read!(
    writer ListedMedia(release: String) { media_empty: bool },
    r#"
    SELECT listing.media_empty, listing.media_key
    FROM release_listing AS listing
    WHERE listing.release_id = :release
"#
);

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

// How many track and work search entries hold a title key other than their
// title's in the language this device orders names in, and how many entries
// there are of each.
#[cfg(test)]
coven::read!(
    writer StaleSearchKeys {
        stale_tracks: i64,
        tracks: i64,
        stale_works: i64,
        works: i64,
    },
    r#"
    SELECT (SELECT COUNT(*) FROM track_search_entry AS entry
            JOIN @bae_records::records::spread_fragments::TrackRow AS track
                ON track.track_id = entry.track_id
            JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
            WHERE entry.title_sort_key IS NOT bae_name_key(track.title, sort.language))
               AS stale_tracks,
           (SELECT COUNT(*) FROM track_search_entry AS entry) AS tracks,
           (SELECT COUNT(*) FROM work_search_entry AS entry
            JOIN @bae_records::records::browse_fragments::WorkSummary AS work
                ON work.work_id = entry.work_id
            JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
            WHERE entry.title_sort_key IS NOT bae_name_key(work.title, sort.language))
               AS stale_works,
           (SELECT COUNT(*) FROM work_search_entry AS entry) AS works
"#
);

// How many releases each storage filter keeps, and their size: every release,
// those this device holds, and those only the cloud holds.
#[cfg(test)]
coven::read!(
    writer StoredTotalsAll { releases: u64, size: u64 },
    r#"
    SELECT COUNT(*) AS releases, COALESCE(SUM(kept.total_size), 0) AS size
    FROM @bae_records::release_storage::ReleaseStorage AS kept
"#
);

#[cfg(test)]
coven::read!(
    writer StoredTotalsLocal { releases: u64, size: u64 },
    r#"
    SELECT COUNT(*) AS releases, COALESCE(SUM(kept.total_size), 0) AS size
    FROM @bae_records::release_storage::ReleaseStorage AS kept
    WHERE kept.stored = 0
"#
);

#[cfg(test)]
coven::read!(
    writer StoredTotalsRemote { releases: u64, size: u64 },
    r#"
    SELECT COUNT(*) AS releases, COALESCE(SUM(kept.total_size), 0) AS size
    FROM @bae_records::release_storage::ReleaseStorage AS kept
    WHERE kept.stored = 1
"#
);
