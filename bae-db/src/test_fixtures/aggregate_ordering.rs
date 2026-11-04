//! Libraries seeded for the reads that aggregate and order a release's
//! credits, tracks and files.

use super::RELEASE_A;
use crate::{Database, DbError, NewReleaseFile};

pub async fn aggregate_db() -> (Database, tempfile::TempDir) {
    let (db, tmp) = super::temp_db(super::TEST_SCHEMA).await;
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('7cdf9a34-0746-472b-8c68-0a669c11f2f1', 'Artist Name Primary', 'artist name primary', '2026-01-01T00:00:00Z'),
             ('7e7d8df5-8292-4287-80be-7abd24f5a992', 'Artist Name First', 'artist name first', '2026-01-01T00:00:00Z'),
             ('863911c6-a6b6-40a7-8096-b85eb877f7c7', 'Artist Name Second', 'artist name second', '2026-01-01T00:00:00Z'),
             ('c0770501-2551-4e87-8801-93f780248cf3', 'Composer Name First', 'composer name first', '2026-01-01T00:00:00Z'),
             ('0d0e8916-becd-4d2c-89e0-7cc5c7005f83', 'Composer Name Second', 'composer name second', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', 2026, 0, '2026-01-01T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('2bd047a8-0ed8-4f71-851d-e168c16cbd36', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '863911c6-a6b6-40a7-8096-b85eb877f7c7', 2, '2026-01-01T00:00:00Z'),
             ('087bc9e6-f2ef-422c-ba5d-1f78f3ed333c', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '7cdf9a34-0746-472b-8c68-0a669c11f2f1', 0, '2026-01-01T00:00:00Z'),
             ('1c2ab709-4221-4c38-8d0c-ff1d18107cce', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '7e7d8df5-8292-4287-80be-7abd24f5a992', 1, '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('8aa66d48-65a0-42e4-8c1d-e7481e8c1861', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '2026-01-01T00:00:00Z', 0),
             ('64e79a1f-404a-4c34-809a-a3cb44bf1942', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '2026-01-01T00:00:01Z', 1),
             ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '2026-01-01T00:00:01Z', 1);

            INSERT INTO works (id, title, work_type, musicbrainz_work_id, created_at) VALUES ('432c8996-8af0-43dc-868a-822a256f65c4', 'Work Title A', 'work', 'mb-work-a', '2026-01-01T00:00:00Z');

            INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('ba0d989a-7cb6-4050-8247-9a5424b33041', '432c8996-8af0-43dc-868a-822a256f65c4', '0d0e8916-becd-4d2c-89e0-7cc5c7005f83', 1, 'file_tags', '2026-01-01T00:00:00Z'),
             ('6027384b-545d-4289-8123-7201ec25276f', '432c8996-8af0-43dc-868a-822a256f65c4', 'c0770501-2551-4e87-8801-93f780248cf3', 0, 'file_tags', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}

pub async fn queue_db() -> (Database, tempfile::TempDir) {
    let (db, tmp) = super::temp_db(super::TEST_SCHEMA).await;
    db.call(|conn| {
        super::seed_artist_album_release(conn)?;
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('5b5f8c38-5237-4187-895c-28b1b2a43672', 'Track Artist Name First', 'track artist name first', '2026-01-01T00:00:00Z'),
             ('8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 'Track Artist Name Second', 'track artist name second', '2026-01-01T00:00:00Z');

            INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES ('c9f6f13c-15b2-5039-aad4-8cdd8b6c67f8', '0252dedb-ee39-4547-8803-438dbeb57a64', 1, 'cd', 1, '2026-01-01T00:00:00Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, '2026-01-01T00:00:00Z', 'c9f6f13c-15b2-5039-aad4-8cdd8b6c67f8');

            INSERT INTO track_artists (id, track_id, artist_id, position, created_at) VALUES ('af940e5f-472b-4162-81fb-97517afd23be', '0482872e-d4bf-4080-8426-441a0a3e71fc', '8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 1, '2026-01-01T00:00:00Z'),
             ('8b08019f-1c04-400e-8107-9b85f7222407', '0482872e-d4bf-4080-8426-441a0a3e71fc', '5b5f8c38-5237-4187-895c-28b1b2a43672', 0, '2026-01-01T00:00:00Z');
            ",
        )?;
        Ok(())
    })
    .await
    .unwrap();
    (db, tmp)
}

pub async fn release_detail_db() -> (Database, tempfile::TempDir) {
    let (db, tmp) = super::temp_db(super::TEST_SCHEMA).await;
    db.call(|conn| {
        super::seed_artist_album_release(conn)?;
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('5b5f8c38-5237-4187-895c-28b1b2a43672', 'Track Artist Name First', 'track artist name first', '2026-01-01T00:00:00Z'),
             ('8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 'Track Artist Name Second', 'track artist name second', '2026-01-01T00:00:00Z');

            INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES ('c9f6f13c-15b2-5039-aad4-8cdd8b6c67f8', '0252dedb-ee39-4547-8803-438dbeb57a64', 1, 'cd', 1, '2026-01-01T00:00:00Z');

            INSERT INTO tracks (position, id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES (1, '04676261-1659-47b1-879c-2947c52f4a8d', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title B', 1, 2, 1000, '2026-01-01T00:00:00Z', 'c9f6f13c-15b2-5039-aad4-8cdd8b6c67f8'),
             (0, '0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, '2026-01-01T00:00:00Z', 'c9f6f13c-15b2-5039-aad4-8cdd8b6c67f8');

            INSERT INTO track_artists (id, track_id, artist_id, position, created_at) VALUES ('af940e5f-472b-4162-81fb-97517afd23be', '0482872e-d4bf-4080-8426-441a0a3e71fc', '8ccac2a7-7e60-4f52-881e-0b349ff78cc5', 1, '2026-01-01T00:00:00Z'),
             ('8b08019f-1c04-400e-8107-9b85f7222407', '0482872e-d4bf-4080-8426-441a0a3e71fc', '5b5f8c38-5237-4187-895c-28b1b2a43672', 0, '2026-01-01T00:00:00Z');
            ",
        )?;
        Ok(())
    })
    .await
    .unwrap();
    (db, tmp)
}

/// Seed a release file, staged on this device only from an original of
/// `size` zero bytes written under `dir`.
pub async fn insert_file(db: &Database, dir: &std::path::Path, file: NewReleaseFile, size: u64) {
    let original = dir.join(&file.id);
    std::fs::File::create_new(&original).unwrap().set_len(size).unwrap();
    db.insert_file_for_test(&file, &original).await.unwrap();
}

/// The names `insert_files_out_of_natural_order` seeds, in natural order,
/// case aside.
pub const FILES_IN_NATURAL_ORDER: [&str; 5] =
    ["album.cue", "Back.jpg", "cover.jpg", "Track 2.flac", "Track 10.flac"];

/// Seed `RELEASE_A` with the files of `FILES_IN_NATURAL_ORDER`, inserted out
/// of order, mixed case, with a two-digit number.
pub async fn insert_files_out_of_natural_order(db: &Database, dir: &std::path::Path) {
    for (i, name) in
        ["Track 10.flac", "cover.jpg", "Track 2.flac", "Back.jpg", "album.cue"].iter().enumerate()
    {
        let mime = if name.ends_with(".flac") {
            "audio/flac"
        } else if name.ends_with(".cue") {
            "application/x-cue"
        } else {
            "image/jpeg"
        };
        insert_file(
            db,
            dir,
            NewReleaseFile {
                id: bae_util::test_ids::test_uuid(&format!("ordering-file-{i}")),
                release_id: RELEASE_A.to_string(),
                original_filename: name.to_string(),
                content_type: bae_util::content_type::ContentType::from_mime(mime),
                source_audio: None,
                created_at: chrono::Utc::now(),
            },
            1,
        )
        .await;
    }
}
