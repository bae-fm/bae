use super::*;

#[test]
fn artists_replaced_by_edits_are_not_reconciled() {
    let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut artists = vec![
        bae_db::DbArtist {
            id: "artist-referenced".into(),
            name: "Artist Name".into(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        },
        bae_db::DbArtist {
            id: "artist-replaced".into(),
            name: "Replaced Artist".into(),
            sort_name: None,
            discogs_artist_id: Some("discogs-replaced".into()),
            musicbrainz_artist_id: None,
            created_at: now,
        },
    ];
    let album = bae_db::DbAlbum {
        id: "album-1".into(),
        title: "Album Title".into(),
        year: None,
        is_compilation: false,
        created_at: now,
    };
    let credits = [bae_db::DbAlbumArtist::new(&album.id, "artist-referenced", 0, now)];

    retain_referenced_artists(&mut artists, Some((&album, &credits)), &[], &[], &[], &[]);

    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].id, "artist-referenced");
}

#[test]
fn existing_album_does_not_retain_its_transient_album_artist() {
    let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut artists = vec![bae_db::DbArtist {
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

/// The track the link rows below credit.
const TRACK: &str = "f2f77437-aa03-4583-8b1c-d12bcf984967";

fn track_artist(artist_id: &str) -> bae_db::DbTrackArtist {
    bae_db::DbTrackArtist {
        track_id: TRACK.to_string(),
        artist_id: artist_id.to_string(),
        position: 3,
        credit: None,
        join_phrase: None,
        created_at: chrono::Utc::now(),
    }
}

#[test]
fn remap_links_rewrites_id_and_preserves_the_rest() {
    let ta = track_artist("parsed-1");
    let map = std::collections::HashMap::from([("parsed-1".to_string(), "db-1".to_string())]);

    let remapped = remap_links(
        std::slice::from_ref(&ta),
        &map,
        "track artist",
        |ta| &ta.artist_id,
        |ta, artist_id| ta.artist_id = artist_id,
    )
    .unwrap();
    assert_eq!(remapped.len(), 1);
    assert_eq!(remapped[0].artist_id, "db-1");
    // Everything other than the remapped artist id carries through.
    assert_eq!(remapped[0].track_id, TRACK);
    assert_eq!(remapped[0].position, 3);
    assert_eq!(remapped[0].created_at, ta.created_at);
}

#[test]
fn remap_links_errors_on_unmapped_id() {
    let ta = track_artist("orphan-track-artist");
    let err = remap_links(
        std::slice::from_ref(&ta),
        &std::collections::HashMap::new(),
        "track artist",
        |ta| &ta.artist_id,
        |ta, artist_id| ta.artist_id = artist_id,
    )
    .unwrap_err();
    assert!(
        matches!(&err, crate::ImportError::Internal { detail } if detail.contains("orphan-track-artist")),
        "error should name the unmapped id: {err}"
    );
}
