use super::*;
use coven::FixedClock;
use coven::SequentialIdProvider;

#[test]
fn affected_roots_maps_changed_paths_to_their_watched_roots() {
    let root_a = PathBuf::from("/music/new rips");
    let root_b = PathBuf::from("/downloads/bandcamp");
    let roots = vec![root_a.clone(), root_b.clone()];

    // A change inside one root flags only that root.
    let changed = [Path::new("/music/new rips/Album/01.flac")];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a.clone()]);

    // Changes under both roots flag both, in roots order, deduped.
    let changed = [
        Path::new("/downloads/bandcamp/X/cover.jpg"),
        Path::new("/music/new rips/Y"),
        Path::new("/music/new rips/Z"),
    ];
    assert_eq!(affected_roots(&changed, &roots), vec![root_a, root_b]);

    // A change outside every watched root flags nothing.
    let changed = [Path::new("/elsewhere/file")];
    assert!(affected_roots(&changed, &roots).is_empty());
}

/// `common_ancestor` derives the local-path root by folding over the
/// files' parent dirs. It must compare path components, not string
/// prefixes, so `/m/Album` and `/m/Album2` collapse to `/m` (a string
/// prefix would wrongly keep `/m/Album`), and an ancestor argument returns
/// itself rather than descending.
#[test]
fn common_ancestor_cases() {
    use std::path::Path;
    // Sibling files share their parent.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/01.flac"), Path::new("/m/Album/02.flac")),
        Path::new("/m/Album")
    );
    // `a` is already an ancestor of `b`: keep `a`.
    assert_eq!(
        common_ancestor(Path::new("/m/Album"), Path::new("/m/Album/Disc1/01.flac")),
        Path::new("/m/Album")
    );
    // Component-wise, not string-prefix: Album vs Album2 don't share /m/Album.
    assert_eq!(
        common_ancestor(Path::new("/m/Album/x"), Path::new("/m/Album2/y")),
        Path::new("/m")
    );
    // Disjoint trees collapse to the root.
    assert_eq!(
        common_ancestor(Path::new("/a/b"), Path::new("/c/d")),
        Path::new("/")
    );
}

/// image_cover_priority decides which folder image wins as the cover when
/// the user makes no explicit pick: a name containing "cover" or "front"
/// (case-insensitive, anywhere in the name) ranks first, everything else
/// second. The fallback sort relies on this ordering.
#[test]
fn image_cover_priority_ranks_front_and_cover_first() {
    assert_eq!(ImportService::image_cover_priority("Cover.jpg"), 0);
    assert_eq!(ImportService::image_cover_priority("front.png"), 0);
    assert_eq!(ImportService::image_cover_priority("FRONT.JPG"), 0);
    assert_eq!(
        ImportService::image_cover_priority("album-front-scan.jpg"),
        0
    );
    assert_eq!(ImportService::image_cover_priority("Back.jpg"), 1);
    assert_eq!(ImportService::image_cover_priority("inlay.png"), 1);
    assert_eq!(ImportService::image_cover_priority("disc1.jpg"), 1);
}

/// Deterministic clock for the `apply_user_edit_to_seed` tests — the
/// exact instant is immaterial to what they assert (artist-row
/// preservation / rebuild), only that the same one feeds every row.
fn test_clock() -> FixedClock {
    FixedClock(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    )
}

// ── apply_identity_choice ──────────────────────────────────────────

fn mb_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::MusicBrainz,
        source_group_id: group.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

fn discogs_id_exact(group: &str, release: &str) -> crate::import::ReleaseIdentity {
    crate::import::ReleaseIdentity {
        source: crate::import::MetadataSource::Discogs,
        source_group_id: group.to_string(),
        source_release_id: Some(release.to_string()),
    }
}

fn mb_release_ref() -> crate::import::MetadataRef {
    crate::import::MetadataRef::new("rel-mb", crate::import::MetadataSource::MusicBrainz)
}

#[test]
fn exact_choice_passes_mapper_output_through() {
    let mapper_output = vec![
        mb_id_exact("rg-mb", "rel-mb"),
        discogs_id_exact("master-d", "rel-d"),
    ];
    let result = apply_identity_choice(
        &mapper_output,
        &crate::import::IdentityChoice::Exact {
            release_ref: mb_release_ref(),
        },
    );
    assert_eq!(result, mapper_output);
}

