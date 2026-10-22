// ── Credit lines: printed names and joins, one builder for every surface ────
//
// The library holds an album credited "Artist A & B Alias" (Artist B printed
// under one of its other names) and a track credited "Artist A feat. Artist
// B"; its other track has no credits of its own. Every surface prints those
// lines from the same stored credits.

const CREDIT_ARTIST_A: &str = "6d1f3a20-8b4c-4e2a-9f1d-2c3b4a5d6e01";
const CREDIT_ARTIST_B: &str = "6d1f3a20-8b4c-4e2a-9f1d-2c3b4a5d6e02";

struct CreditedLibrary {
    manager: TestManager,
    _temp_dir: TempDir,
    album: DbAlbum,
    release: DbRelease,
    featured: DbTrack,
    plain: DbTrack,
}

async fn credited_library() -> CreditedLibrary {
    let (manager, temp_dir, album, release) = manager_with_release().await;
    for (id, name) in [(CREDIT_ARTIST_A, "Artist A"), (CREDIT_ARTIST_B, "Artist B")] {
        manager
            .database
            .insert_artist(&DbArtist {
                id: id.to_string(),
                name: name.to_string(),
                sort_name: None,
                discogs_artist_id: None,
                musicbrainz_artist_id: None,
                created_at: Utc::now(),
            })
            .await
            .unwrap();
    }
    let now = Utc::now();
    for (artist, position, credit, join) in [
        (CREDIT_ARTIST_A, 0, None, Some(" & ")),
        (CREDIT_ARTIST_B, 1, Some("B Alias"), None),
    ] {
        manager
            .database
            .insert_album_artist(&crate::db::DbAlbumArtist {
                credit: credit.map(crate::credits::CreditName::of),
                join_phrase: join.map(str::to_string),
                ..crate::db::DbAlbumArtist::new(&album.id, artist, position, now)
            })
            .await
            .unwrap();
    }
    let featured = DbTrack::new_test(&release.id, &Uuid::new_v4().to_string(), "Featured", 1);
    let plain = DbTrack::new_test(&release.id, &Uuid::new_v4().to_string(), "Plain", 2);
    manager.database.insert_track(&featured).await.unwrap();
    manager.database.insert_track(&plain).await.unwrap();
    for (artist, position, join) in [(CREDIT_ARTIST_A, 0, Some(" feat. ")), (CREDIT_ARTIST_B, 1, None)]
    {
        manager
            .database
            .insert_track_artist(&crate::db::DbTrackArtist {
                join_phrase: join.map(str::to_string),
                ..crate::db::DbTrackArtist::new(&featured.id, artist, position, now)
            })
            .await
            .unwrap();
    }
    CreditedLibrary {
        manager,
        _temp_dir: temp_dir,
        album,
        release,
        featured,
        plain,
    }
}

/// The lines the album page, the release's track rows, the queue, search and
/// Save As tags print.
struct PrintedLines {
    album_page: String,
    featured_row: String,
    plain_row: String,
    queued_featured: String,
    searched_album: String,
    saved_artist: String,
    saved_album_artist: String,
}

async fn printed_lines(library: &CreditedLibrary) -> PrintedLines {
    let manager = &library.manager;
    let album = manager
        .find_album_detail(&library.album.id)
        .await
        .unwrap()
        .expect("the album");
    let row = |id: &str| {
        album.releases[0]
            .tracks
            .iter()
            .find(|track| track.id == id)
            .expect("the track's row")
            .artist_names
            .clone()
    };
    let queued_featured = manager
        .database
        .track_display(&library.featured.id)
        .await
        .unwrap()
        .expect("the track's display")
        .artist_names;
    let searched_album = manager
        .database
        .search_library("Test Album", 10)
        .await
        .unwrap()
        .albums
        .first()
        .expect("the album is found")
        .artist_name
        .clone();
    let saved = manager
        .resolve_save_tags(&library.release, &library.featured)
        .await
        .unwrap();
    PrintedLines {
        album_page: album.artist_names.clone(),
        featured_row: row(&library.featured.id),
        plain_row: row(&library.plain.id),
        queued_featured,
        searched_album,
        saved_artist: saved.tags.artist,
        saved_album_artist: saved.tags.album_artist,
    }
}

#[tokio::test]
async fn every_surface_prints_the_credits_as_stated() {
    let library = credited_library().await;

    let lines = printed_lines(&library).await;

    assert_eq!(lines.album_page, "Artist A & B Alias");
    assert_eq!(lines.featured_row, "Artist A feat. Artist B");
    assert_eq!(lines.plain_row, "Artist A & B Alias", "a track with no credits of its own");
    assert_eq!(lines.queued_featured, "Artist A feat. Artist B");
    assert_eq!(lines.searched_album, "Artist A & B Alias");
    assert_eq!(lines.saved_artist, "Artist A feat. Artist B");
    assert_eq!(lines.saved_album_artist, "Artist A & B Alias");
}

/// A credit printing another name still links its artist: the album lists
/// Artist B, and Artist B's page lists the album and the name.
#[tokio::test]
async fn a_printed_name_shows_in_place_of_the_artists_while_the_link_names_the_artist() {
    let library = credited_library().await;

    let credits = library
        .manager
        .get_artists_for_album(&library.album.id)
        .await
        .unwrap();
    let detail = library
        .manager
        .get_artist_detail(CREDIT_ARTIST_B)
        .await
        .unwrap()
        .expect("Artist B's page");

    assert_eq!(credits[1].artist.id, CREDIT_ARTIST_B);
    assert_eq!(credits[1].artist.name, "Artist B");
    assert_eq!(credits[1].printed(), "B Alias");
    assert_eq!(detail.albums.len(), 1);
    assert_eq!(
        detail
            .credit_names
            .iter()
            .map(|name| name.credit.name.as_str())
            .collect::<Vec<_>>(),
        ["B Alias"]
    );
}

