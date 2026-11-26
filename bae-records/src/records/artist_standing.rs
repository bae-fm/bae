//! How each artist field of a draft or of a release editor's form, and each
//! of its credits, stands to the library: a picked artist is in the library, and
//! a credit stands as the rule's answer to it says, with the library artists it
//! could be when it is ambiguous. None names a new artist, one an artist in the
//! library, and several leave the credit ambiguous. A field's own standing counts
//! its credits': one credit that could be several is a choice among them, any
//! such credit leaves that many to choose, else the field is new, in the
//! library or partly new.

sqlive_compose::fragment!(
    ArtistFieldStanding with raw(part INTEGER, track_id TEXT, track_position INTEGER,
        position INTEGER, assignment_kind TEXT, artist_id TEXT, name TEXT, sort_name TEXT,
        musicbrainz_artist_id TEXT, discogs_artist_id TEXT, credit_key TEXT, credited_as TEXT,
        join_phrase TEXT),
        credit_answer(name_key TEXT, mb TEXT, dg TEXT, artists TEXT)
    { part: i64, track_id: Option<String>, field: String },
    r#"
    WITH placed_standing AS (
            SELECT part, track_id, position, credit, kind, CASE WHEN kind = 'ambiguous' THEN
                artists END AS choices
                FROM (
                SELECT assignment.part, assignment.track_id, assignment.position,
                    json_object('kind', assignment.assignment_kind, 'artist_id',
                    assignment.artist_id, 'name', CASE assignment.assignment_kind WHEN 'picked'
                    THEN picked.name WHEN 'credit' THEN assignment.name END, 'sort_name', CASE
                    assignment.assignment_kind WHEN 'picked' THEN picked.sort_name WHEN 'credit'
                    THEN assignment.sort_name END, 'musicbrainz_artist_id', CASE
                    assignment.assignment_kind WHEN 'picked' THEN picked.musicbrainz_artist_id
                    WHEN 'credit' THEN assignment.musicbrainz_artist_id END,
                    'discogs_artist_id', CASE assignment.assignment_kind WHEN 'picked' THEN
                    picked.discogs_artist_id WHEN 'credit' THEN assignment.discogs_artist_id
                    END, 'credit_key', picked_credit.name_key, 'credit_name',
                    picked_credit.name, 'credited_as', assignment.credited_as, 'join_phrase',
                    assignment.join_phrase) AS credit, CASE WHEN assignment.assignment_kind =
                    'picked' THEN 'library' WHEN answer.name_key IS NULL THEN NULL ELSE CASE
                    json_array_length(answer.artists) WHEN 0 THEN 'new' WHEN 1 THEN 'library'
                    ELSE 'ambiguous' END END AS kind, answer.artists AS artists
                FROM raw AS assignment
                LEFT JOIN artists AS picked ON picked.id = assignment.artist_id
                LEFT JOIN artist_credits AS picked_credit ON picked_credit.artist_id =
                    assignment.artist_id AND picked_credit.name_key = assignment.credit_key
                LEFT JOIN credit_answer AS answer ON assignment.assignment_kind = 'credit' AND
                    answer.name_key = bae_artist_name_key(assignment.name) AND answer.mb IS
                    assignment.musicbrainz_artist_id AND answer.dg IS
                    assignment.discogs_artist_id
            )
            ), field_counts AS (
            SELECT part, track_id, COUNT(*) AS total, SUM(kind = 'ambiguous') AS to_choose,
                SUM(kind = 'new') AS new_count, MAX(json_array_length(choices)) AS choice_count,
                json_group_array(json_object('credit', json(credit), 'kind', kind, 'choices',
                json(choices)) ORDER BY position) AS credits
            FROM placed_standing
            GROUP BY part, track_id
    )
    SELECT part, track_id, json_object('kind', CASE WHEN total = 1 AND to_choose = 1
            THEN 'choose' WHEN to_choose > 0 THEN 'some_to_choose' WHEN new_count = total
            THEN 'new' WHEN new_count = 0 THEN 'library' ELSE 'some_new' END, 'count', CASE
            WHEN total = 1 AND to_choose = 1 THEN choice_count WHEN to_choose > 0 THEN
            to_choose WHEN new_count = total OR new_count = 0 THEN NULL ELSE new_count END,
            'credits', json(credits)) AS field
    FROM field_counts
"#);

impl ArtistFieldStanding {
    /// The part of the draft the field is of (0 for the album's artists, 1 for
    /// an explicit track's), the track's id for part 1, and the field's JSON.
    pub fn into_columns(self) -> (i64, Option<String>, String) {
        (self.part, self.track_id, self.field)
    }
}
