//! The candidate state's rows that other domains' fragments spread to reach
//! them.

// The album's credits and the tracks' are two fragments. SQLite plans a read
// of a fragment of one table as a read of that table, and runs a union of the
// two as a compound subquery.
coven::fragment!(
    AlbumCreditNames {
        name_key: Option<String>
    },
    r#"
    SELECT bae_artist_name_key(held.name) AS name_key, held.musicbrainz_artist_id,
        held.discogs_artist_id
    FROM import_candidate_album_artist_assignment AS held
    WHERE held.assignment_kind = 'credit'
"#);

coven::fragment!(
    TrackCreditNames {
        name_key: Option<String>
    },
    r#"
    SELECT bae_artist_name_key(held.name) AS name_key, held.musicbrainz_artist_id,
        held.discogs_artist_id
    FROM import_candidate_track_artist_assignment AS held
    WHERE held.assignment_kind = 'credit'
"#);

coven::fragment!(
    DraftCreditRows(key: crate::CandidateKey) {
        part: i64,
        track_id: Option<String>,
        position: i64,
        assignment_kind: String,
        artist_id: Option<String>,
        name: Option<String>,
        sort_name: Option<String>,
        musicbrainz_artist_id: Option<String>,
        discogs_artist_id: Option<String>,
        credit_key: Option<String>,
        credited_as: Option<String>,
        join_phrase: Option<String>
    },
    r#"
    SELECT 0 AS part, NULL AS track_id, a.position AS position,
        a.assignment_kind, a.artist_id, a.name, a.sort_name, a.musicbrainz_artist_id,
        a.discogs_artist_id, a.credit_key, a.credited_as, a.join_phrase
    FROM import_candidate_album_artist_assignment AS a
    WHERE a.content_hash = (
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key
    )
    UNION ALL SELECT 1, t.track_id, a.position, a.assignment_kind,
        a.artist_id, a.name, a.sort_name, a.musicbrainz_artist_id, a.discogs_artist_id,
        a.credit_key, a.credited_as, a.join_phrase
    FROM import_candidate_track_artist_assignment AS a
    JOIN import_candidate_track AS t ON t.content_hash = a.content_hash AND t.track_id =
        a.track_id
    WHERE a.content_hash = (
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key
    ) AND t.artist_assignment_kind = 'explicit'
"#);

coven::fragment!(
    DraftTrackRecord on import_candidate_track {
        record: String
    },
    r#"
    SELECT json_object('track_id', @parent.track_id, 'title',
                    @parent.title, 'assignment_kind', @parent.artist_assignment_kind, 'medium',
                    @parent.medium, 'side', @parent.side, 'track_number', @parent.track_number,
                    'stated_length_ms', @parent.stated_length_ms, 'file_kind', @parent.file_kind,
                    'file_id', @parent.file_id, 'sheet_id', @parent.sheet_id, 'slice_index',
                    @parent.slice_index, 'assignments', json((
                SELECT json_group_array(json_object('kind', assignment.assignment_kind,
                    'artist_id', assignment.artist_id, 'name', CASE assignment.assignment_kind
                    WHEN 'picked' THEN picked.name WHEN 'credit' THEN assignment.name END,
                    'sort_name', CASE assignment.assignment_kind WHEN 'picked' THEN
                    picked.sort_name WHEN 'credit' THEN assignment.sort_name END,
                    'musicbrainz_artist_id', CASE assignment.assignment_kind WHEN 'picked' THEN
                    picked.musicbrainz_artist_id WHEN 'credit' THEN
                    assignment.musicbrainz_artist_id END, 'discogs_artist_id', CASE
                    assignment.assignment_kind WHEN 'picked' THEN picked.discogs_artist_id WHEN
                    'credit' THEN assignment.discogs_artist_id END, 'credit_key',
                    picked_credit.name_key, 'credit_name', picked_credit.name, 'credited_as',
                    assignment.credited_as, 'join_phrase', assignment.join_phrase) ORDER BY
                    assignment.position)
                FROM import_candidate_track_artist_assignment AS assignment
                LEFT JOIN @bae_records::records::spread_fragments::ArtistRow AS picked ON
                    picked.artist_id = assignment.artist_id
                LEFT JOIN @bae_records::records::spread_fragments::ArtistCreditName AS
                    picked_credit ON picked_credit.artist_id = assignment.artist_id AND
                    picked_credit.name_key = assignment.credit_key
                WHERE assignment.content_hash = @parent.content_hash AND assignment.track_id =
                    @parent.track_id
                )), 'roles', json((
                SELECT json_group_array(json_object('name', role.name, 'sort_name',
                    role.sort_name, 'musicbrainz_artist_id', role.musicbrainz_artist_id,
                    'discogs_artist_id', role.discogs_artist_id, 'credited_as',
                    role.credited_as, 'source', role.source, 'role', role.role, 'role_words',
                    role.role_words, 'qualifiers', role.qualifiers) ORDER BY role.position)
                FROM import_candidate_track_role AS role
                WHERE role.content_hash = @parent.content_hash AND role.track_id = @parent.track_id
                )), 'works', json((
                SELECT json_group_array(performed.musicbrainz_work_id ORDER BY
                    performed.position)
                FROM import_candidate_track_work AS performed
                WHERE performed.content_hash = @parent.content_hash AND performed.track_id =
                    @parent.track_id
            ))) AS record
"#);

coven::fragment!(
    DraftTrackRows(key: crate::CandidateKey),
    r#"
    SELECT track.content_hash, track.track_id, track.position, track.title, track.medium,
        track.side, track.track_number, track.stated_length_ms, track.file_kind, track.file_id,
        track.sheet_id, track.slice_index,
        @crate::tables::import_state::row_fragments::DraftTrackRecord(parent: track) AS track_json
    FROM import_candidate_track AS track
    WHERE track.content_hash = (
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key
        LIMIT 1
    )
"#
);

coven::fragment!(
    DraftMediumRows(key: crate::CandidateKey),
    r#"
    SELECT medium.position, medium.content_hash, medium.medium, medium.side_count
    FROM import_candidate_medium AS medium
    WHERE medium.content_hash = (
        SELECT scan.content_hash
        FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key
        LIMIT 1
    )
"#
);

// The covers each candidate's linked pressing offers its picker, by the
// candidate's content hash, each cover as `RemoteCover` JSON. A candidate has
// them only while its draft is the one bae read from that pressing.
coven::fragment!(
    OfferedCovers,
    r#"
    SELECT offered.content_hash AS offered_hash, offered.position, offered.source, offered.cover
    FROM import_candidate_offered_cover AS offered
    WHERE EXISTS (SELECT 1 FROM import_candidate_draft_provenance AS provenance
                  JOIN import_candidate_release_link AS link
                      ON link.content_hash = provenance.content_hash
                     AND link.kind = 'pressing'
                     AND (link.source, link.release_id)
                         = (provenance.source, provenance.release_id)
                  WHERE provenance.content_hash = offered.content_hash
                    AND provenance.kind = 'external_release')
"#
);
