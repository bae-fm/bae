//! A release's tracks as its detail groups them, with how long they play and
//! whether any performs a work.

// No person's stars: the `stars` relation of a read that shows none.
sqlive_compose::fragment!(
    NoTrackStars { track_id: Option<String>, stars: Option<i64> },
    r#"
    SELECT NULL AS track_id, NULL AS stars
    WHERE 0
"#
);

// The release's tracks in playing order, grouped by side or disc (a run of
// tracks on one side is one group), as a JSON array of groups: each group
// with its tracks' lengths added up, whether the release has more than one
// group, where its second column starts, and its tracks with their credits,
// works and the stars `stars` gives each.
sqlive_compose::fragment!(
    TrackGroups on releases
    with stars(track_id TEXT, stars INTEGER) { track_groups: String },
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
    placed AS (
        SELECT track.id, track.title, track.duration_ms, place.position, place.number,
               place.side_kind, place.side_ordinal,
               COALESCE(@crate::records::credit_fragments::TrackOwnCredits(parent: track), '[]')
                   AS own_credits,
               @crate::records::credit_fragments::TrackCredits(parent: track) AS shown_credits,
               (SELECT json_group_array(json_object(
                           'work_id', work.id, 'title', work.title, 'composer_names',
                           @crate::records::credit_fragments::WorkComposerNames(parent: work))
                           ORDER BY performed.position, performed.id)
                FROM track_works AS performed
                JOIN works AS work ON work.id = performed.work_id
                WHERE performed.track_id = track.id) AS works,
               (SELECT seen.stars FROM stars AS seen WHERE seen.track_id = track.id)
                   AS my_stars
        FROM tracks AS track
        JOIN @crate::records::place_fragments::TrackPlaces(
            mediums: (SELECT * FROM own_mediums),
            places: (SELECT * FROM own_places)) AS place
          ON place.track_id = track.id
        WHERE track.release_id = @parent.id),
    island AS (
        SELECT placed.*,
               ROW_NUMBER() OVER (ORDER BY position)
               - ROW_NUMBER() OVER (PARTITION BY side_kind, side_ordinal ORDER BY position)
                   AS island
        FROM placed),
    grouped AS (
        SELECT side_kind, side_ordinal, MIN(position) AS first_position,
               SUM(duration_ms) AS total_duration_ms,
               COUNT(*) OVER () > 1 AS shows_time,
               CASE WHEN COUNT(*) > 8 THEN (COUNT(*) + 1) / 2 END AS second_column_start,
               json_group_array(json_object(
                   'id', id, 'title', title, 'duration_ms', duration_ms, 'number', number,
                   'own_credits', json(own_credits), 'shown_credits', json(shown_credits),
                   'works', json(works), 'my_stars', my_stars) ORDER BY position) AS tracks
        FROM island
        GROUP BY side_kind, side_ordinal, island)
    SELECT json_group_array(json_object(
               'side_kind', side_kind, 'side_ordinal', side_ordinal,
               'total_duration_ms', total_duration_ms, 'shows_time', shows_time,
               'second_column_start', second_column_start,
               'tracks', json(tracks)) ORDER BY first_position) AS track_groups
    FROM grouped
"#
);

// How long the release's tracks play.
sqlive_compose::fragment!(
    TotalDuration on releases { total_duration_ms: i64 },
    r#"
    SELECT COALESCE(SUM(held.duration_ms), 0) AS total_duration_ms
    FROM tracks AS held
    WHERE held.release_id = @parent.id
"#
);

// Whether any of the release's tracks performs a work.
sqlive_compose::fragment!(
    PerformsWorks on releases { performs_works: bool },
    r#"
    SELECT COUNT(*) > 0 AS performs_works
    FROM (SELECT 1 FROM tracks AS held
          JOIN track_works AS performed ON performed.track_id = held.id
          WHERE held.release_id = @parent.id
          LIMIT 1) AS any_work
"#
);