#[test]
fn approximate_choice_nulls_release_ids_on_every_row() {
    // Both the primary identity row AND any cross-source row from
    // url-rels mirror the user's choice — Approximate means a
    // group-level claim across the board.
    let mapper_output = vec![
        mb_id_exact("rg-mb", "rel-mb"),
        discogs_id_exact("master-d", "rel-d"),
    ];
    let result = apply_identity_choice(
        &mapper_output,
        &crate::import::IdentityChoice::Approximate {
            release_ref: mb_release_ref(),
        },
    );
    assert_eq!(result.len(), 2);
    for id in &result {
        assert!(
            id.source_release_id.is_none(),
            "Approximate must NULL source_release_id, got {id:?}"
        );
    }
    // Group IDs survive — the claim is at the group level.
    assert_eq!(result[0].source_group_id, "rg-mb");
    assert_eq!(result[1].source_group_id, "master-d");
}

#[test]
fn unknown_choice_passes_empty_mapper_output_through() {
    // The file-tag mapper emits an empty identity vec; the choice
    // post-process is a no-op. Confirms Unknown writes zero
    // `release_identities` rows even when paired with a mapper
    // that somehow surfaces rows (defensive — file_tag_mapper
    // never does, but the projection is the algebraic identity).
    let result = apply_identity_choice(&[], &crate::import::IdentityChoice::Unknown);
    assert!(result.is_empty());
}

// ── apply_user_edit_to_seed ────────────────────────────────────────

fn make_seed_album_release_track() -> (
    crate::db::DbAlbum,
    crate::db::DbRelease,
    crate::db::DbTrack,
    crate::db::DbArtist,
) {
    let now = chrono::Utc::now();
    let artist = crate::db::DbArtist {
        id: "artist-orig".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "album-1".to_string(),
        title: "Album Title".to_string(),
        artist_id: artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-001".to_string()),
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
        metadata_source_release_id: Some("rel-mb".to_string()),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Original Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: Some(180000),
        discogs_position: None,
        created_at: now,
    };
    (album, release, track, artist)
}

#[test]
fn user_edit_overrides_seeded_pressing_fields() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "Edited Title".to_string(),
        album_artist_names: vec!["Edited Artist".to_string()],
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            format: Some("Vinyl".to_string()),
            label: Some("Edited Label".to_string()),
            catalog_number: Some("EDIT-1".to_string()),
            country: Some("JP".to_string()),
            barcode: Some("4943674000000".to_string()),
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: "Edited Track".to_string(),
            side: 1,
            track_number: Some(1),
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(album.title, "Edited Title");
    assert_eq!(release.pressing.year, Some(1995));
    assert_eq!(release.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(release.pressing.label.as_deref(), Some("Edited Label"));
    assert_eq!(release.pressing.catalog_number.as_deref(), Some("EDIT-1"));
    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(release.pressing.barcode.as_deref(), Some("4943674000000"));
    assert_eq!(tracks[0].title, "Edited Track");

    // The new album artist gets a placeholder DbArtist row so the
    // import pipeline can canonicalize via find_or_create_artists.
    assert!(artists.iter().any(|a| a.name == "Edited Artist"));
    assert_eq!(
        album.artist_id,
        artists
            .iter()
            .find(|a| a.name == "Edited Artist")
            .unwrap()
            .id
    );
}

#[test]
fn user_edit_can_fill_country_for_approximate_seed() {
    // Approximate seed clears pressing fields; the user can supply
    // them via the editor and the overlay applies the value.
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    // Simulate the Approximate-cleared release row.
    release.pressing = crate::db::Pressing::blank();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec![artists[0].name.clone()],
        pressing: crate::import::PressingEdit {
            country: Some("JP".to_string()),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    assert_eq!(release.pressing.country.as_deref(), Some("JP"));
    assert!(release.pressing.year.is_none());
    assert!(release.pressing.format.is_none());
}

#[test]
fn user_edit_track_count_mismatch_is_an_error() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: "T".to_string(),
        album_artist_names: vec!["A".to_string()],
        pressing: crate::import::PressingEdit::blank(),
        // Two edits but seed has one track.
        tracks: vec![
            crate::import::TrackUserEdit {
                title: "X".to_string(),
                side: 1,
                track_number: Some(1),
                artist_names: vec![],
            },
            crate::import::TrackUserEdit {
                title: "Y".to_string(),
                side: 1,
                track_number: Some(2),
                artist_names: vec![],
            },
        ],
    };

    let err = apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap_err();
    assert!(err.contains("Track count mismatch"), "got: {err}");
}

