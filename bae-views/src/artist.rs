//! The artist views: the artist list and an artist's detail.
//!
//! Its tables and the triggers keeping them are `artist.sql`.

use super::*;
use crate::album::AlbumAppearanceRow;
use crate::album_detail::{AlbumAppearance, ArtistDetail};
use bae_records::records::files::ArtistImageVersion;

mod list;
pub(crate) use list::{
    artist_key_at_on, artist_sort, artist_window_of, artist_window_read, artists_before_read,
    ArtistListLengthParams,
};
#[cfg(test)]
pub(crate) use list::{artist_window_on, artists_before_on};

impl LibraryViews {
    /// Follow one window of the artist list under `sort`: the rows from a key
    /// of its order on.
    pub fn subscribe_artist_window(
        &self,
        sort: &[ArtistSortCriterion],
        initial: ListWindow,
    ) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<DbArtistSummary>, bae_store::DbError>
    {
        let sort = artist_sort(sort);
        self.store
            .subscribe_reconfigurable(initial, move |window| artist_window_read(&sort, window))
            .process(artist_window_of)
    }

    /// Follow how many artists the list has. The count runs as a separate
    /// query from the windows, because every artist added or removed changes
    /// it.
    pub fn subscribe_artist_count(&self) -> coven::LiveQuery<u64, bae_store::DbError> {
        self.store.subscribe_row(ArtistListLengthParams {}).process(|row| list_count(row.entries))
    }

    /// Follow how many artists come before the row at a key; the caller
    /// changes the key on the same query.
    pub fn subscribe_artist_position(
        &self,
        sort: &[ArtistSortCriterion],
        initial: OrderKey,
    ) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
        let sort = artist_sort(sort);
        self.store
            .subscribe_reconfigurable(initial, move |key| artists_before_read(&sort, key))
            .process(|_, entries| list_count(exactly_one::<coven::ReadAs<i64>, _>(entries)?))
    }

    /// The key of the artist at `position`, or of the last one when the list
    /// ends before it; `None` for an empty list.
    pub async fn artist_key_at(
        &self,
        sort: &[ArtistSortCriterion],
        position: u64,
    ) -> Result<Option<OrderKey>, DbError> {
        let sort = artist_sort(sort);
        self.store.read(move |sql| artist_key_at_on(&sql, &sort, position)).await
    }

    /// Follow the artist `initial` names, or none. The detail view moves to
    /// another artist through the request handle, not another query.
    pub fn subscribe_artist_detail(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, ArtistDetailProjection, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |artist_id| {
                Ok(coven::Maybe(artist_id.as_deref().map(artist_detail_read)))
            })
            .process(|_, reads| {
                let rows = reads.map(artist_detail_rows_of).transpose()?;
                let detail = rows.map(ArtistDetailRows::process).transpose()?.flatten();
                Ok(ArtistDetailProjection { detail })
            })
    }
}

/// One delivery of an artist list window as the list shows it.
pub fn resolve_artist_window(read: WindowRead<DbArtistSummary>) -> crate::ListWindowValue {
    crate::ListWindowValue::of(read, crate::ListRows::Artists)
}

/// One delivery of the artist detail as the detail view shows it.
pub fn resolve_artist_detail_projection(
    projection: ArtistDetailProjection,
) -> Option<ArtistDetail> {
    projection.detail.map(artist_detail_of)
}

/// An artist's page as its rows read it.
fn artist_detail_of(raw: DbArtistDetail) -> ArtistDetail {
    ArtistDetail {
        artist: raw.artist,
        credit_names: raw.credit_names,
        albums: raw.albums.into_iter().map(AlbumAppearance::from_raw).collect(),
        credit_name_count: raw.credit_name_count,
    }
}

/// The reads of [`ArtistPageHead`] and the artist's albums of the artist
/// `artist_id`, which run on one snapshot.
fn artist_detail_read(
    artist_id: &str,
) -> coven::Both<ArtistPageHeadParams, crate::album::ArtistAlbumsParams> {
    coven::Both(
        ArtistPageHeadParams { artist: artist_id.to_string() },
        crate::album::ArtistAlbumsParams { artist: artist_id.to_string() },
    )
}

