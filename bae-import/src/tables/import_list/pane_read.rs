//! The candidate pane's reads. The pane reads two concerns, each one
//! statement: the head reads the candidate as the scan, the list and the
//! candidate's state hold it, and the draft reads what it files under, which
//! is the draft it holds or the release this device imported from it. The
//! pane's value takes fields of both. Each statement is a fragment over a
//! relation of candidate keys, which the pane reads at its one key and
//! automation over every listed candidate.

// The pane's head of each candidate of `keys` the list lists as one: no row
// for a key it lists as no candidate.
coven::fragment!(
    PaneHeadRows with keys(candidate_key BLOB),
    r#"
    SELECT listed.*, scan.*, grouping.*, files.*, standing.*, imported.*, state.*, evidence.*,
           cards.*
    FROM keys
    JOIN @crate::tables::import_list::fragments::Listed(keys: (SELECT * FROM keys)) AS listed
        ON listed.listed_key = keys.candidate_key
    LEFT JOIN @crate::tables::folder_scans::fragments::ScanItems(keys: (SELECT * FROM keys))
        AS scan ON scan.path = keys.candidate_key
    LEFT JOIN @crate::tables::release_groupings::fragments::GroupingOfCandidate(
        keys: (SELECT * FROM keys)) AS grouping ON grouping.grouping_of_key = keys.candidate_key
    LEFT JOIN @crate::tables::folder_scans::fragments::PaneFiles(keys: (SELECT * FROM keys))
        AS files ON files.pane_files_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::Standing(keys: (SELECT * FROM keys))
        AS standing ON standing.standing_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_origin::fragments::ImportedRelease(
        keys: (SELECT * FROM keys)) AS imported ON imported.imported_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::current_owner::CurrentOwner(
        keys: (SELECT * FROM keys)) AS owner ON owner.owner_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::state_fragments::CurrentState(
        owner: (SELECT owner.content_hash, owner.identify_run
            FROM @crate::tables::import_state::current_owner::CurrentOwner(
                keys: (SELECT * FROM keys)) AS owner))
        AS state ON state.owner_row = owner.content_hash
    LEFT JOIN @crate::tables::import_state::fragments::FileEvidence(keys: (SELECT * FROM keys))
        AS evidence ON evidence.file_evidence_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::card_fragments::VerdictCards(
        owner: (SELECT owner.content_hash, owner.identify_run
            FROM @crate::tables::import_state::current_owner::CurrentOwner(
                keys: (SELECT * FROM keys)) AS owner))
        AS cards ON cards.owner_row = owner.content_hash
"#);

// The pane's draft of each candidate of `keys`, one row for each key: what
// the candidate files under unless this device imported a release from it.
coven::fragment!(
    PaneDraftRows with keys(candidate_key BLOB) { pane_key: Option<crate::CandidateKey> },
    r#"
    SELECT keys.candidate_key AS pane_key, draft.*, flags.*, failure.*, pane_failure.*,
           mapping.*, artist_field.*, claimed.*, status.*, album.*, cover.*, previewed.*
    FROM keys
    LEFT JOIN @crate::tables::import_origin::fragments::ImportedRelease(
        keys: (SELECT * FROM keys)) AS imported ON imported.imported_key = keys.candidate_key
    LEFT JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scan
        ON scan.candidate_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::DraftRows(
        candidate: (SELECT scanned.content_hash
            FROM keys
            JOIN @crate::tables::folder_scans::row_fragments::ScanContent AS scanned
                ON scanned.candidate_key = keys.candidate_key))
        AS draft ON imported.imported_key IS NULL AND draft.draft_hash = scan.content_hash
    LEFT JOIN @crate::tables::import_state::fragments::DraftFlags(keys: (SELECT * FROM keys))
        AS flags ON imported.imported_key IS NULL AND flags.draft_flags_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::Failure(keys: (SELECT * FROM keys))
        AS failure ON imported.imported_key IS NULL AND failure.failure_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::PaneFailure(keys: (SELECT * FROM keys))
        AS pane_failure ON imported.imported_key IS NULL
        AND pane_failure.pane_failure_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_list::fragments::Mapping(keys: (SELECT * FROM keys))
        AS mapping ON imported.imported_key IS NULL AND mapping.mapping_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_list::fragments::ArtistField(keys: (SELECT * FROM keys))
        AS artist_field ON imported.imported_key IS NULL
        AND artist_field.artist_field_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::ClaimedRecords(keys: (SELECT * FROM keys))
        AS claimed ON imported.imported_key IS NULL AND claimed.claimed_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::PickedLibraryStatus(
        keys: (SELECT * FROM keys)) AS status ON imported.imported_key IS NULL
        AND status.picked_status_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::CandidateAlbum(keys: (SELECT * FROM keys))
        AS album ON imported.imported_key IS NULL
        AND album.candidate_album_key = keys.candidate_key
    LEFT JOIN @crate::tables::import_state::fragments::CoverChoice(keys: (SELECT * FROM keys))
        AS cover ON imported.imported_key IS NULL AND cover.cover_choice_key = keys.candidate_key
    LEFT JOIN @bae_playback::tables::now::PreviewedWindow AS previewed
        ON imported.imported_key IS NULL
"#);

coven::read!(
    writer PaneHead(key: Option<crate::CandidateKey>),
    r#"
    SELECT head.*
    FROM @crate::tables::import_list::pane_read::PaneHeadRows(
        keys: (SELECT :key AS candidate_key)) AS head
"#
);

coven::read!(
    writer PaneDraft(key: Option<crate::CandidateKey>),
    r#"
    SELECT draft.*
    FROM @crate::tables::import_list::pane_read::PaneDraftRows(
        keys: (SELECT :key AS candidate_key)) AS draft
"#
);
