//! An album's card, which the album grid and an artist's page show, and the
//! albums of an artist's page.

use super::*;
use bae_records::records::query::AlbumSummaryRow;
use bae_store::statement_optional;
use bae_types::credits::CreditLine;

// Each album's card: its summary, and whether another device alone holds the
// audio of its primary release.
sqlive_compose::fragment!(
    AlbumCardRow { primary_release_unplayable: bool },
    r#"
    SELECT summary.*,
           EXISTS (SELECT 1
                   FROM @bae_records::release_storage::ReleaseStorage AS storage
                   WHERE storage.release_id = summary.primary_release
                     AND storage.audio_elsewhere_device IS NOT NULL)
               AS primary_release_unplayable
    FROM @bae_records::records::fragments::AlbumSummary AS summary
"#
);

// The cards of the albums the artist's credits show, in its page's order: by
// year, an unknown year after every year, then by title and id, which
// `album_artist_link`'s artist page index holds, so the read sorts nothing.
sqlive_compose::read!(
    writer ArtistAlbums(artist: String),
    r#"
    SELECT card.*
    FROM album_artist_link AS link
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = link.album_id
    WHERE link.artist_id = :artist
    ORDER BY link.sort_year, link.title_sort_key, link.album_id
"#
);

// The card of the album `album`, as the album grid lists it; no row for an
// album the grid doesn't list.
sqlive_compose::read!(
    writer ListedAlbumCard(album: String),
    r#"
    SELECT card.*
    FROM album_listing AS listing
    CROSS JOIN @crate::album::card::AlbumCardRow AS card ON card.summary_album_id = listing.album_id
    WHERE listing.album_id = :album
"#
);

impl LibraryViews {
    /// The album `album_id` as a media browser's item shows it, read once;
    /// `None` when the album grid doesn't list it.
    pub async fn find_album_item(
        &self,
        album_id: &str,
    ) -> Result<Option<crate::album_detail::AlbumSummary>, DbError> {
        let statement = ListedAlbumCardParams { album: album_id.to_string() };
        let row: Option<DbAlbumSummary> = self
            .store
            .read(move |sql| {
                statement_optional(&sql, &statement)?
                    .map(|row| row.card.summary.into_row())
                    .transpose()
            })
            .process(|row| row.map(AlbumSummaryRow::process).transpose())
            .await?;
        Ok(row.map(crate::album_detail::AlbumSummary::from_raw))
    }
}

/// One album card as read: the album, and in a list grouped by artist, the
/// line of the album's credits that show another artist than the one whose
/// group lists it (none when it has no others).
#[derive(Clone, PartialEq)]
pub(crate) struct AlbumAppearanceRow {
    pub(crate) row: AlbumSummaryRow,
    credited_with: Option<CreditLine>,
    primary_release_unplayable: bool,
}

impl AlbumAppearanceRow {
    /// The card `card`, in a group where the album's other credits print
    /// `credited_with`.
    pub(super) fn of(
        card: AlbumCardRow,
        credited_with: Option<CreditLine>,
    ) -> Result<Self, DbError> {
        Ok(Self {
            row: card.summary.into_row()?,
            credited_with,
            primary_release_unplayable: card.primary_release_unplayable,
        })
    }

    pub(crate) fn process(self, group_by_artist: bool) -> Result<DbAlbumAppearance, DbError> {
        let album = self.row.process()?;
        let display_artist_line =
            if group_by_artist { self.credited_with } else { Some(album.artist_line.clone()) };
        Ok(DbAlbumAppearance {
            album,
            display_artist_line,
            primary_release_unplayable: self.primary_release_unplayable,
        })
    }
}

/// The cards of an artist's page as [`ArtistAlbums`] returned them, in its
/// order.
pub(crate) fn artist_albums_of(
    rows: Vec<ArtistAlbumsRow>,
) -> Result<Vec<AlbumAppearanceRow>, DbError> {
    rows.into_iter().map(|row| AlbumAppearanceRow::of(row.card, None)).collect()
}
