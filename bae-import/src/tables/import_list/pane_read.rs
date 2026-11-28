//! The candidate pane's reads. The pane is one concern: one release, read for
//! one request. SQLite refuses to prepare it as one statement, which nests
//! every fragment's subqueries too deep, so one live query runs two
//! statements in one snapshot. The head reads the candidate as the scan, the
//! list and the candidate's state hold it, and the draft reads what it files
//! under, which is the draft it holds or the release this device imported from
//! it. The pane's value takes fields of both.

coven::read!(
    writer PaneHead(key: Option<crate::CandidateKey>),
    r#"
    SELECT listed.*, scan.*, grouping.*, files.*, standing.*, imported.*, state.*, evidence.*,
           cards.*
    FROM (SELECT :key AS key) AS pane
    JOIN @crate::tables::import_list::fragments::Listed(keys: (SELECT :key AS candidate_key)) AS listed ON 1
    LEFT JOIN @crate::tables::folder_scans::fragments::ScanItems(
        keys: (SELECT :key AS candidate_key)) AS scan ON 1
    LEFT JOIN @crate::tables::release_groupings::fragments::GroupingOfCandidate(keys: (SELECT :key AS candidate_key))
        AS grouping ON 1
    LEFT JOIN @crate::tables::folder_scans::fragments::PaneFiles(
        keys: (SELECT :key AS candidate_key)) AS files ON 1
    LEFT JOIN @crate::tables::import_state::fragments::Standing(
        keys: (SELECT :key AS candidate_key)) AS standing ON 1
    LEFT JOIN @crate::tables::import_origin::fragments::ImportedRelease(keys: (SELECT :key AS candidate_key))
        AS imported ON 1
    LEFT JOIN @crate::tables::import_state::state_fragments::CurrentState(
        owner: (SELECT owner.content_hash, owner.identify_run
            FROM @crate::tables::import_state::current_owner::CurrentOwner(keys: (SELECT :key AS candidate_key)) AS owner))
        AS state ON 1
    LEFT JOIN @crate::tables::import_state::fragments::FileEvidence(keys: (SELECT :key AS candidate_key)) AS evidence ON 1
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictCards(
        owner: (SELECT owner.content_hash, owner.identify_run
            FROM @crate::tables::import_state::current_owner::CurrentOwner(keys: (SELECT :key AS candidate_key)) AS owner))
        AS cards ON 1
"#
);

coven::read!(
    writer PaneDraft(key: Option<crate::CandidateKey>) { pane_key: Option<crate::CandidateKey> },
    r#"
    SELECT pane.key AS pane_key, draft.*, flags.*, failure.*, pane_failure.*, mapping.*,
           artist_field.*, claimed.*, status.*, album.*, cover.*, previewed.*
    FROM (SELECT :key AS key) AS pane
    LEFT JOIN @crate::tables::import_origin::fragments::ImportedRelease(keys: (SELECT :key AS candidate_key))
        AS imported ON 1
    LEFT JOIN @crate::tables::import_state::fragments::DraftRows(
        candidate: (SELECT scan.content_hash
                    FROM @crate::tables::folder_scans::row_fragments::ScanContent AS scan WHERE scan.candidate_key = :key))
        AS draft ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::DraftFlags(keys: (SELECT :key AS candidate_key)) AS flags
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::Failure(keys: (SELECT :key AS candidate_key)) AS failure
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::PaneFailure(keys: (SELECT :key AS candidate_key)) AS pane_failure
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_list::fragments::Mapping(keys: (SELECT :key AS candidate_key)) AS mapping
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_list::fragments::ArtistField(keys: (SELECT :key AS candidate_key)) AS artist_field
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::ClaimedRecords(keys: (SELECT :key AS candidate_key)) AS claimed
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::PickedLibraryStatus(keys: (SELECT :key AS candidate_key)) AS status
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::CandidateAlbum(keys: (SELECT :key AS candidate_key)) AS album
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::CoverChoice(keys: (SELECT :key AS candidate_key)) AS cover
        ON imported.imported_key IS NULL
    LEFT JOIN @bae_playback::tables::now::PreviewedWindow AS previewed
        ON imported.imported_key IS NULL
"#);
