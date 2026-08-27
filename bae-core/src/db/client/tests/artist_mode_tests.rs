use super::super::*;
use super::*;
use coven::SystemClock;

/// Artists covering every membership case: a primary-FK artist that is
/// also a junction artist elsewhere, a junction-only artist, the Various
/// Artists row as a compilation's primary, a work-only composer (no album
/// links), and a fully unlinked artist.
async fn seeded_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
            VALUES
                ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name B', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8', 'Artist Name A', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('f862abf2-3b15-4518-889b-1996d7100201', 'Various Artists', NULL, '194', NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('b96d8066-777d-408d-8ae4-ed58c767e40c', 'Composer Name A', NULL, NULL, 'mb-artist-work-only', 'stamp', '2026-01-01T00:00:00Z'),
                ('7d8362d9-b321-495a-89f7-4cd8998449a4', 'Artist Name C', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES
                ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 2001, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                ('a0231b0b-549b-4e4d-806f-a4b66373e087', 'Album Title B', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 1999, NULL, 0, 'stamp', '2026-01-01T00:00:00Z'),
                ('20022731-6ca1-4bf3-8d27-1e2b2e5e9816', 'Compilation Title A', 'f862abf2-3b15-4518-889b-1996d7100201', 2005, NULL, 1, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES
                ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('64e79a1f-404a-4c34-809a-a3cb44bf1942', 'a0231b0b-549b-4e4d-806f-a4b66373e087', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('77f4af5a-9661-4fb7-845b-73901b0a3ebd', '20022731-6ca1-4bf3-8d27-1e2b2e5e9816', 'file_tags', 1, 'stamp', '2026-01-01T00:00:00Z');

            -- artist-extra joins album-b; artist-primary's junction row on
            -- album-a duplicates its primary FK and must not double-count.
            INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
            VALUES
                ('04ebd233-5eef-4f8c-8ca1-6612601fd136', 'a0231b0b-549b-4e4d-806f-a4b66373e087', '7fa00099-f5d8-4ec2-88bd-e19d8edd7bb8', 1, 'stamp', '2026-01-01T00:00:00Z'),
                ('34392f8d-93a5-47ad-8fe1-f8f5ce013123', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 1, 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO works (id, title, disambiguation, work_type, musicbrainz_work_id, _updated_at, created_at)
            VALUES ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', NULL, 'work', 'mb-work-a', 'stamp', '2026-01-01T00:00:00Z');

            INSERT INTO work_artists (id, work_id, artist_id, position, source, _updated_at, created_at)
            VALUES ('ec41a8cd-a9a4-473e-8b70-d78168aefd8e', '432c8996-8af0-43dc-868a-822a256f65c4', 'b96d8066-777d-408d-8ae4-ed58c767e40c', 0, 'musicbrainz', 'stamp', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}

#[tokio::test]
async fn artist_search_ranks_exact_prefix_and_substring_matches() {
    let (db, _tmp) = seeded_db().await;
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
            VALUES
                ('0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b', 'Artist Search', NULL, 'discogs-exact', 'mb-exact', 'stamp', '2026-01-01T00:00:00Z'),
                ('6b312597-2c63-454e-9341-065704bd5f9f', 'Artist Search Alpha', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('ef0f40c5-83b9-4e14-be41-210438e73ef1', 'Artist Search Beta', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('d5cfb57f-0df4-4c99-9a1a-a634161c2e2c', 'Name With Artist Search Inside', NULL, NULL, NULL, 'stamp', '2026-01-01T00:00:00Z'),
                ('1dd8239d-493b-434a-b0dd-d2838a9b404a', 'Displayed Name', 'Artist Search Sort', NULL, NULL, 'stamp', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let matches = db.search_artists("artist search", 10).await.unwrap();
    let ids = matches
        .iter()
        .map(|artist| artist.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b",
            "6b312597-2c63-454e-9341-065704bd5f9f",
            "ef0f40c5-83b9-4e14-be41-210438e73ef1",
            "1dd8239d-493b-434a-b0dd-d2838a9b404a",
            "d5cfb57f-0df4-4c99-9a1a-a634161c2e2c",
        ]
    );
    assert_eq!(
        matches[0].discogs_artist_id.as_deref(),
        Some("discogs-exact")
    );
    assert_eq!(
        matches[0].musicbrainz_artist_id.as_deref(),
        Some("mb-exact")
    );

    let by_musicbrainz_id = db.search_artists("mb-exact", 10).await.unwrap();
    assert_eq!(by_musicbrainz_id.len(), 1);
    assert_eq!(
        by_musicbrainz_id[0].id,
        "0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b"
    );

    let by_library_id = db
        .search_artists("0d3d77ce-a3ea-4d31-b5ff-e10facb0cc0b", 10)
        .await
        .unwrap();
    assert_eq!(by_library_id.len(), 1);
    assert_eq!(
        by_library_id[0].discogs_artist_id.as_deref(),
        Some("discogs-exact")
    );
}

#[tokio::test]
async fn artist_page_lists_album_artists_with_distinct_album_counts() {
    let (db, _tmp) = seeded_db().await;

    let sort = [ArtistSortCriterion {
        field: ArtistSortField::Name,
        direction: SortDirection::Ascending,
    }];
    let page = db.get_artist_page(&sort, 0, 10).await.unwrap();

    let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
    assert_eq!(ids, vec![ARTIST_EXTRA, ARTIST_PRIMARY, ARTIST_VARIOUS]);

    let counts: Vec<i64> = page.iter().map(|a| a.album_count).collect();
    assert_eq!(counts, vec![1, 2, 1]);

    assert_eq!(db.get_artist_count().await.unwrap(), 3);
}

#[tokio::test]
async fn artist_page_sorts_by_album_count() {
    let (db, _tmp) = seeded_db().await;

    let sort = [ArtistSortCriterion {
        field: ArtistSortField::AlbumCount,
        direction: SortDirection::Descending,
    }];
    let page = db.get_artist_page(&sort, 0, 10).await.unwrap();

    let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();
    assert_eq!(ids, vec![ARTIST_PRIMARY, ARTIST_EXTRA, ARTIST_VARIOUS]);
}

#[tokio::test]
async fn artist_page_uses_id_tiebreaker() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
            VALUES
                ('1b4bafc9-0ece-4538-833e-4ff52feb6ef0', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('38fc314c-c130-4120-8ca9-38b870ccef3a', 'Artist Name Shared', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES
                ('881dc5cf-0686-456a-87c9-98c50e775177', 'Album Title C', '1b4bafc9-0ece-4538-833e-4ff52feb6ef0', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('a0231b0b-549b-4e4d-806f-a4b66373e087', 'Album Title B', '38fc314c-c130-4120-8ca9-38b870ccef3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES
                ('599bc437-136f-4643-87a6-ac30c3fae614', '881dc5cf-0686-456a-87c9-98c50e775177', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('64e79a1f-404a-4c34-809a-a3cb44bf1942', 'a0231b0b-549b-4e4d-806f-a4b66373e087', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let sort = [ArtistSortCriterion {
        field: ArtistSortField::AlbumCount,
        direction: SortDirection::Descending,
    }];
    let mut page_ids = Vec::new();
    for offset in 0..3 {
        let page = db.get_artist_page(&sort, offset, 1).await.unwrap();
        assert_eq!(page.len(), 1);
        page_ids.push(page[0].artist.id.clone());
    }

    // Equal album counts, so the id breaks the tie ascending. The ids are
    // UUIDs, so the expectation is those same three sorted — asserting the
    // rule rather than a hand-written order that depends on which UUID the
    // fixture happened to mint.
    let mut expected = vec![ARTIST_A, ARTIST_B, ARTIST_C];
    expected.sort();
    assert_eq!(page_ids, expected);
}

/// A secondary criterion applies before the name-ASC tail. Two artists tie on
/// `AlbumCount` but differ in name, so `[AlbumCount DESC, Name DESC]` must order
/// the tied pair by name descending — a single-criterion implementation would
/// fall through to the tail's `ar.name ASC` and order them the other way.
#[tokio::test]
async fn artist_page_applies_secondary_criterion() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
            VALUES
                ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('38fc314c-c130-4120-8ca9-38b870ccef3a', 'Artist Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('49549823-0e72-4747-891e-ee50e1611e3a', 'Artist Name Solo', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES
                ('1ac5b125-782f-4dab-8669-417e804d02bb', 'Album Title A1', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('f270285f-ef24-4daa-8058-0dd91571843e', 'Album Title A2', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('2d705181-42c1-47bd-822c-245d8db41d60', 'Album Title B1', '38fc314c-c130-4120-8ca9-38b870ccef3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('191beebd-146b-431f-8a87-ed4be0db20b7', 'Album Title B2', '38fc314c-c130-4120-8ca9-38b870ccef3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('831374ed-abd3-4b6d-84b1-15a9974ecadc', 'Album Title Solo', '49549823-0e72-4747-891e-ee50e1611e3a', 2026, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES
                ('5611ca91-e045-490a-8c14-89c3181a92ab', '1ac5b125-782f-4dab-8669-417e804d02bb', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('48c59983-901e-455e-8cb0-ac0011c08bb4', 'f270285f-ef24-4daa-8058-0dd91571843e', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('7680c516-307b-4a2a-8aea-850f976e006e', '2d705181-42c1-47bd-822c-245d8db41d60', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('51ca3994-c53d-45e4-8ad2-04ac3f4181c8', '191beebd-146b-431f-8a87-ed4be0db20b7', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('7ae8e6fc-98bd-46e2-82c0-4b913087deb1', '831374ed-abd3-4b6d-84b1-15a9974ecadc', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let sort = [
        ArtistSortCriterion {
            field: ArtistSortField::AlbumCount,
            direction: SortDirection::Descending,
        },
        ArtistSortCriterion {
            field: ArtistSortField::Name,
            direction: SortDirection::Descending,
        },
    ];
    let page = db.get_artist_page(&sort, 0, 10).await.unwrap();
    let ids: Vec<&str> = page.iter().map(|a| a.artist.id.as_str()).collect();

    // artist-a and artist-b tie on album_count (2 each); the secondary
    // Name DESC criterion orders artist-b before artist-a.
    assert_eq!(ids, vec![ARTIST_B, ARTIST_A, ARTIST_SOLO]);
}

#[tokio::test]
async fn artist_detail_orders_albums_year_then_title_with_unknown_years_last() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, sort_name, discogs_artist_id, musicbrainz_artist_id, _updated_at, created_at)
            VALUES
                ('d7d8141f-54ff-467d-8b60-4f34a4d2e528', 'Artist Name A', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('4d0b27b7-c953-47f5-8614-70ed973923dc', 'Artist Name B', NULL, NULL, NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, artist_id, year, primary_release_id, is_compilation, _updated_at, created_at)
            VALUES
                ('c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'Album Title Null', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', NULL, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('a663cff7-fad7-45b1-8469-5f77af82ddb8', 'Album Title B', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2001, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('88183677-683b-485e-8224-f6a328c233c7', 'album title a', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 2001, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('88f57246-3e65-4eb9-8d36-ee8d40326cfc', 'Album Title 1999', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 1999, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('7e6f42e7-8952-48e6-89bf-d1bcc611176d', 'Album Title Junction', '4d0b27b7-c953-47f5-8614-70ed973923dc', 2005, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('ebef769a-f2a3-4443-8c01-50921de47fbb', 'Album Title Unrelated', '4d0b27b7-c953-47f5-8614-70ed973923dc', 1990, NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, metadata_source, remote, _updated_at, created_at)
            VALUES
                ('f09739aa-4dde-4cbc-8daf-d77eb2f980ff', 'c6648d5a-617e-4b69-87da-b7f1c4fb5e65', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('f582ee48-93a9-4152-8a12-7a9b62f86c2a', 'a663cff7-fad7-45b1-8469-5f77af82ddb8', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('7361d9e4-6e06-4a57-84fa-c6042cb2fb78', '88183677-683b-485e-8224-f6a328c233c7', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('0014a775-79c9-419f-849e-8de944a1ef04', '88f57246-3e65-4eb9-8d36-ee8d40326cfc', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('d981c4b7-e727-41f9-826e-31ec67066d8c', '7e6f42e7-8952-48e6-89bf-d1bcc611176d', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                ('4fad85ec-94ef-4de6-8cf8-79dc25389051', 'ebef769a-f2a3-4443-8c01-50921de47fbb', 'file_tags', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');

            INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at)
            VALUES ('1725fb34-3b73-477f-8113-e995104feae3', '7e6f42e7-8952-48e6-89bf-d1bcc611176d', 'd7d8141f-54ff-467d-8b60-4f34a4d2e528', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let detail = db.find_artist_detail(ARTIST_A).await.unwrap();
    let detail = detail.expect("artist-a has album links and must resolve");

    assert_eq!(detail.artist.artist.id, ARTIST_A);
    assert_eq!(detail.artist.album_count, 5);

    let album_ids: Vec<&str> = detail.albums.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        album_ids,
        vec![
            ALBUM_1999,
            ALBUM_2001_LOWER,
            ALBUM_2001_UPPER,
            ALBUM_JUNCTION,
            ALBUM_NULL,
        ]
    );
}

#[tokio::test]
async fn artist_detail_absent_or_album_less_artist_is_none() {
    let (db, _tmp) = seeded_db().await;

    assert!(db
        .find_artist_detail(ARTIST_ABSENT)
        .await
        .unwrap()
        .is_none());
    assert!(db
        .find_artist_detail(ARTIST_WORK_ONLY)
        .await
        .unwrap()
        .is_none());
}
