//! Reconcile a release's rows, built from its draft, against existing library
//! state: match the release to an existing album — yielding the
//! [`super::PreparedMetadata`] the worker runs the import from. Its artist
//! credits stay credits until the commit resolves them. Its works need no
//! matching: a work's id is its MusicBrainz id's, so a work the library
//! already holds is the row the import writes.

use super::{ImportService, PreparedMetadata};

fn retain_referenced_artists(
    artists: &mut Vec<bae_records::DbArtist>,
    new_album: Option<(&bae_records::DbAlbum, &[bae_records::DbAlbumArtist])>,
    track_artists: &[bae_records::DbTrackArtist],
    release_artist_roles: &[bae_records::DbReleaseArtistRole],
    track_artist_roles: &[bae_records::DbTrackArtistRole],
    work_artists: &[bae_records::DbWorkArtist],
) {
    let album_artists = new_album
        .into_iter()
        .flat_map(|(_, links)| links.iter().map(|link| link.artist_id.as_str()));
    let referenced: std::collections::HashSet<&str> = album_artists
        .chain(track_artists.iter().map(|link| link.artist_id.as_str()))
        .chain(release_artist_roles.iter().map(|role| role.artist_id.as_str()))
        .chain(track_artist_roles.iter().map(|role| role.artist_id.as_str()))
        .chain(work_artists.iter().map(|link| link.artist_id.as_str()))
        .collect();
    artists.retain(|artist| referenced.contains(artist.id.as_str()));
}

impl ImportService {
    /// Reconcile the release's rows against existing library state — yielding
    /// the [`PreparedMetadata`] the worker runs the import from. Pure DB reads,
    /// no network.
    ///
    /// A release joins the library album `picked_album` names, else the one its
    /// records name; one with neither — read from its files or from nothing —
    /// lands on a fresh album. `picked` are the artist rows that are library artists a
    /// person picked.
    ///
    /// Artists are not decided here. Every artist link keeps naming its credit,
    /// and the commit resolves the credits inside its own transaction, against
    /// the library as it stands when the release is written.
    pub(super) async fn reconcile_prepared_release(
        &self,
        parsed: bae_records::ReleaseRows,
        records: Vec<bae_catalogs::records::ReleaseRecord>,
        picked_album: Option<String>,
        picked_artists: std::collections::HashSet<String>,
    ) -> Result<PreparedMetadata, crate::ImportError> {
        let bae_records::ReleaseRows {
            album: mut db_album,
            release: mut db_release,
            mediums,
            tracks: track_rows,
            mut artists,
            mut album_artists,
            track_artists,
            work_graph,
            release_artist_roles,
            track_artist_roles,
        } = parsed;

        let album_title = db_album.title.clone();

        // The album the person picked, else the one the records name.
        let existing_album_id = match picked_album {
            Some(album_id) => Some(album_id),
            None => self.records.album_records_name(&records, &[]).await?,
        };
        match &existing_album_id {
            Some(album_id) => {
                db_release.album_id = album_id.clone();
                // The album is already in the library with its own artists;
                // this release's album credits are not written.
                album_artists.clear();
            }
            // A release group is one album: a new album for one takes the id
            // derived from the group.
            None => {
                if let Some(group_album_id) = bae_records::identity::album_id_for_records(&records)
                {
                    db_album.id = group_album_id;
                    db_release.album_id = db_album.id.clone();
                    for album_artist in &mut album_artists {
                        album_artist.album_id = db_album.id.clone();
                    }
                }
            }
        }

        retain_referenced_artists(
            &mut artists,
            existing_album_id.is_none().then_some((&db_album, album_artists.as_slice())),
            &track_artists,
            &release_artist_roles,
            &track_artist_roles,
            &work_graph.work_artists,
        );
        let picked_artists = artists
            .iter()
            .filter(|artist| picked_artists.contains(&artist.id))
            .map(|artist| artist.id.clone())
            .collect();

        Ok(PreparedMetadata {
            db_album,
            db_release,
            mediums,
            track_rows,
            existing_album_id,
            track_artists,
            album_artists,
            work_graph,
            release_artist_roles,
            track_artist_roles,
            artist_credits: artists,
            picked_artists,
            records,
            album_title,
        })
    }
}

#[cfg(test)]
#[path = "reconcile_tests.rs"]
mod tests;
