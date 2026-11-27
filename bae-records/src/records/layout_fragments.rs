//! Where a release's tracks sit on its mediums once moves are made, as
//! fragments: the mediums the layout starts from, and the rule that lays the
//! tracks out on them. A release editor's draft and save, and an import
//! draft, spread the same rule, so they move a sheet alike.

// The mediums the JSON array `mediums` gives, each a `{position, carrier,
// sides}`.
sqlive_compose::fragment!(
    GivenMediums(mediums: String) {
        position: Option<i64>,
        carrier: Option<String>,
        sides: Option<i64>
    },
    r#"
    SELECT given.value ->> 'position' AS position, given.value ->> 'carrier' AS carrier,
           given.value ->> 'sides' AS sides
    FROM json_each(:mediums) AS given
"#
);

// The mediums a release editor's layout starts from: an applied catalog's
// (the JSON array `mediums`, [`GivenMediums`]) where `applied`, else the
// release's own. A layout neither adds nor drops one, so a medium no track
// sits on stays at its position.
sqlive_compose::fragment!(
    BaseMediums(release: String, applied: bool, mediums: String) {
        position: Option<i64>,
        carrier: Option<String>,
        sides: Option<i64>
    },
    r#"
    WITH given_mediums AS (
        SELECT listed.*
        FROM @crate::records::layout_fragments::GivenMediums(mediums: :mediums) AS listed)
    SELECT position, carrier, sides
    FROM given_mediums
    WHERE :applied
    UNION ALL
    SELECT held.position, held.medium, held.side_count
    FROM release_mediums AS held
    WHERE held.release_id = :release AND NOT :applied
"#
);

// The tracks laid out on `base_mediums`, in playing order.
//
// The layout starts from the places `base_places`, in playing order. A sheet
// that `picks` puts onto a side a medium has, with a track not on it, moves
// all its tracks there (`carving` names each sheet's tracks), after the
// tracks already on that side, each numbered one more than the highest number
// on that side, so no two tracks of a side share a number; sheets going to one
// side keep the order of their first tracks. The tracks then take their
// playing order from their places. A layout that moves no sheet lays the
// tracks out as they were.
//
// Where the edits delete a track (`dropped`), the layout lays out what is
// left: the deleted tracks go, each added track (`added`, the files a save
// adds a track for, with the sheet whose tracks each replaces) takes the
// place of its anchor, the first track of that sheet, ahead of the track that
// holds it, and every side numbers its tracks from one in playing order. An
// added track is named by its file's id.
sqlive_compose::fragment!(
    LaidPlaces
    with base_mediums(position INTEGER, carrier TEXT, sides INTEGER),
         base_places(
             track_id TEXT, medium INTEGER, side INTEGER, number INTEGER,
             stated_length_ms INTEGER, position INTEGER
         ),
         picks(sheet_file_id TEXT, medium INTEGER, side INTEGER),
         carving(sheet_file_id TEXT, track_id TEXT),
         dropped(track_id TEXT),
         added(file_id TEXT, sheet_file_id TEXT, first_position INTEGER)
    {
        track_id: String,
        medium: i64,
        side: i64,
        number: i64,
        stated_length_ms: Option<i64>,
        position: i64
    },
    r#"
    WITH sheet_tracks (sheet_file_id, track_id, medium, side, position, first_position) AS (
        SELECT cut.sheet_file_id, cut.track_id, p.medium, p.side, p.position,
               MIN(p.position) OVER (PARTITION BY cut.sheet_file_id)
        FROM carving AS cut JOIN base_places AS p ON p.track_id = cut.track_id),
    picked (track_id, medium, side, first_position, away) AS (
        SELECT st.track_id, pick.medium, pick.side, st.first_position,
               MAX(st.medium <> pick.medium OR st.side <> pick.side)
                   OVER (PARTITION BY pick.sheet_file_id)
        FROM picks AS pick
        JOIN sheet_tracks AS st ON st.sheet_file_id = pick.sheet_file_id
        WHERE EXISTS (SELECT 1 FROM base_mediums AS target
                      WHERE target.position = pick.medium
                        AND pick.side BETWEEN 1 AND target.sides)),
    moves (track_id, medium, side, first_position) AS (
        SELECT track_id, medium, side, first_position FROM picked WHERE away),
    targets (track_id, medium, side, number, stated_length_ms, position, moved,
             first_position) AS (
        SELECT p.track_id, COALESCE(mv.medium, p.medium), COALESCE(mv.side, p.side),
               p.number, p.stated_length_ms, p.position, mv.track_id IS NOT NULL,
               mv.first_position
        FROM base_places AS p LEFT JOIN moves AS mv ON p.track_id = mv.track_id),
    kept (track_id, medium, side, number, stated_length_ms, position, moves, changing) AS (
        SELECT track_id, medium, side,
               CASE WHEN moved
                    THEN COALESCE(MAX(number) FILTER (WHERE NOT moved) OVER on_side, 0)
                         + ROW_NUMBER() OVER (PARTITION BY medium, side, moved
                                              ORDER BY first_position, position)
                    ELSE number END,
               stated_length_ms, position, MAX(moved) OVER (),
               EXISTS (SELECT 1 FROM dropped)
        FROM targets WINDOW on_side AS (PARTITION BY medium, side)),
    anchored (file_id, anchor, ord) AS (
        SELECT replacing.file_id, st.track_id, replacing.first_position
        FROM added AS replacing
        JOIN sheet_tracks AS st
          ON st.sheet_file_id = replacing.sheet_file_id AND st.position = st.first_position),
    numbered (track_id, medium, side, number, stated_length_ms, position, moving) AS (
        SELECT CASE WHEN pass.fresh THEN a.file_id ELSE k.track_id END, k.medium, k.side,
               CASE WHEN k.changing
                    THEN ROW_NUMBER() OVER (PARTITION BY k.medium, k.side
                                            ORDER BY k.number, pass.fresh DESC, a.ord,
                                                     k.position)
                    ELSE k.number END,
               CASE WHEN pass.fresh THEN NULL ELSE k.stated_length_ms END, k.position,
               k.moves OR k.changing
        FROM kept AS k
        JOIN (SELECT 0 AS fresh UNION ALL SELECT 1) AS pass
        LEFT JOIN anchored AS a ON pass.fresh AND a.anchor = k.track_id
        WHERE (NOT pass.fresh AND k.track_id NOT IN (SELECT track_id FROM dropped))
           OR (pass.fresh AND a.file_id IS NOT NULL))
    SELECT n.track_id, n.medium, n.side, n.number, n.stated_length_ms,
           CASE WHEN n.moving
                THEN ROW_NUMBER() OVER (ORDER BY n.medium, n.side, n.number) - 1
                ELSE n.position END AS position
    FROM numbered AS n
"#
);

