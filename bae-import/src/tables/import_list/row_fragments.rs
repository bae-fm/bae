//! What a row of the import list holds past its placement, as fragments over
//! the row of the placement that lists it: the commands it offers, what its
//! release's other tables hold, and the header of its group. Each is one JSON
//! document, which `row_columns` decodes.

// The commands the release the placement row lists offers, one flag per
// command, `1` when the release offers it. They are decided from the
// placement row and what is running for the release, so every surface offers
// the same commands.
//
// While an import runs for the release, the release offers only Cancel, not
// even Skip. Once the import starts writing the release, it offers nothing,
// because the write finishes whatever the person asks. While an identify run
// is in progress, the release offers only Cancel and Skip, and Import once
// the run reads its pick, until the person asks for the import. Identifying
// again after a finished answer would fail the same way, so bae does not
// offer it, and the list shows a stored lookup unchanged, so only a release
// with no stored lookup offers Identify. Regrouping comes before any of that:
// a release that is imported, or has work running for it, offers neither
// Separate nor Combine; any other release offers Separate when a grouping
// reads it, even one that cannot be worked on as it stands, and Combine when
// the person can act on it. Every release offers to reveal its folders.
sqlive_compose::fragment!(
    ListedActions on import_list_placement { actions: String },
    r#"
    SELECT json_object(
        'can_Import', COALESCE((@parent.item = 'candidate' AND @parent.actionable AND
            NOT (st.import_step IS NOT NULL) AND @parent.tab = 'pending' AND
            ((COALESCE(st.identification IN ('queued', 'running', 'finalizing',
            'reading_pick'), 0) AND st.identification = 'reading_pick' AND NOT
            st.import_asked) OR (NOT COALESCE(st.identification IN ('queued', 'running',
            'finalizing', 'reading_pick'), 0) AND @parent.draft_valid))), 0),
        'can_CancelImport', COALESCE((@parent.item = 'candidate' AND @parent.actionable
            AND st.import_step IN ('queued', 'validating_source_files',
            'fetching_artist_pictures', 'reading_files', 'measuring_loudness')), 0),
        'can_Identify', COALESCE((@parent.item = 'candidate' AND @parent.actionable AND
            NOT (st.import_step IS NOT NULL) AND @parent.tab = 'pending' AND NOT
            COALESCE(st.identification IN ('queued', 'running', 'finalizing',
            'reading_pick'), 0) AND @parent.lookup IS NULL AND
            COALESCE(st.identification <> 'inapplicable', 1)), 0),
        'can_CancelIdentification', COALESCE((@parent.item = 'candidate' AND
            @parent.actionable AND NOT (st.import_step IS NOT NULL) AND @parent.tab =
            'pending' AND COALESCE(st.identification IN ('queued', 'running',
            'finalizing', 'reading_pick'), 0)), 0),
        'can_RetryIdentification', COALESCE((@parent.item = 'candidate' AND
            @parent.actionable AND NOT (st.import_step IS NOT NULL) AND @parent.tab =
            'pending' AND NOT COALESCE(st.identification IN ('queued', 'running',
            'finalizing', 'reading_pick'), 0) AND (@parent.lookup = 'failed' OR
            st.identification = 'not_written')), 0),
        'can_ResetToFileMetadata', COALESCE((@parent.item = 'candidate' AND
            @parent.actionable AND NOT (st.import_step IS NOT NULL) AND @parent.tab =
            'pending' AND NOT COALESCE(st.identification IN ('queued', 'running',
            'finalizing', 'reading_pick'), 0)), 0),
        'can_ClearMetadata', COALESCE((@parent.item = 'candidate' AND
            @parent.actionable AND NOT (st.import_step IS NOT NULL) AND @parent.tab =
            'pending' AND NOT COALESCE(st.identification IN ('queued', 'running',
            'finalizing', 'reading_pick'), 0)), 0),
        'can_Combine', COALESCE((@parent.item = 'candidate' AND NOT (@parent.tab =
            'done' OR st.import_step IS NOT NULL OR COALESCE(st.identification IN
            ('queued', 'running', 'finalizing', 'reading_pick'), 0)) AND NOT
            @parent.separable AND @parent.actionable), 0),
        'can_Separate', COALESCE((@parent.item = 'candidate' AND NOT (@parent.tab =
            'done' OR st.import_step IS NOT NULL OR COALESCE(st.identification IN
            ('queued', 'running', 'finalizing', 'reading_pick'), 0)) AND
            @parent.separable), 0),
        'can_Skip', COALESCE((@parent.item = 'candidate' AND @parent.actionable AND NOT
            (st.import_step IS NOT NULL) AND @parent.tab = 'pending' AND NOT
            @parent.import_failed), 0),
        'can_Restore', COALESCE((@parent.item = 'candidate' AND @parent.actionable AND
            NOT (st.import_step IS NOT NULL) AND @parent.tab = 'skipped'), 0),
        'can_RevealFolder', COALESCE((@parent.item = 'candidate'), 0)) AS actions
    FROM (SELECT 1) AS one
    LEFT JOIN @crate::tables::import_state::listed_fragments::ListedStanding AS st
        ON st.standing_candidate = @parent.key
"#
);