/// Source-id linkage on artist rows (e.g. `musicbrainz_artist_id`)
/// must survive a user edit that doesn't touch artist names. The
/// editor round-trips an unchanged artist field as the same string
/// it was seeded with, so the apply step must compare and short-
/// circuit rather than rebuild rows from name-only placeholders.
#[test]
fn user_edit_preserves_source_id_artist_rows_when_names_unchanged() {
    let now = chrono::Utc::now();
    // Seeded artist row carrying the MB id the mapper attached.
    let seed_artist = crate::db::DbArtist {
        id: "artist-mb".to_string(),
        name: "Artist Name".to_string(),
        sort_name: Some("Artist Name".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-artist-1".to_string()),
        created_at: now,
    };
    let album = crate::db::DbAlbum {
        id: "album-1".to_string(),
        title: "Album Title".to_string(),
        artist_id: seed_artist.id.clone(),
        year: Some(2020),
        primary_release_id: None,
        is_compilation: false,
        created_at: now,
    };
    let release = crate::db::DbRelease {
        id: "release-1".to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: crate::db::Pressing {
            year: Some(2020),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
        },
        disc_id: None,
        metadata_source: crate::db::ReleaseMetadataSource::MusicBrainz,
        metadata_source_release_id: Some("rel-mb".to_string()),
        remote: true,
        source_folder_name: None,
        content_hash: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let track = crate::db::DbTrack {
        id: "track-1".to_string(),
        release_id: release.id.clone(),
        title: "Track Title".to_string(),
        side: 1,
        track_number: Some(1),
        duration_ms: None,
        discogs_position: None,
        created_at: now,
    };
    // Seeded track credit pointing at the MB-id-bearing artist.
    let seed_track_artist = crate::db::DbTrackArtist::new(
        &track.id,
        &seed_artist.id,
        0,
        "track-artist-1".to_string(),
        now,
    );

    let mut album = album;
    let mut release = release;
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::<crate::db::DbAlbumArtist>::new();
    let mut track_artists = vec![seed_track_artist.clone()];

    // The user changes pressing fields but leaves artist names
    // alone. The track's edit ships `artist_names = []` because
    // the editor's "no override" form maps to empty when the
    // track's credit equals the album's.
    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec![seed_artist.name.clone()],
        pressing: crate::import::PressingEdit {
            year: Some(1995),
            ..crate::import::PressingEdit::blank()
        },
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    // The MB-id-bearing artist row must still exist with its
    // source binding intact — no fresh placeholder created.
    assert_eq!(artists.len(), 1, "no extra placeholder rows expected");
    assert_eq!(
        artists[0].musicbrainz_artist_id.as_deref(),
        Some("mb-artist-1"),
        "MB artist id must survive the edit",
    );
    assert_eq!(
        album.artist_id, seed_artist.id,
        "album.artist_id should still reference the seeded row",
    );

    // Track credit must still reference the seeded artist row.
    assert_eq!(track_artists.len(), 1);
    assert_eq!(track_artists[0].artist_id, seed_artist.id);
}

/// User-renaming an artist must rebuild the credit rows. The new
/// name has no source binding, so the inserted `DbArtist` row
/// carries `None` for both source ids.
#[test]
fn user_edit_renaming_album_artist_rebuilds_credits() {
    let (mut album, mut release, track, seed_artist) = make_seed_album_release_track();
    let mut tracks = vec![track];
    let mut artists = vec![seed_artist.clone()];
    let mut album_artists = Vec::new();
    let mut track_artists = Vec::new();

    let edit = crate::import::ReleaseUserEdit {
        album_title: album.title.clone(),
        album_artist_names: vec!["Different Artist".to_string()],
        pressing: crate::import::PressingEdit::blank(),
        tracks: vec![crate::import::TrackUserEdit {
            title: tracks[0].title.clone(),
            side: tracks[0].side,
            track_number: tracks[0].track_number,
            artist_names: vec![],
        }],
    };

    apply_user_edit_to_seed(
        &edit,
        &mut album,
        &mut release,
        &mut tracks,
        &mut artists,
        &mut album_artists,
        &mut track_artists,
        &test_clock(),
        &SequentialIdProvider::new("seed"),
    )
    .unwrap();

    let new_artist = artists
        .iter()
        .find(|a| a.name == "Different Artist")
        .expect("new placeholder should be inserted");
    assert!(new_artist.musicbrainz_artist_id.is_none());
    assert!(new_artist.discogs_artist_id.is_none());
    assert_eq!(album.artist_id, new_artist.id);
}