/// The artist's rows as [`artist_detail_read`] returned them.
fn artist_detail_rows_of(
    (head, albums): (Vec<ArtistPageHeadRow>, Vec<crate::album::ArtistAlbumsRow>),
) -> Result<ArtistDetailRows, DbError> {
    Ok(ArtistDetailRows {
        head: at_most_one::<ArtistPageHeadParams, _>(head)?,
        albums: crate::album::artist_albums_of(albums)?,
    })
}

// The artist `artist` as the artist list shows it, with the other printed
// names it goes by; no row for an artist the list doesn't show.
coven::read!(
    writer ArtistPageHead(artist: String),
    r#"
    SELECT shown.*, names.*
    FROM @crate::artist::ListedArtist AS shown
    JOIN @bae_records::records::artist_credit_names::ArtistOtherNames AS names
      ON names.names_artist_id = :artist
    WHERE shown.listed_id = :artist
"#
);

#[derive(Clone, PartialEq)]
struct ArtistDetailRows {
    head: Option<ArtistPageHeadRow>,
    albums: Vec<AlbumAppearanceRow>,
}

impl ArtistDetailRows {
    /// The artist's page; `None` when the artist list doesn't show the artist.
    fn process(self) -> Result<Option<DbArtistDetail>, DbError> {
        let Some(head) = self.head else {
            return Ok(None);
        };
        let (credit_names, credit_name_count) = head.names.into_names();
        Ok(Some(DbArtistDetail {
            artist: head.shown.into_summary()?,
            credit_names,
            albums: self
                .albums
                .into_iter()
                .map(|row| row.process(false))
                .collect::<Result<_, _>>()?,
            credit_name_count,
        }))
    }
}

// Each artist the artist list shows (`artist_listing`), with its count of the
// albums whose credit lists (`album_artists`) show it, and its image's
// version. A track's credits (`track_artists`) list no artist. Various Artists
// is an `artists` row like any other and lists like one.
coven::fragment!(
    ListedArtist,
    r#"
    SELECT listing.artist_id AS listed_id, listing.album_count, artist.*, image.*
    FROM artist_listing AS listing
    JOIN @bae_records::records::browse_fragments::ArtistRecord AS artist
      ON artist.artist_id = listing.artist_id
    LEFT JOIN @bae_records::records::files::ArtistImageVersion AS image
           ON image.id = listing.artist_id
"#
);

impl ListedArtist {
    /// The artist as its row shows it.
    pub(crate) fn into_summary(self) -> Result<DbArtistSummary, DbError> {
        let artist = self.artist.into_artist()?;
        let version = self.image.and_then(ArtistImageVersion::into_version);
        Ok(DbArtistSummary {
            image: bae_records::artist_image_ref(&artist.id, version),
            artist,
            album_count: self.album_count,
        })
    }
}

// The first `limit` artists library search finds for `pattern`, in name
// order, ties going by id, each with its place in that order, from 1, how many
// artists these are, and the artist as the artist list shows it.
coven::fragment!(
    FoundArtists(pattern: Option<String>, limit: i64) { place: i64, listed: i64 },
    r#"
    SELECT ROW_NUMBER() OVER (ORDER BY picked.name_sort_key, picked.artist_id) AS place,
           COUNT(*) OVER () AS listed, shown.*
    FROM (SELECT listing.artist_id, listing.name_sort_key
          FROM artist_listing AS listing
          WHERE listing.artist_id IN (
              SELECT matched.artist_id
              FROM @crate::album_search::ArtistsFound(pattern: :pattern) AS matched)
          ORDER BY listing.name_sort_key, listing.artist_id
          LIMIT :limit) AS picked
    CROSS JOIN @crate::artist::ListedArtist AS shown ON shown.listed_id = picked.artist_id
"#
);

#[derive(Debug, Clone, PartialEq)]
pub struct ArtistDetailProjection {
    pub detail: Option<DbArtistDetail>,
}

#[cfg(test)]
#[path = "artist_tests.rs"]
mod tests;
