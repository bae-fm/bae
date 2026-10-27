const SEARCHED_ARTIST: &str = "0373bb67-3abb-48e7-8a5b-6d29ef24308e";
const SEARCHED_ALBUM: &str = "e307f0f5-e518-4d5e-bea8-83e439780ca0";
const SEARCHED_TRACK: &str = "b2140894-e938-4143-9c9a-5e054c2d7dde";
const SEARCHED_WORK: &str = "cef1d7ca-5e1b-4144-9e95-3ece6a68725e";

/// One album by one artist, on one release of one track performing one work
/// the artist wrote; every title and name starts with a letter outside ASCII.
const SEARCHED_LIBRARY: &str = "
    INSERT INTO artists (id, name, name_key, created_at) VALUES
        ('0373bb67-3abb-48e7-8a5b-6d29ef24308e', 'Åsel Artist Name', 'asel artist name', '2026-01-01T00:00:00Z');
    INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES
        ('e307f0f5-e518-4d5e-bea8-83e439780ca0', 'Écrin Album Title', 2026, 0, '2026-01-01T00:00:00Z');
    INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES
        ('cd1bc541-a0b2-4440-8a6b-b9c1ffa33f67', 'e307f0f5-e518-4d5e-bea8-83e439780ca0', '0373bb67-3abb-48e7-8a5b-6d29ef24308e', 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('45397b28-ee0e-4e87-921e-4fd7f9f07478', 'e307f0f5-e518-4d5e-bea8-83e439780ca0', '2026-01-01T00:00:00Z', 0);
    INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES
        ('941c9538-4220-4ff3-a5ed-a8bac3a6067d', '45397b28-ee0e-4e87-921e-4fd7f9f07478', 1, 'cd', 1, '2026-01-01T00:00:00Z');
    INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES
        ('b2140894-e938-4143-9c9a-5e054c2d7dde', '45397b28-ee0e-4e87-921e-4fd7f9f07478', 'Übel Track Title', 1, 1, 1000, '2026-01-01T00:00:00Z', '941c9538-4220-4ff3-a5ed-a8bac3a6067d');
    INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES
        ('cef1d7ca-5e1b-4144-9e95-3ece6a68725e', 'Œvra Work Title', 'work', 'mb-work-oevra', '2026-01-01T00:00:00Z');
    INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES
        ('47d0b5cc-ed3e-4deb-b3d4-20ed4268ec58', 'cef1d7ca-5e1b-4144-9e95-3ece6a68725e', '0373bb67-3abb-48e7-8a5b-6d29ef24308e', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
    INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES
        ('d2239bd6-e0b8-491c-8383-f444078a0cf1', 'b2140894-e938-4143-9c9a-5e054c2d7dde', 'cef1d7ca-5e1b-4144-9e95-3ece6a68725e', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
";

async fn exec_searched(db: &Database, sql: &'static str) {
    db.call(move |conn| conn.execute_batch(sql).map_err(DbError::from))
        .await
        .unwrap();
}

/// What a search for `query` finds: the ids of its albums, tracks, artists,
/// composers and works.
async fn found(db: &Database, query: &str) -> [Vec<String>; 5] {
    let results = db.search_library(query, 10).await.unwrap();
    [
        results.albums.into_iter().map(|album| album.id).collect(),
        results.tracks.into_iter().map(|track| track.id).collect(),
        results
            .artists
            .into_iter()
            .map(|artist| artist.artist.id)
            .collect(),
        results
            .composers
            .into_iter()
            .map(|composer| composer.artist.id)
            .collect(),
        results.works.into_iter().map(|work| work.work.id).collect(),
    ]
}

#[tokio::test]
async fn search_library_ignores_case_outside_ascii() {
    let (db, _tmp) = super::temp_db().await;
    exec_searched(&db, SEARCHED_LIBRARY).await;

    let ids = |id: &str| vec![id.to_string()];
    assert_eq!(found(&db, "écrin").await[0], ids(SEARCHED_ALBUM));
    assert_eq!(found(&db, "ÉCRIN").await[0], ids(SEARCHED_ALBUM));
    assert_eq!(found(&db, "übel").await[1], ids(SEARCHED_TRACK));
    let artist = found(&db, "åsel").await;
    assert_eq!(artist[0], ids(SEARCHED_ALBUM), "the album by its artist's name");
    assert_eq!(artist[2], ids(SEARCHED_ARTIST));
    assert_eq!(artist[3], ids(SEARCHED_ARTIST));
    assert_eq!(found(&db, "œvra").await[4], ids(SEARCHED_WORK));
    // Folding is case only: an unaccented query doesn't find accented text.
    assert!(found(&db, "ecrin").await[0].is_empty());
}

#[tokio::test]
async fn search_library_reads_wildcards_as_themselves() {
    let (db, _tmp) = super::temp_db().await;
    exec_searched(&db, SEARCHED_LIBRARY).await;
    exec_searched(
        &db,
        "UPDATE albums SET title = 'Écrin*[x]?Album' WHERE id = 'e307f0f5-e518-4d5e-bea8-83e439780ca0';",
    )
    .await;

    assert_eq!(found(&db, "n*[x]?a").await[0], vec![SEARCHED_ALBUM.to_string()]);
    assert!(found(&db, "nx").await[0].is_empty());
    assert!(found(&db, "n?[").await[0].is_empty());
    assert!(found(&db, "*album").await[0].is_empty());
}

#[tokio::test]
async fn search_library_follows_names_as_they_change() {
    let (db, _tmp) = super::temp_db().await;
    exec_searched(&db, SEARCHED_LIBRARY).await;

    exec_searched(
        &db,
        "UPDATE artists SET name = 'Renamed Artist Name' WHERE id = '0373bb67-3abb-48e7-8a5b-6d29ef24308e';",
    )
    .await;
    assert!(found(&db, "åsel").await[0].is_empty());
    assert_eq!(
        found(&db, "renamed artist").await[0],
        vec![SEARCHED_ALBUM.to_string()]
    );

    // The credit prints another of the artist's names, which finds the album.
    exec_searched(
        &db,
        "INSERT INTO artist_credits (artist_id, name_key, name, created_at) VALUES
            ('0373bb67-3abb-48e7-8a5b-6d29ef24308e', 'printed name', 'Printed Näme', '2026-01-01T00:00:00Z');
         UPDATE album_artists SET credit_key = 'printed name' WHERE id = 'cd1bc541-a0b2-4440-8a6b-b9c1ffa33f67';",
    )
    .await;
    assert_eq!(
        found(&db, "printed näme").await[0],
        vec![SEARCHED_ALBUM.to_string()]
    );

    exec_searched(
        &db,
        "DELETE FROM albums WHERE id = 'e307f0f5-e518-4d5e-bea8-83e439780ca0';
         DELETE FROM works WHERE id = 'cef1d7ca-5e1b-4144-9e95-3ece6a68725e';",
    )
    .await;
    let after = found(&db, "title").await;
    assert!(after[0].is_empty() && after[1].is_empty() && after[4].is_empty());
}