// A release editor's layout of the release's tracks ([`LaidPlaces`]): on
// [`BaseMediums`], from the places an applied catalog gives them (the JSON
// array `places`, each a `{track_id, medium, side, number, stated_length_ms,
// position}`) where `applied`, else the library's, with the sheets the JSON
// array `moves` puts onto sides, each a `{sheet_file_id, medium, side}`, and
// without and with the tracks the edits `edits` delete and add.
sqlive_compose::fragment!(
    ReleaseLaidPlaces(
        release: String,
        applied: bool,
        mediums: String,
        places: String,
        moves: String,
        edits: String,
    ),
    r#"
    WITH starting AS (
        SELECT base.*
        FROM @crate::records::layout_fragments::BaseMediums(
            release: :release, applied: :applied, mediums: :mediums) AS base),
    gone AS (
        SELECT deleted.*
        FROM @crate::records::sheet_fragments::DroppedSheetTracks(
            release: :release, edits: :edits) AS deleted),
    replacing AS (
        SELECT fresh.*
        FROM @crate::records::sheet_fragments::SheetAddedFiles(
            release: :release, edits: :edits) AS fresh)
    SELECT laid.*
    FROM @crate::records::layout_fragments::LaidPlaces(
        base_mediums: (SELECT starting.position, starting.carrier, starting.sides
                       FROM starting),
        base_places: (
            SELECT given.value ->> 'track_id', given.value ->> 'medium',
                   given.value ->> 'side', given.value ->> 'number',
                   given.value ->> 'stated_length_ms', given.value ->> 'position'
            FROM json_each(:places) AS given WHERE :applied
            UNION ALL
            SELECT held.id, medium.position, held.side, held.track_number,
                   held.stated_length_ms, held.position
            FROM tracks AS held
            JOIN release_mediums AS medium ON medium.id = held.medium_id
            WHERE held.release_id = :release AND NOT :applied),
        picks: (SELECT picked.value ->> 'sheet_file_id', picked.value ->> 'medium',
                       picked.value ->> 'side'
                FROM json_each(:moves) AS picked),
        carving: (SELECT cut.sheet_file_id, cut.track_id
                  FROM tracks AS held
                  JOIN carved_tracks AS cut ON cut.track_id = held.id
                  WHERE held.release_id = :release),
        dropped: (SELECT gone.track_id FROM gone),
        added: (SELECT replacing.file_id, replacing.sheet_file_id, replacing.first_position
                FROM replacing)) AS laid
"#
);
