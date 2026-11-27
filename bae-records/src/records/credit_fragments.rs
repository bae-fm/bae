//! The credits records' reads spread: a track's, the composers of a work, and
//! the credit lines a release's detail lists.

// The track's own credits as a JSON array in list order, each the artist it
// shows after merges, the other printed name it names and its join; no row for
// a track with none.
sqlive_compose::fragment!(
    TrackOwnCredits on tracks { credits: String },
    r#"
    SELECT json_group_array(json_object(
               'id', credited.id, 'name', credited.name, 'sort_name', credited.sort_name,
               'discogs_artist_id', credited.discogs_artist_id,
               'musicbrainz_artist_id', credited.musicbrainz_artist_id,
               'created_at', credited.created_at,
               'credit_key', credit_name.name_key, 'credit_name', credit_name.name,
               'join', credit.join_phrase)
               ORDER BY credit.position) AS credits
    FROM track_artists AS credit
    JOIN artists AS credited
      ON credited.id = (SELECT shown_artist_id FROM artist_shown_as
                        WHERE artist_id = credit.artist_id)
    LEFT JOIN artist_credits AS credit_name
           ON credit_name.artist_id = credit.artist_id
          AND credit_name.name_key = credit.credit_key
    WHERE credit.track_id = @parent.id
    HAVING COUNT(*) > 0
"#
);

// The credits the track shows, as a JSON array: its own, else its album's.
sqlive_compose::fragment!(
    TrackCredits on tracks { credits: String },
    r#"
    SELECT COALESCE(
        (SELECT @crate::records::credit_fragments::TrackOwnCredits(parent: own_track)
         FROM tracks AS own_track WHERE own_track.id = @parent.id),
        (SELECT (SELECT credited.credits
                 FROM (SELECT @crate::records::fragments::AlbumCredits(parent: album) AS credits) AS credited)
         FROM releases AS release
         JOIN albums AS album ON album.id = release.album_id
         WHERE release.id = @parent.release_id)) AS credits
"#
);

// The credits each track shows ([`TrackCredits`]),
// for a read that joins them to its tracks.
sqlive_compose::fragment!(
    ShownTrackCredits,
    r#"
    SELECT credited.id AS track_id,
           @crate::records::credit_fragments::TrackCredits(parent: credited) AS credits
    FROM tracks AS credited
"#
);

// The names of the artists credited on the work in a writing role, each once
// as the artist it shows after merges, joined by ", " in credit order; NULL
// when the work has no artists.
sqlive_compose::fragment!(
    WorkComposerNames on works { composer_names: Option<String> },
    r#"
    SELECT GROUP_CONCAT(credited.name, ', ' ORDER BY credited.position) AS composer_names
    FROM (
        SELECT composer.name, MIN(writing.position) AS position
        FROM work_artists AS writing
        JOIN artist_shown_as AS shown_as ON shown_as.artist_id = writing.artist_id
        JOIN artists AS composer ON composer.id = shown_as.shown_artist_id
        WHERE writing.work_id = @parent.id
          AND writing.role IN (SELECT role FROM writing_credit_roles)
        GROUP BY composer.id
    ) AS credited
"#
);

impl WorkComposerNames {
    /// The names joined in credit order; `None` for a work with no composer.
    pub(crate) fn into_names(self) -> Option<String> {
        self.composer_names
    }
}

// The credits a release's detail lists, each with the release it is on: the
// release's own (`part` 0), each track's own, and the credits in a role other
// than a writing one (a work's writing credits are its composers, named with
// the work) of the works each track performs (`part` 1). `kind` is 0 for a
// track's own credits and 1 for a work's, and `track_position` and the two
// positions are the credit's places in the order a release lists them. Each
// names the artist it shows after merges and the name it prints.
sqlive_compose::fragment!(
    ReleaseCreditLines {
        release_id: String,
        part: i64,
        track_id: Option<String>,
        track_position: i64,
        kind: i64,
        first_position: i64,
        second_position: i64,
        credit_id: String,
        artist_id: String,
        printed: String,
        role: String,
        role_words: Option<String>,
        qualifiers: String
    },
    r#"
    SELECT credit.release_id AS release_id, 0 AS part, NULL AS track_id,
           0 AS track_position, 0 AS kind, credit.position AS first_position,
           0 AS second_position, credit.id AS credit_id, shown.id AS artist_id,
           COALESCE(printed.name, shown.name) AS printed, credit.role AS role,
           credit.role_words AS role_words, credit.qualifiers AS qualifiers
    FROM release_artist_roles AS credit
    JOIN artists AS shown
      ON shown.id = (SELECT shown_artist_id FROM artist_shown_as
                     WHERE artist_id = credit.artist_id)
    LEFT JOIN artist_credits AS printed
           ON printed.artist_id = credit.artist_id AND printed.name_key = credit.credit_key
    UNION ALL
    SELECT track.release_id, 1, credit.track_id, track.position, 0, credit.position, 0,
           credit.id, shown.id, COALESCE(printed.name, shown.name), credit.role,
           credit.role_words, credit.qualifiers
    FROM track_artist_roles AS credit
    JOIN tracks AS track ON track.id = credit.track_id
    JOIN artists AS shown
      ON shown.id = (SELECT shown_artist_id FROM artist_shown_as
                     WHERE artist_id = credit.artist_id)
    LEFT JOIN artist_credits AS printed
           ON printed.artist_id = credit.artist_id AND printed.name_key = credit.credit_key
    UNION ALL
    SELECT track.release_id, 1, performed.track_id, track.position, 1, performed.position,
           credit.position, credit.id, shown.id, COALESCE(printed.name, shown.name),
           credit.role, credit.role_words, credit.qualifiers
    FROM work_artists AS credit
    JOIN track_works AS performed ON performed.work_id = credit.work_id
    JOIN tracks AS track ON track.id = performed.track_id
    JOIN artists AS shown
      ON shown.id = (SELECT shown_artist_id FROM artist_shown_as
                     WHERE artist_id = credit.artist_id)
    LEFT JOIN artist_credits AS printed
           ON printed.artist_id = credit.artist_id AND printed.name_key = credit.credit_key
    WHERE credit.role NOT IN (SELECT role FROM writing_credit_roles)
"#
);