// What the row of a release shows past its placement, read by the release's
// key from each table that holds a part of it: its scan, why a grouping
// holds it back, its last import's failure and what is running for it, and
// then either, for a release this device imported, the library release's
// album, cover, catalog records and upload, or its draft's provenance, link
// and chosen cover with the versions of the files the cover reads. A part
// whose row is missing is JSON null.
sqlive_compose::fragment!(
    ListedRow on import_list_placement { listed_row: String },
    r#"
    SELECT json_object(
        'name', scan.listed_name,
        'block', json_array(block.blocked, block.blocked_subject, block.blocked_holder),
        'failure', failure.failed_error,
        'facts', json_array(st.identification, st.import_asked, st.save_failure_error,
            st.import_step),
        'invalid', iif(scan.listed_invalid_reason IS NULL, NULL, json_object(
            'folder', hex(scan.listed_folder),
            'separable', scan.listed_grouping_key IS NOT NULL,
            'reason', scan.listed_invalid_reason,
            'shared_name', scan.listed_shared_name,
            'damaged_audio', scan.listed_damaged_audio,
            'tags', json(scan.listed_invalid_tags),
            'cover', json(scan.listed_invalid_cover))),
        'imported', iif(origin.origin_release_id IS NULL, NULL, json_object(
            'release_id', origin.origin_release_id,
            'album_id', released.album_id,
            'cover', library_cover.version,
            'records', json(library_records.records),
            'records_kind', library_records.records_kind,
            'upload', iif(storage.release_id IS NULL, NULL,
                json_array(storage.waiting_files, storage.sending_files)))),
        'provenance', iif(provenance.provenance_hash IS NULL, NULL, json_array(
            provenance.provenance_kind, provenance.provenance_source,
            provenance.provenance_release_id)),
        'link', iif(link.link_hash IS NULL, NULL, json_object(
            'content_hash', link.link_hash,
            'kind', link.link_kind,
            'source', link.link_source,
            'release_id', link.link_release_id,
            'partners', link.link_partners,
            'albums', link.link_albums,
            'records', link.link_records)),
        'cover', iif(cover.cover_hash IS NULL, NULL, json_object(
            'kind', cover.cover_kind,
            'file_id', cover.cover_file_id,
            'url', cover.cover_url,
            'source', cover.cover_source,
            'copies', cover.cover_copies,
            'local', json_array(hex(local.file_path), local.file_size, local.file_modified),
            'embedded_source', snapshot.snapshot_cover_source,
            'embedded', json_array(reading.tags_size, reading.tags_modified,
                hex(embedded.file_path))))) AS listed_row
    FROM (SELECT 1) AS one
    LEFT JOIN @crate::tables::folder_scans::listed_fragments::ListedScan AS scan
        ON scan.listed_root = @parent.watched_folder_path AND scan.listed_key = @parent.key
    LEFT JOIN @crate::tables::release_groupings::row_fragments::ListedBlock AS block
        ON block.block_key = scan.listed_grouping_key
    LEFT JOIN @crate::tables::import_state::listed_fragments::ListedState AS state
        ON state.state_hash = scan.listed_content_hash
       AND state.state_revision = scan.listed_revision
    LEFT JOIN @crate::tables::import_origin::fragments::ReleaseOrigin AS origin
        ON origin.origin_hash = scan.listed_content_hash
    LEFT JOIN @bae_records::records::fragments::ReleaseAlbum AS released
        ON released.release_id = origin.origin_release_id
    LEFT JOIN @bae_records::records::files::CoverVersion AS library_cover
        ON library_cover.id = origin.origin_release_id
    LEFT JOIN @bae_records::records::fragments::ReleaseRecords AS library_records
        ON library_records.records_release_id = origin.origin_release_id
    LEFT JOIN @bae_records::release_storage::ReleaseStorage AS storage
        ON storage.release_id = origin.origin_release_id AND storage.queued_count > 0
    LEFT JOIN @crate::tables::import_state::listed_fragments::ListedFailure AS failure
        ON failure.failed_hash = scan.listed_content_hash
    LEFT JOIN @crate::tables::import_state::listed_fragments::ListedStanding AS st
        ON st.standing_candidate = @parent.key
    LEFT JOIN @crate::tables::import_state::listed_fragments::ListedProvenance AS provenance
        ON provenance.provenance_hash = state.state_hash AND origin.origin_release_id IS NULL
    LEFT JOIN @crate::tables::import_state::listed_fragments::ListedLink AS link
        ON link.link_hash = state.state_hash AND origin.origin_release_id IS NULL
    LEFT JOIN @crate::tables::import_state::listed_fragments::ListedCover AS cover
        ON cover.cover_hash = state.state_hash AND origin.origin_release_id IS NULL
    LEFT JOIN @crate::tables::folder_scans::listed_fragments::ListedFile AS local
        ON cover.cover_kind = 'local'
       AND local.file_root = scan.listed_root AND local.file_candidate = scan.listed_key
       AND local.file_name = cover.cover_file_id
    LEFT JOIN @crate::tables::folder_scans::listed_fragments::ListedSnapshot AS snapshot
        ON cover.cover_kind = 'embedded'
       AND snapshot.snapshot_root = scan.listed_root
       AND snapshot.snapshot_candidate = scan.listed_key
    LEFT JOIN @crate::tables::folder_scans::listed_fragments::ListedTagReading AS reading
        ON reading.tags_root = snapshot.snapshot_root
       AND reading.tags_candidate = snapshot.snapshot_candidate
       AND reading.tags_file = cover.cover_file_id
    LEFT JOIN @crate::tables::folder_scans::listed_fragments::ListedFile AS embedded
        ON embedded.file_root = snapshot.snapshot_root
       AND embedded.file_candidate = snapshot.snapshot_candidate
       AND embedded.file_name = cover.cover_file_id
"#
);

