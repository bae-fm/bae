/// Candidates hashing the same are one job, and the answer one of them stores
/// answers all of them: the next admission finds both settled and asks nothing.
#[tokio::test(flavor = "multi_thread")]
async fn one_answer_covers_every_candidate_that_hashes_the_same() {
    let fixture = Fixture::new("shared-hash-answered").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    assert_eq!(
        fixture.content_hash(&first),
        fixture.content_hash(&second),
        "the two folders hold the same bytes"
    );
    let probed = fixture.probed_total_ms(&first);
    fixture.provider.route(
        "/discid/",
        200,
        discid_json("mb-shared-1", "rg-shared-1", &[probed, 0]),
    );
    fixture.provider.route(
        "/release/mb-shared-1?",
        200,
        release_json("mb-shared-1", "rg-shared-1", &[probed, 0]),
    );
    fixture.scan(2).await;

    fixture.drain_automatic().await;

    let asked = fixture.provider.count_containing("/discid/");
    assert_eq!(
        asked,
        1,
        "one run answered both: {:?}",
        fixture.provider.requests()
    );
    assert!(fixture.identified_for(&first).await.is_some());
    assert!(fixture.identified_for(&second).await.is_some());

    fixture.drain_automatic().await;

    assert_eq!(
        fixture.provider.count_containing("/discid/"),
        asked,
        "and a second admission finds both of them answered"
    );
}

// ── Synthetic candidates, for the pure planning tests ───────────────────────

fn synthetic_candidate(path: &str, size: u64) -> FolderCandidate {
    use crate::import::folder_scanner::{CandidateFile, CategorizedFiles, FileRole, ScannedFile};
    FolderCandidate {
        path: PathBuf::from(path),
        file_root: PathBuf::from(path),
        name: path.trim_start_matches('/').to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                file: ScannedFile::new(
                    PathBuf::from(format!("{path}/01.flac")),
                    "01.flac".to_string(),
                    size,
                    1,
                )
                .with_test_flac_audio(),
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        watched_folder_path: "/".to_string(),
        scope: crate::import::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: path.trim_start_matches('/').to_string(),
        grouping: None,
    }
}

// ── 10. Lookup reuses a stored verdict ──────────────────────────────────────

/// A several-match verdict, as identification stores one: the pressing is the
/// open question, so no match carries a settled tracklist.
fn multi_match_verdict(release_ids: &[&str], group_id: &str) -> TerminalVerdict {
    TerminalVerdict::Found {
        findings: crate::identify::Findings {
            matches: release_ids
                .iter()
                .map(|release_id| MetadataResult {
                    source: crate::catalog::Catalog::MusicBrainz,
                    release_id: release_id.to_string(),
                    title: "Album".to_string(),
                    artist: Some("Artist".to_string()),
                    year: None,
                    labels: Vec::new(),
                    area: None,
                    status: None,
                    packaging: None,
                    discogs_details: Vec::new(),
                    barcodes: Vec::new(),
                    media: bae_types::pressing::StatedMedia::Undescribed,
                    links: Vec::new(),
                    cover_art: None,
                    source_group_id: Some(group_id.to_string()),
                    album_links: crate::import::album_links::AlbumLinks::NotAsked,
                    source_tracks: None,
                    document_failure: None,
                    album_first_year: None,
                    track_titles: Vec::new(),
                    notes: Vec::new(),
                })
                .collect(),
            provenance: release_ids
                .iter()
                .map(|_| crate::identify::LookupProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                    by_isrc: false,
                    by_search: false,
                    by_pressing: false,
                })
                .collect(),
            // Each release is a pressing of its own: the group lists several, and
            // which one the folder is, is the open question.
            pressings: (0..release_ids.len() as u32).collect(),
            narrowed_out: crate::identify::NarrowedOut::default(),
            medium_conflict: None,
            named_notes: Vec::new(),
        },
        track_count: 2,
        ledger: None,
    }
}

/// A verdict is refused for a candidate an import has claimed. The claim and
/// the check share the folder-state commit lock, so by the time a claim
/// returns there is no interval left in which a verdict can be stored for a
/// candidate the user has already committed to importing — and a verdict that
/// did land would describe files the import is in the middle of consuming.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_is_refused_for_a_claimed_candidate() {
    let fixture = Fixture::new("verdict-refused-when-claimed").await;
    let dir = fixture.disc_id_candidate("Album");
    fixture.scan(1).await;
    let key = dir.to_string_lossy().into_owned();
    let row = || NewImportCandidateVerdict {
        content_hash: fixture.content_hash(&dir),
        file_edit_revision: 0,
        folder_path: key.clone(),
        verdict: multi_match_verdict(&["mb-claimed-1"], "rg-claimed-1"),
        signals: settled_signals(),
        pick: None,
    };

    assert!(
        matches!(
            fixture
                .import
                .save_candidate_verdict_if_current(&key, IdentifyRunId::for_test(1), &row())
                .await
                .unwrap(),
            crate::import::VerdictWrite::Stored { .. }
        ),
        "an unclaimed candidate still takes its verdict"
    );

    fixture.import.claim_candidate_for_import(&key, "import-1").await;

    assert_eq!(
        fixture
            .import
            .save_candidate_verdict_if_current(&key, IdentifyRunId::for_test(1), &row())
            .await
            .unwrap(),
        crate::import::VerdictWrite::Refused,
        "a claimed candidate refuses a verdict"
    );
}

// ── 11. Re-stating a file decision changes nothing ──────────────────────────

/// The disc menu and the role picker fire on every selection, including of
/// the item already in force. A decision that re-states what is already true
/// writes nothing — above all it does not clear the stored verdict, which
/// would re-identify a folder whose shape did not change and blank the pane
/// over it.
#[tokio::test(flavor = "multi_thread")]
async fn restating_a_file_decision_changes_nothing() {
    let fixture = Fixture::new("edit-noop").await;
    let dir = fixture.seed_cue_album("Album");
    fixture.scan(1).await;
    fixture.archive("mb-noop-1", "rg-noop-1", &[500, 500]).await;
    fixture
        .store_settled_verdict(&dir, "mb-noop-1", "rg-noop-1",)
        .await;
    let key = dir.to_string_lossy().into_owned();
    let mut events = fixture.import.every_event();

    // The sheet already carves disc one and already binds its own container.
    fixture
        .import
        .set_sheet_disc(
            key.clone(),
            "Test Album.cue".to_string(),
            crate::import::folder_scanner::SheetDisc::Disc { number: 1 },
        )
        .await
        .unwrap();
    fixture
        .import
        .set_sheet_binding(
            key.clone(),
            "Test Album.cue".to_string(),
            "Test Album.flac".to_string(),
            Some("Test Album.flac".to_string()),
        )
        .await
        .unwrap();

    let row = fixture
        .stored_for(&dir)
        .await
        .expect("the candidate's row remains");
    assert!(
        row.identify.is_some(),
        "a re-stated decision must not clear the stored verdict"
    );
    assert!(
        !drain_events(&mut events).iter().any(|event| matches!(
            event,
            ImportEvent::Scan(ScanEvent::CandidateBindingChanged { .. })
        )),
        "and must not announce a changed candidate"
    );

    // A genuinely different decision still lands and still clears.
    fixture
        .import
        .set_sheet_disc(
            key,
            "Test Album.cue".to_string(),
            crate::import::folder_scanner::SheetDisc::Ignored,
        )
        .await
        .unwrap();
    assert!(
        fixture
            .stored_for(&dir)
            .await
            .is_none_or(|row| row.identify.is_none()),
        "a real change clears the verdict as before"
    );
}
