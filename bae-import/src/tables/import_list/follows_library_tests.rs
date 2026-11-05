//! What this device imported places: a release on Done, dated by when its
//! release was written, and what its Done row shows of its library album:
//! the album's title, its credits and the names they print, each moved by
//! the write that changes it in the library.

use crate::tables::import_list::placement::tests::*;
use crate::tables::import_list::tests::*;
use bae_records::test_fixtures::{ALBUM_A, ARTIST_A, ARTIST_B, RELEASE_A};

/// An imported release's Done row, showing `title` and `artists`.
fn done(title: &str, artists: &str) -> Placed {
    found("Album").on("done").dated(1_767_225_600_000).shown(title, Some(artists))
}

fn uuid(name: &str) -> String {
    bae_util::test_ids::test_uuid(name)
}

/// What this device imported from a folder's files puts it on Done, dated by
/// when its release was written and showing its library album, whatever else
/// is stored for it.
#[tokio::test]
async fn an_imported_release_is_done_dated_by_its_release() {
    let lib = Library::new().await;
    lib.run(&format!(
        "{}{}{}
         INSERT INTO skipped_import_candidates (candidate_path) VALUES ({});
         INSERT INTO albums (id, title, is_compilation, created_at)
         VALUES ('{ALBUM_A}', 'Album Title', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order)
         VALUES ('{RELEASE_A}', '{ALBUM_A}', '2026-01-01T00:00:00.123456789+00:00', 0);",
        lib.scan("Album"),
        lib.discovered("Album", 100),
        lib.state("Album"),
        lib.sql("Album")
    ))
    .await;
    let imported_at = 1_767_225_600_000;
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO local_release_origin (release_id, content_hash)
             VALUES ('{RELEASE_A}', '{}');",
            hash("Album")
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(found("Album").on("done").dated(imported_at + 123).shown("Album Title", Some("")))
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE releases SET created_at = '2026-01-01T02:00:01+02:00' WHERE id = '{RELEASE_A}';"
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(
                found("Album").on("done").dated(imported_at + 1_000).shown("Album Title", Some(""))
            )
        )])
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE releases SET created_at = '2026-01-01T00:00:02Z' WHERE id = '{RELEASE_A}';"
        ))
        .await,
        moves([(
            lib.release("Album"),
            Some(
                found("Album").on("done").dated(imported_at + 2_000).shown("Album Title", Some(""))
            )
        )])
    );
    assert_eq!(
        lib.moved(&format!("DELETE FROM local_release_origin WHERE release_id = '{RELEASE_A}';"))
            .await,
        moves([(lib.release("Album"), Some(found("Album").on("skipped").dated(100)))])
    );
}

/// The Done row follows its library album: retitling it, its credits coming,
/// going and changing, the artists they name being renamed or merged, the
/// credit names they print, and the release moving to another album. A
/// release nobody imported, credited to the same artists in its draft, moves
/// only where its draft names them.
#[tokio::test]
async fn a_done_row_follows_its_library_album() {
    let lib = Library::new().await;
    let other_album = uuid("other album");
    let survivor = uuid("survivor artist");
    lib.run(&format!(
        "{}{}{}{}{}
         INSERT INTO artists (id, name, name_key, created_at) VALUES
             ('{ARTIST_A}', 'Artist A', 'artist a', '2026-01-01T00:00:00Z'),
             ('{ARTIST_B}', 'Artist B', 'artist b', '2026-01-01T00:00:00Z'),
             ('{survivor}', 'Survivor', 'survivor', '2026-01-01T00:00:00Z');
         INSERT INTO albums (id, title, is_compilation, created_at) VALUES
             ('{ALBUM_A}', 'Album Title', 0, '2026-01-01T00:00:00Z'),
             ('{other_album}', 'Other Album', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order)
         VALUES ('{RELEASE_A}', '{ALBUM_A}', '2026-01-01T00:00:00Z', 0);
         INSERT INTO local_release_origin (release_id, content_hash)
         VALUES ('{RELEASE_A}', '{album_hash}');",
        lib.scan("Album"),
        lib.state("Album"),
        lib.draft("Album", "Draft Title"),
        lib.scan("Other"),
        lib.state("Other"),
        album_hash = hash("Album"),
    ))
    .await;
    assert_eq!(
        lib.placed().await.get(&lib.release("Album")),
        Some(&done("Album Title", "")),
        "the library album's title, not the draft's"
    );
    let moved_to =
        |title: &str, artists: &str| moves([(lib.release("Album"), Some(done(title, artists)))]);
    assert_eq!(
        lib.moved(&format!("UPDATE albums SET title = 'Retitled' WHERE id = '{ALBUM_A}';")).await,
        moved_to("Retitled", "")
    );
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO album_artists (id, album_id, artist_id, position, join_phrase, created_at)
             VALUES ('{}', '{ALBUM_A}', '{ARTIST_A}', 0, ' & ', '2026-01-01T00:00:00Z');",
            uuid("credit a")
        ))
        .await,
        moved_to("Retitled", "Artist A")
    );
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO album_artists (id, album_id, artist_id, position, created_at)
             VALUES ('{}', '{ALBUM_A}', '{ARTIST_B}', 1, '2026-01-01T00:00:00Z');",
            uuid("credit b")
        ))
        .await,
        moved_to("Retitled", "Artist A & Artist B")
    );
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO artist_credits (artist_id, name_key, name, created_at)
             VALUES ('{ARTIST_B}', 'printed b', 'Printed B', '2026-01-01T00:00:00Z');"
        ))
        .await,
        moves([]),
        "a credit name no credit prints shows nowhere"
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE album_artists SET credit_key = 'printed b' WHERE id = '{}';",
            uuid("credit b")
        ))
        .await,
        moved_to("Retitled", "Artist A & Printed B")
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE artist_credits SET name = 'Printed Bee' WHERE artist_id = '{ARTIST_B}';"
        ))
        .await,
        moved_to("Retitled", "Artist A & Printed Bee")
    );
    assert_eq!(
        lib.moved(&format!("UPDATE artists SET name = 'Renamed A' WHERE id = '{ARTIST_A}';")).await,
        moved_to("Retitled", "Renamed A & Printed Bee")
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE album_artists SET join_phrase = ' with ' WHERE id = '{}';",
            uuid("credit a")
        ))
        .await,
        moved_to("Retitled", "Renamed A with Printed Bee")
    );
    assert_eq!(
        lib.moved(&format!(
            "INSERT INTO artist_merges (id, into_artist_id, created_at)
             VALUES ('{ARTIST_A}', '{survivor}', '2026-01-01T00:00:00Z');
             INSERT INTO artist_merge_refresh (artist_id, into_artist_id)
             VALUES ('{ARTIST_A}', '{survivor}');"
        ))
        .await,
        moved_to("Retitled", "Survivor with Printed Bee"),
        "a merged artist prints as the artist it shows as"
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE artists SET name = 'Renamed Survivor' WHERE id = '{survivor}';"
        ))
        .await,
        moved_to("Retitled", "Renamed Survivor with Printed Bee")
    );
    assert_eq!(
        lib.moved(&format!("DELETE FROM album_artists WHERE id = '{}';", uuid("credit b"))).await,
        moved_to("Retitled", "Renamed Survivor")
    );
    assert_eq!(
        lib.moved(&format!(
            "UPDATE releases SET album_id = '{other_album}' WHERE id = '{RELEASE_A}';"
        ))
        .await,
        moved_to("Other Album", "")
    );
}
