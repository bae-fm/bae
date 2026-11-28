//! What the candidate state offers the candidate pane: what is running for the candidate, the evidence in its files, its draft and cover, its failures, and the records and album its draft links to.

coven::fragment!(
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

// The values each scanned candidate of `keys` read from its files that name
// the file they came from: its disc IDs, then its barcodes.
coven::fragment!(
    FileEvidence with keys(candidate_key BLOB) {
        file_evidence_key: crate::CandidateKey,
        file_evidence: String
    },
    r#"
    SELECT scan.candidate_key AS file_evidence_key,
           COALESCE(
               (SELECT json_group_array(json_object(
                           'list', listed.list, 'value', listed.value,
                           'origin_path', listed.origin_path)
                           ORDER BY listed.list = 'barcode', listed.position)
                FROM import_candidate_signal_value AS listed
                WHERE listed.signals_id = (
                          SELECT signals.signals_id
                          FROM import_candidate_signals AS signals
                          WHERE signals.content_hash = owner.content_hash)
                  AND listed.list IN ('disc_id', 'barcode')
                  AND listed.origin_path IS NOT NULL),
               '[]') AS file_evidence
    FROM keys
    JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scan
        ON scan.candidate_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::current_owner::CurrentOwner(keys: (SELECT * FROM keys)) AS owner
        ON owner.owner_key = scan.candidate_key
"#);

// Which library albums hold the pressing each scanned candidate of `keys`
// links its draft to; no row for a candidate whose draft links no pressing.
// Each link is checked once, numbered by its row.
coven::fragment!(
    PickedLibraryStatus with keys(candidate_key BLOB) {
        picked_status_key: crate::CandidateKey,
        picked_status_release_id: Option<String>,
        picked_status_albums: Option<String>
    },
    r#"
    WITH linked(candidate_key, position, release_id, source, source_group_id) AS (
        SELECT scan.candidate_key, link.rowid, link.release_id, link.source,
               @crate::tables::source_releases::row_fragments::LinkedAlbumGroup(parent: link)
        FROM keys
        JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scan
            ON scan.candidate_key = keys.candidate_key
        JOIN import_candidate_release_link AS link
            ON link.content_hash = scan.content_hash AND link.kind = 'pressing')
    SELECT linked.candidate_key AS picked_status_key,
           found.release_id AS picked_status_release_id, found.albums AS picked_status_albums
    FROM linked
    JOIN @bae_records::records::fragments::LibraryStatusRows(checks: (
        SELECT DISTINCT position, release_id, source, source_group_id FROM linked)) AS found
        ON found.position = linked.position
"#);

coven::fragment!(
    CandidateAlbum(key: crate::CandidateKey) {
        candidate_album_key: crate::CandidateKey,
        candidate_album_kind: String
    },
    r#"
    WITH pick AS (
        SELECT CASE WHEN pick.pick_hash IS NULL THEN 'records'
                    WHEN pick.picked_album_id IS NULL THEN 'new'
                    ELSE 'album' END AS pick,
               pick.picked_album_id AS album_id
        FROM (SELECT 1)
        LEFT JOIN @crate::tables::import_state::album_pick_rows::AlbumPickRow AS pick
               ON pick.pick_hash = (
                   SELECT scan.content_hash
                   FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key)),
    choice(kind, album_id) AS (
        SELECT pick.pick, pick.album_id FROM pick WHERE pick.pick <> 'records'
        UNION ALL
        SELECT 'named', named.album_id
        FROM @bae_records::records::fragments::AlbumRecordsName(records: (
            SELECT given.key, given.value ->> 'catalog', given.value ->> 'kind',
                   given.value ->> 'key', given.value ->> 'album'
            FROM json_each((
                SELECT json_group_array(json_object(
                           'catalog', claimed.catalog, 'kind', claimed.kind,
                           'key', claimed.key, 'album', claimed.album)
                           ORDER BY claimed.position)
                FROM import_candidate_claimed_record AS claimed
                WHERE claimed.content_hash = (
                    SELECT scan.content_hash
                    FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key))) AS given),
            excluding: (SELECT NULL WHERE 0)) AS named
        JOIN pick ON pick.pick = 'records')
    SELECT :key AS candidate_album_key, COALESCE(choice.kind, 'none') AS candidate_album_kind,
           album.*
    FROM (SELECT 1)
    LEFT JOIN choice
    LEFT JOIN @bae_records::records::fragments::LibraryAlbum AS album
           ON album.album_id = choice.album_id
"#);

coven::fragment!(
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
                SELECT json_group_array(
                    json(@crate::tables::import_state::row_fragments::DraftTrackRecord(
                        parent: track))
                    ORDER BY track.position)
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

coven::fragment!(
    DraftFlags with keys(candidate_key BLOB) {
        draft_flags_key: crate::CandidateKey,
        draft_blank: bool,
        draft_valid: bool
    },
    r#"
    SELECT scan.candidate_key AS draft_flags_key, edit.draft_blank, edit.draft_valid
    FROM keys
    JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scan
        ON scan.candidate_key = keys.candidate_key
    JOIN import_candidate_edit AS edit ON edit.content_hash = scan.content_hash
"#);

coven::fragment!(
    Failure with keys(candidate_key BLOB) {
        failure_key: crate::CandidateKey,
        failure_error: String,
        failed_at: String
    },
    r#"
    SELECT scan.candidate_key AS failure_key, failure.error AS failure_error, failure.failed_at
    FROM keys
    JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scan
        ON scan.candidate_key = keys.candidate_key
    JOIN import_candidate_failure AS failure ON failure.content_hash = scan.content_hash
"#);

coven::fragment!(
    PaneFailure with keys(candidate_key BLOB) {
        pane_failure_key: crate::CandidateKey,
        pane_failure_command: String,
        pane_failure_category: String,
        pane_failure_detail: String
    },
    r#"
    SELECT scan.candidate_key AS pane_failure_key, failure.command AS pane_failure_command, failure.category AS pane_failure_category,
        failure.detail AS pane_failure_detail
    FROM keys
    JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scan
        ON scan.candidate_key = keys.candidate_key
    JOIN import_candidate_pane_failure AS failure ON failure.content_hash = scan.content_hash
"#);

// The records each scanned candidate of `keys` claims for its draft, as a JSON
// array; empty while the candidate has no current state.
coven::fragment!(
    ClaimedRecords with keys(candidate_key BLOB) {
        claimed_key: crate::CandidateKey,
        claimed_records: Option<String>
    },
    r#"
    SELECT scan.candidate_key AS claimed_key, (
            SELECT CASE WHEN owner.owner_key IS NOT NULL THEN (
            SELECT json_group_array(json_object('catalog', claimed.catalog, 'kind',
                claimed.kind, 'key', claimed.key, 'album', claimed.album) ORDER BY
                claimed.position)
            FROM import_candidate_claimed_record AS claimed
                WHERE claimed.content_hash = scan.content_hash
        ) ELSE '[]' END
    ) AS claimed_records
    FROM keys
    JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scan
        ON scan.candidate_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::current_owner::CurrentOwner(keys: (SELECT * FROM keys)) AS owner
        ON owner.owner_key = scan.candidate_key
"#);

coven::fragment!(
    CoverChoice(key: crate::CandidateKey) {
        cover_choice_key: crate::CandidateKey,
        cover_row: String,
        cover_kind: String,
        cover_file_id: Option<String>,
        cover_url: Option<String>,
        cover_source: Option<String>,
        cover_copies: String,
        cover_local_path: Option<Vec<u8>>,
        cover_local_size: Option<i64>,
        cover_local_modified: Option<i64>,
        cover_embedded_source: Option<String>,
        cover_embedded_size: Option<i64>,
        cover_embedded_modified: Option<i64>,
        cover_embedded_path: Option<Vec<u8>>
    },
    r#"
    WITH statement(cover_row, cover_kind, cover_file_id, cover_url, cover_source, cover_copies, cover_local_path, cover_local_size, cover_local_modified, cover_embedded_source, cover_embedded_size, cover_embedded_modified, cover_embedded_path) AS (
            SELECT cover.content_hash AS cover_row, cover.kind AS cover_kind, cover.file_id AS
                cover_file_id, cover.url AS cover_url, cover.source AS cover_source,(
            SELECT json_group_array(json_object('url', copy.url, 'max_edge', copy.max_edge)
                ORDER BY copy.max_edge)
            FROM import_candidate_cover_copy AS copy
            WHERE copy.content_hash = cover.content_hash AND copy.image_url = cover.url
        ) AS cover_copies, cover_local.absolute_path AS cover_local_path, cover_local.size AS
            cover_local_size, cover_local.modified_at_ns AS cover_local_modified,
            cover_snapshot.embedded_cover_source_relative_path AS cover_embedded_source,
            cover_tags.file_size AS cover_embedded_size, cover_tags.modified_at_ns AS
            cover_embedded_modified, cover_source.absolute_path AS cover_embedded_path
        FROM @crate::tables::folder_scans::row_fragments::ScanContent AS c
        JOIN import_candidate_cover AS cover ON cover.content_hash = c.content_hash
        LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateFiles AS cover_local ON cover_local.candidate_key = :key AND cover.kind = 'local' AND cover_local.relative_path = cover.file_id
        LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateTagSnapshot AS cover_snapshot ON cover_snapshot.candidate_key = :key AND cover.kind = 'embedded'
        LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateFileTags AS cover_tags ON cover_tags.candidate_key = :key AND cover_snapshot.candidate_key IS NOT NULL AND
            cover_tags.relative_path = cover.file_id
        LEFT JOIN @crate::tables::folder_scans::row_fragments::CandidateFiles AS cover_source ON cover_source.candidate_key = :key AND cover_snapshot.candidate_key IS NOT NULL AND
            cover_source.relative_path = cover.file_id
        WHERE c.candidate_key = :key
        LIMIT 1
    )
    SELECT :key AS cover_choice_key, cover_row, cover_kind, cover_file_id, cover_url, cover_source, cover_copies, cover_local_path, cover_local_size, cover_local_modified, cover_embedded_source, cover_embedded_size, cover_embedded_modified, cover_embedded_path
    FROM statement
"#);
