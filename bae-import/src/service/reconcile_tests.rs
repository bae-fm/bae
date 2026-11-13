use super::*;

#[test]
fn artists_replaced_by_edits_are_not_reconciled() {
    let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut artists = vec![
        bae_records::DbArtist {
            id: "artist-referenced".into(),
            name: "Artist Name".into(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: now,
        },
        bae_records::DbArtist {
            id: "artist-replaced".into(),
            name: "Replaced Artist".into(),
            sort_name: None,
            discogs_artist_id: Some("discogs-replaced".into()),
            musicbrainz_artist_id: None,
            created_at: now,
        },
    ];
    let album = bae_records::DbAlbum {
        id: "album-1".into(),
        title: "Album Title".into(),
        year: None,
        is_compilation: false,
        created_at: now,
    };
    let credits =
        [bae_records::DbAlbumArtist::new(&coven::UuidIds, &album.id, "artist-referenced", 0, now)];

    retain_referenced_artists(&mut artists, Some((&album, &credits)), &[], &[], &[], &[]);

    assert_eq!(artists.len(), 1);
    assert_eq!(artists[0].id, "artist-referenced");
}

#[test]
fn existing_album_does_not_retain_its_transient_album_artist() {
    let now = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let mut artists = vec![bae_records::DbArtist {
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