/// Artist B merged into Artist A: a credit with no printed name of its own
/// prints the survivor's name, one with a printed name keeps it.
#[tokio::test]
async fn a_merged_artists_credit_prints_the_survivor_unless_a_printed_name_was_stored() {
    let library = credited_library().await;
    library
        .manager
        .database
        .execute_local_sql_for_test(&format!(
            "INSERT INTO artist_merges (id, into_artist_id, created_at) \
             VALUES ('{CREDIT_ARTIST_B}', '{CREDIT_ARTIST_A}', '2026-01-01T00:00:00Z')"
        ))
        .await
        .unwrap();

    let lines = printed_lines(&library).await;

    assert_eq!(lines.album_page, "Artist A & B Alias");
    assert_eq!(lines.featured_row, "Artist A feat. Artist A");
}

/// A typed artist added after a picked one prints after ", ", and the last
/// credit's join is not kept: removing the credit after one leaves that one
/// printing none.
#[tokio::test]
async fn an_edit_prints_typed_artists_with_commas_and_drops_the_last_join() {
    let library = credited_library().await;
    let manager = &library.manager;
    let artist_a = crate::import::ExistingArtist::from(
        manager
            .get_artist_by_id(CREDIT_ARTIST_A)
            .await
            .unwrap()
            .expect("Artist A"),
    );
    let edit = |album_artists: Vec<crate::import::Credited>| crate::import::ReleaseUserEdit {
        album_title: "Test Album".to_string(),
        album_artist_assignments: album_artists,
        album_year: None,
        pressing: crate::pressing::Pressing::blank(),
        tracks: vec![
            crate::import::TrackUserEdit {
                title: "Featured".to_string(),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: None,
            },
            crate::import::TrackUserEdit {
                title: "Plain".to_string(),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: None,
            },
        ],
    };

    // The second credit removed: Artist A's " & " no longer prints.
    manager
        .apply_release_metadata_user_edit(
            &library.release.id,
            &edit(vec![crate::import::Credited {
                artist: crate::import::ArtistAssignment::picked(artist_a.clone()),
                join: Some(" & ".to_string()),
            }]),
        )
        .await
        .unwrap();
    assert_eq!(printed_lines(&library).await.album_page, "Artist A");
    let kept = manager.get_artists_for_album(&library.album.id).await.unwrap();
    assert_eq!(kept[0].join, None, "the last credit keeps no join");

    // A typed artist added after it prints after ", ".
    manager
        .apply_release_metadata_user_edit(
            &library.release.id,
            &edit(vec![
                crate::import::Credited::plain(crate::import::ArtistAssignment::picked(artist_a)),
                crate::import::Credited::plain(crate::import::ArtistAssignment::named(
                    "Typed Artist",
                )),
            ]),
        )
        .await
        .unwrap();
    assert_eq!(printed_lines(&library).await.album_page, "Artist A, Typed Artist");
}

/// A credit name is the artist's to keep: renamed, every credit printing it
/// prints the new name; removed, they print the artist's own.
#[tokio::test]
async fn renaming_or_removing_a_credit_name_reaches_every_credit_printing_it() {
    let library = credited_library().await;
    let manager = &library.manager;

    manager
        .rename_artist_credit_name(CREDIT_ARTIST_B, "b alias", "B Other")
        .await
        .unwrap();
    assert_eq!(printed_lines(&library).await.album_page, "Artist A & B Other");

    manager
        .remove_artist_credit_name(CREDIT_ARTIST_B, "b other")
        .await
        .unwrap();
    assert_eq!(printed_lines(&library).await.album_page, "Artist A & Artist B");
    let detail = manager
        .get_artist_detail(CREDIT_ARTIST_B)
        .await
        .unwrap()
        .expect("Artist B's page");
    assert!(detail.credit_names.is_empty());
}

/// A typed credit that is one of an artist's other names is that artist,
/// printed by that name.
#[tokio::test]
async fn a_typed_credit_matching_a_credit_name_is_that_artist() {
    let library = credited_library().await;
    let manager = &library.manager;
    manager
        .add_artist_credit_name(CREDIT_ARTIST_A, "A Pseudonym")
        .await
        .unwrap();

    manager
        .apply_release_metadata_user_edit(
            &library.release.id,
            &crate::import::ReleaseUserEdit {
                album_title: "Test Album".to_string(),
                album_artist_assignments: vec![crate::import::Credited::plain(
                    crate::import::ArtistAssignment::named("a pseudonym"),
                )],
                album_year: None,
                pressing: crate::pressing::Pressing::blank(),
                tracks: vec![
                    crate::import::TrackUserEdit {
                        title: "Featured".to_string(),
                        artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                        file: None,
                    },
                    crate::import::TrackUserEdit {
                        title: "Plain".to_string(),
                        artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                        file: None,
                    },
                ],
            },
        )
        .await
        .unwrap();

    let credits = manager.get_artists_for_album(&library.album.id).await.unwrap();
    assert_eq!(credits.len(), 1);
    assert_eq!(credits[0].artist.id, CREDIT_ARTIST_A);
    assert_eq!(credits[0].printed(), "A Pseudonym");
}
