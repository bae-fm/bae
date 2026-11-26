//! What the candidate state offers the candidate pane: what is running for the candidate, the evidence in its files, its draft, its failures, and the records and album its draft links to.

sqlive_compose::fragment!(
    Standing with keys(candidate_key TEXT) {
        standing_key: crate::CandidateKey,
        identification: Option<String>,
        standing_content_hash: Option<String>,
        standing_file_edit_revision: Option<i64>,
        admission: Option<String>,
        identify_run: Option<i64>,
        import_asked: Option<bool>,
        save_failure_error: Option<String>,
        import_step: Option<String>,
        import_id: Option<String>
    },
    r#"
    SELECT standing.candidate_key AS standing_key, standing.identification,
        standing.content_hash AS standing_content_hash,
        standing.file_edit_revision AS standing_file_edit_revision, standing.admission,
        standing.identify_run, standing.import_asked, standing.save_failure_error,
        standing.import_step, standing.import_id
    FROM keys
    JOIN import_candidate_standing AS standing ON standing.candidate_key = keys.candidate_key
"#);

sqlive_compose::fragment!(
    FileEvidence(key: crate::CandidateKey) {
        file_evidence_key: crate::CandidateKey,
        file_evidence: String
    },
    r#"
    SELECT :key AS file_evidence_key,
           COALESCE(
               (SELECT json_group_array(json_object(
                           'list', listed.list, 'value', listed.value,
                           'origin_path', listed.origin_path)
                           ORDER BY listed.list = 'barcode', listed.position)
                FROM import_candidate_signal_value AS listed
                WHERE listed.signals_id = (
                          SELECT signals.signals_id
                          FROM import_candidate_signals AS signals
                          WHERE signals.content_hash = (
                              SELECT owner.content_hash
                              FROM @crate::tables::import_state::current_owner::CurrentOwner(
                                  key: :key) AS owner))
                  AND listed.list IN ('disc_id', 'barcode')
                  AND listed.origin_path IS NOT NULL),
               '[]') AS file_evidence
    FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan
"#);

sqlive_compose::fragment!(
    PickedLibraryStatus(key: crate::CandidateKey) {
        picked_status_key: crate::CandidateKey,
        picked_status_release_id: Option<String>,
        picked_status_albums: Option<String>
    },
    r#"
    WITH status(release_id, albums) AS (
        SELECT found.release_id, found.albums
        FROM @bae_records::records::fragments::LibraryStatusRows(checks: (
            SELECT 0, link.release_id, link.source,
                   @crate::tables::source_releases::row_fragments::LinkedAlbumGroup(parent: link)
                       AS source_group_id
            FROM import_candidate_release_link AS link
            WHERE link.content_hash = (
                      SELECT scan.content_hash
                      FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key)
                          AS scan)
              AND link.kind = 'pressing')) AS found)
    SELECT :key AS picked_status_key, release_id AS picked_status_release_id,
           albums AS picked_status_albums
    FROM status
"#);

sqlive_compose::fragment!(
    CandidateAlbum(key: crate::CandidateKey) {
        candidate_album_key: crate::CandidateKey,
        candidate_album_kind: String
    },
    r#"
    WITH pick AS (
        SELECT CASE WHEN pick.content_hash IS NULL THEN 'records'
                    WHEN pick.album_id IS NULL THEN 'new'
                    ELSE 'album' END AS pick,
               pick.album_id AS album_id
        FROM (SELECT 1)
        LEFT JOIN import_candidate_album_pick AS pick
               ON pick.content_hash = (
                   SELECT scan.content_hash
                   FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key)
                       AS scan)),
    choice(kind, album_id) AS (
        SELECT pick.pick, pick.album_id FROM pick WHERE pick.pick <> 'records'
        UNION ALL
        SELECT 'named', named.album_id
        FROM @bae_records::records::fragments::AlbumRecordsName(records: (
            SELECT key, value ->> 'catalog', value ->> 'kind', value ->> 'key', value ->> 'album'
            FROM json_each((
                SELECT json_group_array(json_object(
                           'catalog', claimed.catalog, 'kind', claimed.kind,
                           'key', claimed.key, 'album', claimed.album)
                           ORDER BY claimed.position)
                FROM import_candidate_claimed_record AS claimed
                WHERE claimed.content_hash = (
                    SELECT scan.content_hash
                    FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key)
                        AS scan))))) AS named
        JOIN pick ON pick.pick = 'records')
    SELECT :key AS candidate_album_key, COALESCE(choice.kind, 'none') AS candidate_album_kind,
           album.*
    FROM (SELECT 1)
    LEFT JOIN choice
    LEFT JOIN @bae_records::records::fragments::LibraryAlbum AS album
           ON album.album_id = choice.album_id
"#);

sqlive_compose::fragment!(
    DraftRows with candidate(content_hash TEXT) {
        album_title: String,
        album_year: String,
        year: String,
        country: Option<String>,
        region: Option<String>,
        media: String,
        status: Option<String>,
        packaging: Option<String>,
        discogs_details: String,
        labels: String,
        barcodes: String,
        album_assignments: String,
        mediums: String,
        tracks: String,
        release_roles: String,
        works: String,
        draft_parts: String
    },
    r#"
    WITH statement(album_title, album_year, year, country, region, media, status, packaging, discogs_details, labels, barcodes, album_assignments, mediums, tracks, release_roles, works, draft_parts) AS (
            SELECT album_title, album_year, year, country, region, media, status,
                packaging, discogs_details,(
            SELECT json_group_array(json_object('name', label.name, 'catalog_number',
                label.catalog_number, 'musicbrainz_label_id', label.musicbrainz_label_id,
                'discogs_label_id', label.discogs_label_id) ORDER BY label.position)
            FROM import_candidate_edit_label AS label
            WHERE label.content_hash = edit.content_hash
            ) AS labels,(
            SELECT json_group_array(barcode.barcode ORDER BY barcode.position)
            FROM import_candidate_edit_barcode AS barcode
            WHERE barcode.content_hash = edit.content_hash
            ) AS barcodes,(
            SELECT json_group_array(json_object('kind', assignment.assignment_kind, 'artist_id',
                assignment.artist_id, 'name', CASE assignment.assignment_kind WHEN 'picked' THEN
                picked.name WHEN 'credit' THEN assignment.name END, 'sort_name', CASE
                assignment.assignment_kind WHEN 'picked' THEN picked.sort_name WHEN 'credit'
                THEN assignment.sort_name END, 'musicbrainz_artist_id', CASE
                assignment.assignment_kind WHEN 'picked' THEN picked.musicbrainz_artist_id WHEN
                'credit' THEN assignment.musicbrainz_artist_id END, 'discogs_artist_id', CASE
                assignment.assignment_kind WHEN 'picked' THEN picked.discogs_artist_id WHEN
                'credit' THEN assignment.discogs_artist_id END, 'credit_key',
                picked_credit.name_key, 'credit_name', picked_credit.name, 'credited_as',
                assignment.credited_as, 'join_phrase', assignment.join_phrase) ORDER BY
                assignment.position)
            FROM import_candidate_album_artist_assignment AS assignment
            LEFT JOIN @bae_records::records::spread_fragments::ArtistRow AS picked ON
                picked.artist_id = assignment.artist_id
            LEFT JOIN @bae_records::records::spread_fragments::ArtistCreditName AS picked_credit
                ON picked_credit.artist_id = assignment.artist_id AND picked_credit.name_key =
                assignment.credit_key
            WHERE assignment.content_hash = edit.content_hash
            ) AS album_assignments,(
            SELECT json_group_array(json_object('carrier', medium.medium, 'sides',
                medium.side_count) ORDER BY medium.position)
            FROM import_candidate_medium AS medium
            WHERE medium.content_hash = edit.content_hash
            ) AS mediums,(
                SELECT json_group_array(json_object('track_id', track.track_id, 'title',
                    track.title, 'assignment_kind', track.artist_assignment_kind, 'medium',
                    track.medium, 'side', track.side, 'track_number', track.track_number,
                    'stated_length_ms', track.stated_length_ms, 'file_kind', track.file_kind,
                    'file_id', track.file_id, 'sheet_id', track.sheet_id, 'slice_index',
                    track.slice_index, 'assignments', json((
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
                WHERE assignment.content_hash = track.content_hash AND assignment.track_id =
                    track.track_id
                )), 'roles', json((
                SELECT json_group_array(json_object('name', role.name, 'sort_name',
                    role.sort_name, 'musicbrainz_artist_id', role.musicbrainz_artist_id,
                    'discogs_artist_id', role.discogs_artist_id, 'credited_as',
                    role.credited_as, 'source', role.source, 'role', role.role, 'role_words',
                    role.role_words, 'qualifiers', role.qualifiers) ORDER BY role.position)
                FROM import_candidate_track_role AS role
                WHERE role.content_hash = track.content_hash AND role.track_id = track.track_id
                )), 'works', json((
                SELECT json_group_array(performed.musicbrainz_work_id ORDER BY
                    performed.position)
                FROM import_candidate_track_work AS performed
                WHERE performed.content_hash = track.content_hash AND performed.track_id =
                    track.track_id
            ))) ORDER BY track.position)
            FROM import_candidate_track AS track
            WHERE track.content_hash = edit.content_hash
            ) AS tracks,(
            SELECT json_group_array(json_object('name', role.name, 'sort_name', role.sort_name,
                'musicbrainz_artist_id', role.musicbrainz_artist_id, 'discogs_artist_id',
                role.discogs_artist_id, 'credited_as', role.credited_as, 'source', role.source,
                'role', role.role, 'role_words', role.role_words, 'qualifiers', role.qualifiers)
                ORDER BY role.position)
            FROM import_candidate_release_role AS role
            WHERE role.content_hash = edit.content_hash
            ) AS release_roles,(
                SELECT json_group_array(json_object('musicbrainz_work_id',
                    work.musicbrainz_work_id, 'title', work.title, 'disambiguation',
                    work.disambiguation, 'work_type', work.work_type, 'credits', json((
                SELECT json_group_array(json_object('name', credit.name, 'sort_name',
                    credit.sort_name, 'musicbrainz_artist_id', credit.musicbrainz_artist_id,
                    'discogs_artist_id', credit.discogs_artist_id, 'credited_as',
                    credit.credited_as, 'role', credit.role, 'role_words', credit.role_words,
                    'qualifiers', credit.qualifiers) ORDER BY credit.position)
                FROM import_candidate_work_credit AS credit
                WHERE credit.content_hash = work.content_hash AND credit.musicbrainz_work_id =
                    work.musicbrainz_work_id
            ))) ORDER BY work.position)
            FROM import_candidate_work AS work
            WHERE work.content_hash = edit.content_hash
            ) AS works,(
            SELECT json_group_array(json_object('parent', part.parent_work_id, 'child',
                part.child_work_id) ORDER BY part.position)
            FROM import_candidate_work_part AS part
            WHERE part.content_hash = edit.content_hash
        ) AS parts
        FROM import_candidate_edit AS edit
            WHERE edit.content_hash IN (
            SELECT content_hash
            FROM candidate
        )
    )
    SELECT album_title, album_year, year, country, region, media, status, packaging, discogs_details, labels, barcodes, album_assignments, mediums, tracks, release_roles, works, draft_parts
    FROM statement
"#);

sqlive_compose::fragment!(
    DraftFlags(key: crate::CandidateKey) {
        draft_flags_key: crate::CandidateKey,
        draft_blank: bool,
        draft_valid: bool
    },
    r#"
    WITH statement(draft_blank, draft_valid) AS (
        SELECT draft_blank, draft_valid
        FROM import_candidate_edit
            WHERE content_hash =(
                SELECT scan.content_hash
                FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan)
    )
    SELECT :key AS draft_flags_key, draft_blank, draft_valid
    FROM statement
"#);

sqlive_compose::fragment!(
    Failure(key: crate::CandidateKey) {
        failure_key: crate::CandidateKey,
        failure_error: String,
        failed_at: String
    },
    r#"
    WITH statement(failure_error, failed_at) AS (
        SELECT failure.error AS failure_error, failure.failed_at
        FROM import_candidate_failure failure
            WHERE failure.content_hash =(
                SELECT scan.content_hash
                FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan)
    )
    SELECT :key AS failure_key, failure_error, failed_at
    FROM statement
"#);

sqlive_compose::fragment!(
    PaneFailure(key: crate::CandidateKey) {
        pane_failure_key: crate::CandidateKey,
        pane_failure_command: String,
        pane_failure_category: String,
        pane_failure_detail: String
    },
    r#"
    WITH statement(pane_failure_command, pane_failure_category, pane_failure_detail) AS (
        SELECT command, category, detail
        FROM import_candidate_pane_failure
            WHERE content_hash =(
                SELECT scan.content_hash
                FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan)
    )
    SELECT :key AS pane_failure_key, pane_failure_command, pane_failure_category, pane_failure_detail
    FROM statement
"#);

sqlive_compose::fragment!(
    ClaimedRecords(key: crate::CandidateKey) {
        claimed_key: crate::CandidateKey,
        claimed_records: Option<String>
    },
    r#"
    SELECT :key AS claimed_key, (
            SELECT CASE WHEN EXISTS (
            SELECT 1
            FROM @crate::tables::import_state::current_owner::CurrentOwner(key: :key) AS owner
            ) THEN (
            SELECT json_group_array(json_object('catalog', claimed.catalog, 'kind',
                claimed.kind, 'key', claimed.key, 'album', claimed.album) ORDER BY
                claimed.position)
            FROM import_candidate_claimed_record AS claimed
                WHERE claimed.content_hash =(
                    SELECT scan.content_hash
                    FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan)
        ) ELSE '[]' END
    ) AS claimed_records
"#);
