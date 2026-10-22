//! Reconcile a release's rows, built from its draft, against existing library
//! state: match the release to an existing album, and resolve its works —
//! yielding the [`super::PreparedMetadata`] the worker runs the import from.
//! Its artist credits stay credits until the commit resolves them.

use std::collections::HashMap;

use super::{ImportService, PreparedMetadata};
use crate::import::handle::remap_links;
use crate::import::WorkGraphRows;

fn retain_referenced_artists(
    artists: &mut Vec<crate::db::DbArtist>,
    new_album: Option<(&crate::db::DbAlbum, &[crate::db::DbAlbumArtist])>,
    track_artists: &[crate::db::DbTrackArtist],
    release_artist_roles: &[crate::db::DbReleaseArtistRole],
    track_artist_roles: &[crate::db::DbTrackArtistRole],
    work_artists: &[crate::db::DbWorkArtist],
) {
    let album_artists = new_album
        .into_iter()
        .flat_map(|(_, links)| links.iter().map(|link| link.artist_id.as_str()));
    let referenced: std::collections::HashSet<&str> = album_artists
        .chain(track_artists.iter().map(|link| link.artist_id.as_str()))
        .chain(
            release_artist_roles
                .iter()
                .map(|role| role.artist_id.as_str()),
        )
        .chain(
            track_artist_roles
                .iter()
                .map(|role| role.artist_id.as_str()),
        )
        .chain(work_artists.iter().map(|link| link.artist_id.as_str()))
        .collect();
    artists.retain(|artist| referenced.contains(artist.id.as_str()));
}

impl ImportService {
    /// Reconcile the release's rows against existing library state — yielding
    /// the [`PreparedMetadata`] the worker runs the import from. Pure DB reads
    /// and string remapping — no network.
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
        parsed: crate::import::ReleaseRows,
        records: Vec<crate::import::ReleaseRecord>,
        picked_album: Option<String>,
        picked_artists: std::collections::HashSet<String>,
        prepared_artist_images: Vec<crate::import::PreparedArtistImage>,
    ) -> Result<PreparedMetadata, crate::import::ImportError> {
        let library_manager = &self.library_manager;

        let crate::import::ReleaseRows {
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

        // The album the person picked, else the one the records name. A
        // picked album deleted since fails the commit on the release's
        // foreign key.
        let existing_album_id = match picked_album {
            Some(album_id) => Some(album_id),
            None => {
                library_manager
                    .find_existing_album_for_import(&records)
                    .await?
            }
        };
        match &existing_album_id {
            Some(album_id) => {
                db_release.album_id = album_id.clone();
                // The album is already in the library with its own artists;
                // this release's album credits are not written.
                album_artists.clear();
            }
            // A new album for a release group is the group's album on every
            // device, so two devices importing into one group while apart
            // write one album.
            None => {
                if let Some(group_album_id) = crate::db::identity::album_id_for_records(&records) {
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
            existing_album_id
                .is_none()
                .then_some((&db_album, album_artists.as_slice())),
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

        // A work performed by an already-imported release keeps that release's
        // `works` row, so every link this import writes points at the resolved id
        // and only the works new to the library are inserted.
        let resolved_works = library_manager
            .resolve_works_for_import(&work_graph.works)
            .await?;
        let work_id_map: HashMap<String, String> = work_graph
            .works
            .iter()
            .zip(resolved_works.ids.iter())
            .map(|(work, id)| (work.id.clone(), id.clone()))
            .collect();

        // A work_artists row points at both an artist and a work, and a
        // work_parts row at two works: the work ends are remapped here, the
        // artist end when the commit resolves the credits.
        let remapped_work_artists = remap_links(
            &work_graph.work_artists,
            &work_id_map,
            "work artist work",
            |link| &link.work_id,
            |link, work_id| link.work_id = work_id,
        )?;
        let work_parts_by_parent = remap_links(
            &work_graph.work_parts,
            &work_id_map,
            "work part parent",
            |part| &part.parent_work_id,
            |part, work_id| part.parent_work_id = work_id,
        )?;
        let remapped_work_parts = remap_links(
            &work_parts_by_parent,
            &work_id_map,
            "work part child",
            |part| &part.child_work_id,
            |part, work_id| part.child_work_id = work_id,
        )?;
        let remapped_track_works = remap_links(
            &work_graph.track_works,
            &work_id_map,
            "track work",
            |link| &link.work_id,
            |link, work_id| link.work_id = work_id,
        )?;

        Ok(PreparedMetadata {
            db_album,
            db_release,
            mediums,
            track_rows,
            selected_cover: None,
            remote_cover_image: None,
            embedded_cover: None,
            existing_album_id,
            track_artists,
            album_artists,
            work_graph: WorkGraphRows {
                works: resolved_works.inserts,
                work_artists: remapped_work_artists,
                work_parts: remapped_work_parts,
                track_works: remapped_track_works,
            },
            release_artist_roles,
            track_artist_roles,
            artist_credits: artists,
            picked_artists,
            prepared_artist_images,
            records,
            album_title,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artists_replaced_by_edits_are_not_reconciled() {
        let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let mut artists = vec![
            crate::db::DbArtist {
                id: "artist-referenced".into(),
                name: "Artist Name".into(),
                sort_name: None,
                discogs_artist_id: None,
                musicbrainz_artist_id: None,
                created_at: now,
            },
            crate::db::DbArtist {
                id: "artist-replaced".into(),
                name: "Replaced Artist".into(),
                sort_name: None,
                discogs_artist_id: Some("discogs-replaced".into()),
                musicbrainz_artist_id: None,
                created_at: now,
            },
        ];
        let album = crate::db::DbAlbum {
            id: "album-1".into(),
            title: "Album Title".into(),
            year: None,
            is_compilation: false,
            created_at: now,
        };
        let credits = [crate::db::DbAlbumArtist::new(
            &album.id,
            "artist-referenced",
            0,
            now,
        )];

        retain_referenced_artists(&mut artists, Some((&album, &credits)), &[], &[], &[], &[]);

        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].id, "artist-referenced");
    }

    #[test]
    fn existing_album_does_not_retain_its_transient_album_artist() {
        let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let mut artists = vec![crate::db::DbArtist {
            id: "artist-album-only".into(),
            name: "Album Artist".into(),
            sort_name: None,
            discogs_artist_id: Some("discogs-album-only".into()),
            musicbrainz_artist_id: None,
            created_at: now,
        }];
        retain_referenced_artists(&mut artists, None, &[], &[], &[], &[]);

        assert!(
            artists.is_empty(),
            "an existing album discards the parsed album and its artist credits"
        );
    }
}
