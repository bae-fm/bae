//! A library holding one composed work: an album artist's album on one
//! release with one track, which performs a work part credited to its
//! composer, and the work it is part of.

use bae_store::DbError;

pub const WORK_CHILD_A: &str = "f63d8e66-6a81-4a67-8005-1fbe870f27eb";
pub const WORK_PARENT_A: &str = "6b05af7a-ee0c-4f12-8938-1d5536697271";

/// An album artist's album on one release with one track, which performs a
/// work part whose composer is credited on it, and the work it is part of, on
/// `schema`.
pub async fn composed_work_db(schema: bae_store::Schema) -> (bae_store::Store, tempfile::TempDir) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, sort_name, discogs_artist_id, musicbrainz_artist_id, created_at) VALUES ('85f70840-aba5-4eb9-8e1a-0d319e53b798', 'Album Artist A', 'album artist a', NULL, NULL, NULL, '2026-01-01T00:00:00Z'),
             ('5412b7ad-bdc1-4561-8985-b6d6ef8a2880', 'Displayed Composer A', 'displayed composer a', 'Hidden Composer Sort A', NULL, 'mb-artist-composer-a', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES ('a67c03ad-425f-45e9-8279-0144c852aaa5', 'Album Title A', 2026, 0, '2026-01-01T00:00:00Z');
            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('0f3dddda-6465-4f39-acfc-de0bed691179', 'a67c03ad-425f-45e9-8279-0144c852aaa5', '85f70840-aba5-4eb9-8e1a-0d319e53b798', 0, '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, release_name, year, discogs_details, source_folder_name, album_loudness_lufs, album_peak_linear, created_at, album_order) VALUES ('0252dedb-ee39-4547-8803-438dbeb57a64', 'a67c03ad-425f-45e9-8279-0144c852aaa5', NULL, 2026, '[]', NULL, NULL, NULL, '2026-01-01T00:00:00Z', 0);
            INSERT INTO release_media (id, release_id, position, medium, count) VALUES ('2fc6dccf-ed9d-58ae-a838-9527750f14f0', '0252dedb-ee39-4547-8803-438dbeb57a64', 0, 'cd', 1);

            INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES ('c9f6f13c-15b2-5039-aad4-8cdd8b6c67f8', '0252dedb-ee39-4547-8803-438dbeb57a64', 1, 'cd', 1, '2026-01-01T00:00:00Z');

            INSERT INTO tracks (id, release_id, title, side, track_number, duration_ms, created_at, medium_id) VALUES ('0482872e-d4bf-4080-8426-441a0a3e71fc', '0252dedb-ee39-4547-8803-438dbeb57a64', 'Track Title A', 1, 1, 1000, '2026-01-01T00:00:00Z', 'c9f6f13c-15b2-5039-aad4-8cdd8b6c67f8');

            INSERT INTO works (id, title, disambiguation, work_type, musicbrainz_work_id, created_at) VALUES ('6b05af7a-ee0c-4f12-8938-1d5536697271', 'Parent Work A', NULL, 'work', 'mb-work-parent-a', '2026-01-01T00:00:00Z'),
             ('f63d8e66-6a81-4a67-8005-1fbe870f27eb', 'Displayed Work A', NULL, 'part', 'mb-work-child-a', '2026-01-01T00:00:00Z');

            INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) VALUES ('ec41a8cd-a9a4-473e-8b70-d78168aefd8e', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', '5412b7ad-bdc1-4561-8985-b6d6ef8a2880', 0, 'musicbrainz', '2026-01-01T00:00:00Z');

            INSERT INTO work_parts (id, parent_work_id, child_work_id, position, source, created_at) VALUES ('fa383452-6671-4335-87bc-751a52bbdde5', '6b05af7a-ee0c-4f12-8938-1d5536697271', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', 0, 'musicbrainz', '2026-01-01T00:00:00Z');

            INSERT INTO track_works (id, track_id, work_id, position, source, created_at) VALUES ('d410a973-6a19-4ad3-87d8-b0c8c13d6015', '0482872e-d4bf-4080-8426-441a0a3e71fc', 'f63d8e66-6a81-4a67-8005-1fbe870f27eb', 0, 'musicbrainz', '2026-01-01T00:00:00Z');
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    (db, tmp)
}
