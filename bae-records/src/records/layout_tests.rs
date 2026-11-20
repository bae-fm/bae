use super::*;
use rusqlite::Connection;

const RELEASE: &str = "release";

/// A release on the real schema with `mediums` (each its side count) and the
/// tracks `(id, medium, side, number)` in playing order, and CUE sheets that
/// carve the tracks `(sheet, track id)`.
fn release(mediums: &[u8], tracks: &[(&str, u16, u8, u32)], carved: &[(&str, &str)]) -> Connection {
    let conn = Connection::open_in_memory().expect("an in-memory database opens");
    crate::test_fixtures::TEST_SCHEMA.open_bare_library(&conn);
    let now = "2026-01-01T00:00:00Z";
    conn.execute("INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('album', 'Album', 0, ?)", [now]).unwrap();
    conn.execute(
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES (?, 'album', ?, 0)",
        params![RELEASE, now],
    )
    .unwrap();
    for (index, sides) in mediums.iter().enumerate() {
        conn.execute(
            "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) VALUES (?, ?, ?, NULL, ?, ?)",
            params![format!("medium-{}", index + 1), RELEASE, index + 1, sides, now],
        )
        .unwrap();
    }
    for (position, (id, medium, side, number)) in tracks.iter().enumerate() {
        conn.execute(
            "INSERT INTO tracks (id, release_id, title, medium_id, side, track_number, duration_ms, created_at, position) VALUES (?, ?, ?, ?, ?, ?, 1000, ?, ?)",
            params![id, RELEASE, id, format!("medium-{medium}"), side, number, now, position],
        )
        .unwrap();
    }
    let mut sheets: Vec<&str> = carved.iter().map(|(sheet, _)| *sheet).collect();
    sheets.dedup();
    for sheet in sheets {
        conn.execute(
            "INSERT INTO release_files (id, release_id, original_filename, content_type, created_at, file, role, document_kind) VALUES (?, ?, ?, 'text/plain', ?, ?, 'track_sheet', NULL)",
            params![sheet, RELEASE, format!("{sheet}.cue"), now, coven::IdSource::new_id(&coven::UuidIds).to_string()],
        )
        .ok();
        conn.execute(
            "INSERT INTO release_carving_sheets (file_id, disc_number) VALUES (?, 1)",
            [sheet],
        )
        .ok();
    }
    for (sheet, track) in carved {
        conn.execute(
            "INSERT INTO carved_tracks (track_id, sheet_file_id) VALUES (?, ?)",
            params![track, sheet],
        )
        .ok();
    }
    conn
}

/// The tracks as the layout places them for the discs picked: each with its
/// medium, side and number, in playing order, and how many mediums remain.
fn laid(conn: &Connection, picks: &[(&str, u32)]) -> (Vec<(String, u16, u8, u32)>, usize) {
    let moves = serde_json::to_string(
        &picks
            .iter()
            .map(|(sheet, disc)| serde_json::json!({ "sheet_file_id": sheet, "disc": disc }))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let layout = LayoutParams {
        release_id: RELEASE,
        applied: false,
        mediums: "[]",
        places: "[]",
        moves: &moves,
    };
    let places = conn
        .prepare(&format!(
            "WITH {} SELECT track_id, medium, side, number FROM laid_places ORDER BY position",
            layout_ctes()
        ))
        .unwrap()
        .query_map(layout.named().as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mediums: usize = conn
        .query_row(
            &format!("WITH {} SELECT COUNT(*) FROM laid_mediums", layout_ctes()),
            layout.named().as_slice(),
            |row| row.get(0),
        )
        .unwrap();
    (places, mediums)
}

fn place(id: &str, medium: u16, side: u8, number: u32) -> (String, u16, u8, u32) {
    (id.to_string(), medium, side, number)
}

/// A sheet's tracks go after the tracks of the disc it moves to, numbered on
/// from them, and the disc it left is gone.
#[test]
fn a_sheet_moves_after_the_tracks_of_its_new_disc() {
    let conn = release(
        &[1, 1],
        &[("a1", 1, 1, 1), ("a2", 1, 1, 2), ("b1", 2, 1, 1), ("b2", 2, 1, 2)],
        &[("sheet-a", "a1"), ("sheet-a", "a2")],
    );
    let (places, mediums) = laid(&conn, &[("sheet-a", 2)]);
    assert_eq!(
        places,
        [place("b1", 1, 1, 1), place("b2", 1, 1, 2), place("a1", 1, 1, 3), place("a2", 1, 1, 4)]
    );
    assert_eq!(mediums, 1);
}

/// A number the disc's tracks leave a gap in is not taken again: the moved
/// tracks number from one more than the highest.
#[test]
fn moved_tracks_number_past_the_highest_number_of_the_side() {
    let conn = release(
        &[1, 1],
        &[("a1", 1, 1, 1), ("b1", 2, 1, 1), ("b2", 2, 1, 5)],
        &[("sheet-a", "a1")],
    );
    let (places, _) = laid(&conn, &[("sheet-a", 2)]);
    assert_eq!(places, [place("b1", 1, 1, 1), place("b2", 1, 1, 5), place("a1", 1, 1, 6)]);
}

/// A medium between others that loses its tracks goes, and the mediums
/// after it move up.
#[test]
fn the_mediums_after_an_emptied_one_move_up() {
    let conn =
        release(&[1, 1, 1], &[("a", 1, 1, 1), ("b", 2, 1, 1), ("c", 3, 1, 1)], &[("sheet-b", "b")]);
    let (places, mediums) = laid(&conn, &[("sheet-b", 1)]);
    assert_eq!(places, [place("a", 1, 1, 1), place("b", 1, 1, 2), place("c", 2, 1, 1)]);
    assert_eq!(mediums, 2);
}

/// A pick of a disc the release lacks, and of the disc a sheet is on
/// already, change nothing, not even the order of the tracks.
#[test]
fn a_move_that_goes_nowhere_leaves_the_layout_alone() {
    let conn = release(&[1, 1], &[("b1", 2, 1, 1), ("a1", 1, 1, 1)], &[("sheet-a", "a1")]);
    let stored = laid(&conn, &[]);
    for disc in [0, 1, 3, 70_000] {
        assert_eq!(laid(&conn, &[("sheet-a", disc)]), stored, "a pick of disc {disc}");
    }
}

/// A medium played side by side takes the moved tracks on its last side.
#[test]
fn a_sided_medium_takes_the_tracks_on_its_last_side() {
    let conn =
        release(&[1, 2], &[("a", 1, 1, 1), ("s1", 2, 1, 1), ("s2", 2, 2, 1)], &[("sheet-a", "a")]);
    let (places, _) = laid(&conn, &[("sheet-a", 2)]);
    assert_eq!(places, [place("s1", 1, 1, 1), place("s2", 1, 2, 1), place("a", 1, 2, 2)]);
}

/// Two sheets going to one disc keep the order of their first tracks.
#[test]
fn two_sheets_on_one_disc_keep_their_order() {
    let conn = release(
        &[1, 1, 1],
        &[("a", 1, 1, 1), ("b", 2, 1, 1), ("c", 3, 1, 1)],
        &[("sheet-a", "a"), ("sheet-b", "b")],
    );
    let (places, mediums) = laid(&conn, &[("sheet-b", 3), ("sheet-a", 3)]);
    assert_eq!(places, [place("c", 1, 1, 1), place("a", 1, 1, 2), place("b", 1, 1, 3)]);
    assert_eq!(mediums, 1);
}