// The credits the release's detail lists, as a JSON array of rows. The rows
// follow the order the release lists its credits in: the release's, in the
// order the catalog stated them; then for each track in playing order, its own
// credits, then those of the works it performs, each work in the order the
// track performs it. An artist holds a role (with its qualifiers) over the
// tracks its credits sit on, all of them for a credit of the release; artists
// holding a role over the same tracks are one row, in the order of their first
// credit, and rows go in the order of their first credit. A row whose role
// covers every track lists no tracks.
sqlive_compose::fragment!(
    ReleaseCredits on releases { credit_rows: String },
    r#"
    WITH own_mediums AS (
        SELECT medium.id, medium.release_id, medium.position, medium.medium, medium.side_count
        FROM release_mediums AS medium
        WHERE medium.release_id = @parent.id),
    own_places AS (
        SELECT held.id, held.release_id, held.medium_id, held.side, held.track_number,
               held.stated_length_ms, held.position
        FROM tracks AS held
        WHERE held.release_id = @parent.id),
    placed AS MATERIALIZED (
        SELECT place.track_id AS id, place.position, place.number, place.side_kind,
               place.side_ordinal
        FROM @crate::records::place_fragments::TrackPlaces(
            mediums: (SELECT * FROM own_mediums),
            places: (SELECT * FROM own_places)) AS place),
    listed AS (
        SELECT line.part, line.track_id, line.track_position, line.kind,
               line.first_position, line.second_position, line.credit_id, line.artist_id,
               line.printed, line.role, line.role_words, line.qualifiers
        FROM @crate::records::credit_fragments::ReleaseCreditLines AS line
        WHERE line.release_id = @parent.id),
    ordered AS (
        SELECT listed.*,
               ROW_NUMBER() OVER (ORDER BY part, track_position, kind, first_position,
                                           second_position, credit_id) AS ord
        FROM listed),
    named AS (
        SELECT ordered.*,
               FIRST_VALUE(printed) OVER (PARTITION BY role, role_words, qualifiers,
                                                       artist_id ORDER BY ord) AS held_printed
        FROM ordered),
    held AS (
        SELECT n.role, n.role_words, n.qualifiers, n.artist_id, MIN(n.ord) AS ord,
               MIN(n.held_printed) AS printed,
               CASE WHEN MAX(n.track_id IS NULL) = 1
                      OR COUNT(DISTINCT n.track_id) = (SELECT COUNT(*) FROM placed)
                    THEN NULL
                    ELSE (SELECT json_group_array(json_object(
                              'kind', p.side_kind, 'ordinal', p.side_ordinal,
                              'number', p.number) ORDER BY p.position)
                          FROM placed AS p
                          WHERE p.id IN (SELECT same.track_id FROM named AS same
                                         WHERE same.role = n.role
                                           AND same.role_words IS n.role_words
                                           AND same.qualifiers = n.qualifiers
                                           AND same.artist_id = n.artist_id))
               END AS tracks
        FROM named AS n
        GROUP BY n.role, n.role_words, n.qualifiers, n.artist_id),
    grouped AS (
        SELECT role, role_words, qualifiers, MIN(ord) AS first_ord,
               json_group_array(printed ORDER BY ord) AS artists, tracks
        FROM held
        GROUP BY role, role_words, qualifiers, tracks)
    SELECT json_group_array(json_object(
               'role', role, 'role_words', role_words, 'qualifiers', qualifiers,
               'artists', json(artists), 'tracks', json(tracks)) ORDER BY first_ord)
           AS credit_rows
    FROM grouped
"#
);

// Whether the release has any credit its detail lists.
sqlive_compose::fragment!(
    ReleaseHasCredits on releases { has_credits: bool },
    r#"
    SELECT COUNT(*) > 0 AS has_credits
    FROM (SELECT 1 FROM @crate::records::credit_fragments::ReleaseCreditLines AS line
          WHERE line.release_id = @parent.id
          LIMIT 1) AS any_line
"#
);