// Whether the list folds the group of the placement row shut: the group is one
// of those `folded` names, each a JSON array of its watched folder's path and
// its folder in hex, or, when `folds_only` is false, `folded` names the groups
// left open and the group is none of them.
sqlive_compose::fragment!(
    Folded(folds_only: bool, folded: String) on import_list_placement { folded: bool },
    r#"
    SELECT ((@parent.watched_folder_path, @parent.group_folder) IN (
                SELECT unhex(json_extract(named.value, '$[0]')),
                       unhex(json_extract(named.value, '$[1]'))
                FROM json_each(:folded) AS named)) = :folds_only AS folded
"#
);

// The releases the filters keep: Found's filter entry `filter`, when the list
// narrows to one, and the shown text matching the pattern of `filter_text`,
// when the list searches.
sqlive_compose::fragment!(
    KeptReleases(filter: Option<String>, filter_text: Option<String>),
    r#"
    SELECT kept.entry AS kept_entry, kept.watched_folder_path AS kept_root,
        kept.group_folder AS kept_group
    FROM import_list_placement AS kept
    WHERE kept.item <> 'group'
      AND (:filter IS NULL OR kept.pending_entry = :filter)
      AND (:filter_text IS NULL
           OR kept.filter_title GLOB bae_search_pattern(:filter_text)
           OR kept.filter_artists GLOB bae_search_pattern(:filter_text))
"#
);

// Whether the filters keep something of the placement row: a release they
// keep, and a group's header while its group holds a release they keep. With
// nothing narrowing the list, every row.
sqlive_compose::fragment!(
    Kept(filter: Option<String>, filter_text: Option<String>)
        on import_list_placement { kept: bool },
    r#"
    SELECT CASE
        WHEN :filter IS NULL AND :filter_text IS NULL THEN 1
        WHEN @parent.item = 'group' THEN EXISTS (
            SELECT 1
            FROM @crate::tables::import_list::row_fragments::KeptReleases(
                filter: :filter, filter_text: :filter_text) AS member
            WHERE member.kept_root = @parent.watched_folder_path
              AND member.kept_group = @parent.key)
        ELSE EXISTS (
            SELECT 1
            FROM @crate::tables::import_list::row_fragments::KeptReleases(
                filter: :filter, filter_text: :filter_text) AS own
            WHERE own.kept_entry = @parent.entry)
    END AS kept
"#
);

// The header the placement row shows, as a JSON object: its own for a header,
// its group's for a member, and JSON null for a release in no group. Its count is the group's
// members that the filter entry keeps, read from the group counts kept beside
// the placement, so a header walks none of its members; while the list
// searches, it counts the matching releases instead.
sqlive_compose::fragment!(
    ListedHeader(
        filter: Option<String>, filter_text: Option<String>, folds_only: bool, folded: String
    ) on import_list_placement { listed_header: String },
    r#"
    SELECT iif(header.key IS NULL, 'null', json_object(
        'root', hex(header.watched_folder_path),
        'folder', hex(header.key),
        'name', header.group_name,
        'folded', @crate::tables::import_list::row_fragments::Folded(
            folds_only: :folds_only, folded: :folded, parent: header),
        'entries', CASE WHEN :filter_text IS NULL
            THEN (SELECT COALESCE(SUM(counted.count ORDER BY counted.pending_entry), 0)
                  FROM import_list_group_count AS counted
                  WHERE counted.watched_folder_path = header.watched_folder_path
                    AND counted.group_folder = header.key
                    AND (:filter IS NULL OR counted.pending_entry = :filter))
            ELSE (SELECT count(*)
                  FROM @crate::tables::import_list::row_fragments::KeptReleases(
                      filter: :filter, filter_text: :filter_text) AS matching
                  WHERE matching.kept_root = header.watched_folder_path
                    AND matching.kept_group = header.key) END,
        'combinable', header.combinable)) AS listed_header
    FROM (SELECT 1) AS one
    LEFT JOIN import_list_placement AS header
        ON header.watched_folder_path = @parent.watched_folder_path
       AND header.item = 'group'
       AND header.key = CASE @parent.item WHEN 'group' THEN @parent.key
                                          ELSE @parent.group_folder END
"#
);
