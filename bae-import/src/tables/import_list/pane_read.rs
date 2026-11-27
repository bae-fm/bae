//! The candidate pane's reads. SQLite refuses to prepare the pane as one
//! statement, which nests every fragment's subqueries too deep, so it reads
//! its two concerns in one snapshot: the candidate as the scan, the list and
//! the candidate's state hold it, and what it files under, which is the draft
//! it holds or the release this device imported from it.

sqlive_compose::read!(
    PaneHead(key: Option<crate::CandidateKey>),
    r#"
    SELECT listed.*, scan.*, grouping.*, files.*, standing.*, imported.*, state.*, evidence.*,
           cards.*
    FROM (SELECT :key AS key) AS pane
    JOIN @crate::tables::import_list::fragments::Listed(key: :key) AS listed ON 1
    LEFT JOIN @crate::tables::folder_scans::fragments::ScanItems(
        keys: (SELECT :key AS candidate_key)) AS scan ON 1
    LEFT JOIN @crate::tables::release_groupings::fragments::GroupingOfCandidate(key: :key)
        AS grouping ON 1
    LEFT JOIN @crate::tables::folder_scans::fragments::PaneFiles(key: :key) AS files ON 1
    LEFT JOIN @crate::tables::import_state::fragments::Standing(
        keys: (SELECT :key AS candidate_key)) AS standing ON 1
    LEFT JOIN @crate::tables::import_origin::fragments::ImportedRelease(key: :key)
        AS imported ON 1
    LEFT JOIN @crate::tables::import_state::state_fragments::CurrentState(
        owner: @crate::tables::import_state::current_owner::CurrentOwner(key: :key))
        AS state ON 1
    LEFT JOIN @crate::tables::import_state::fragments::FileEvidence(key: :key) AS evidence ON 1
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictCards(
        owner: @crate::tables::import_state::current_owner::CurrentOwner(key: :key))
        AS cards ON 1
"#
);

sqlive_compose::read!(
    PaneDraft(key: Option<crate::CandidateKey>) { pane_key: Option<crate::CandidateKey> },
    r#"
    SELECT pane.key AS pane_key, draft.*, flags.*, failure.*, pane_failure.*, mapping.*,
           artist_field.*, claimed.*, status.*, album.*, cover.*, previewed.*
    FROM (SELECT :key AS key) AS pane
    LEFT JOIN @crate::tables::import_origin::fragments::ImportedRelease(key: :key)
        AS imported ON 1
    LEFT JOIN @crate::tables::import_state::fragments::DraftRows(
        candidate: (SELECT scan.content_hash
                    FROM @crate::tables::folder_scans::row_fragments::ScanContent(key: :key) AS scan))
        AS draft ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::DraftFlags(key: :key) AS flags
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::Failure(key: :key) AS failure
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::PaneFailure(key: :key) AS pane_failure
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_list::fragments::Mapping(key: :key) AS mapping
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_list::fragments::ArtistField(key: :key) AS artist_field
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::ClaimedRecords(key: :key) AS claimed
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::PickedLibraryStatus(key: :key) AS status
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::CandidateAlbum(key: :key) AS album
        ON imported.imported_key IS NULL
    LEFT JOIN @crate::tables::import_state::fragments::CoverChoice(key: :key) AS cover
        ON imported.imported_key IS NULL
    LEFT JOIN @bae_playback::tables::now::PreviewedWindow AS previewed
        ON imported.imported_key IS NULL
"#);
