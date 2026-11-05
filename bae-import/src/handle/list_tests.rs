//! In Progress follows where each candidate stands: a row shows while its run
//! or import is queued or going, and leaves when it ends — and leaves the
//! entry the tables put it under for as long. Every snapshot counts the
//! filter's entries with each row's standing, and a change that moves a count
//! off the page delivers one. A stored pick reads back on its row as the
//! answer and the identity it committed, leading with the picked release and
//! stating what each claimed source says.

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use crate::{ImportListView, LiveImportList, PendingFilter};
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_mb_release_with_catalog;
use bae_catalogs::test_releases::{discogs_release_json, mb_release_json, mb_titled_release_json};
use bae_util::test_channels::HANG_GUARD;
use tempfile::TempDir;

/// The list under one filter entry, and the keys and filter entries its last
/// snapshot carried.
struct LiveList {
    list: LiveImportList,
    shown: Option<Vec<crate::CandidateKey>>,
    entries: Vec<crate::PendingFilterEntry>,
}

impl LiveList {
    async fn new(handle: &ImportServiceHandle, filter: PendingFilter) -> Self {
        Self {
            list: handle
                .subscribe_whole_list(ImportListView {
                    pending_filter: filter,
                    ..ImportListView::default()
                })
                .await,
            shown: None,
            entries: Vec::new(),
        }
    }

    /// Wait until the list shows exactly `expected`; at once when it already
    /// does, since a change that moves no row delivers nothing.
    async fn shows(&mut self, expected: &[&crate::CandidateKey]) {
        let wanted: Vec<crate::CandidateKey> = expected.iter().map(|key| (*key).clone()).collect();
        let reached = tokio::time::timeout(HANG_GUARD, async {
            while self.shown.as_ref() != Some(&wanted) {
                self.next().await;
            }
        })
        .await;
        assert!(reached.is_ok(), "the list shows {:?}, not {wanted:?}", self.shown);
    }

    /// How many rows the last snapshot counts under `filter`; none before
    /// the first.
    fn counted(&self, filter: PendingFilter) -> Option<u32> {
        counted(&self.entries, filter)
    }

    /// Wait until a snapshot counts `count` rows under `filter`; at once when
    /// the last one does.
    async fn counts(&mut self, filter: PendingFilter, count: u32) {
        let reached = tokio::time::timeout(HANG_GUARD, async {
            while self.counted(filter) != Some(count) {
                self.next().await;
            }
        })
        .await;
        assert!(
            reached.is_ok(),
            "the list counts {:?} under {filter:?}, not {count}",
            self.counted(filter)
        );
    }

    async fn next(&mut self) {
        let snapshot = self.list.next().await.expect("the list answers");
        self.entries = snapshot.pending_filter_entries.clone();
        self.shown = Some(
            snapshot
                .windows
                .iter()
                .flat_map(|window| &window.items)
                .filter_map(|item| match item {
                    crate::ImportListItem::Candidate { row, .. } => Some(row.candidate_key.clone()),
                    _ => None,
                })
                .collect(),
        );
    }
}

/// An import shows under In Progress while it waits for the worker and while
/// it runs, and leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn importing_shows_a_queued_and_a_running_import_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::InProgress).await;
    list.shows(&[]).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.shows(&[&key]).await;

    handle
        .runtime
        .move_import(
            &key,
            "import-1",
            Some(crate::ImportStep::Running(crate::ImportPhase::ReadingFiles)),
        )
        .await
        .unwrap();
    list.shows(&[&key]).await;

    handle.end_candidate_import_for_test(&key, "import-1").await;
    list.shows(&[]).await;
    shut_down(handle).await;
}

/// A run shows under In Progress while it is queued and while it runs, and
/// leaves when it ends.
#[tokio::test(flavor = "multi_thread")]
async fn identifying_shows_a_queued_and_a_running_run_until_it_ends() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::InProgress).await;
    list.shows(&[]).await;

    admit(&handle, &key).await;
    list.shows(&[&key]).await;

    start_run(&handle, &key, crate::IdentifyRunId::for_test(1)).await;
    list.shows(&[&key]).await;

    handle.cancel_identification(&key).await.unwrap();
    list.shows(&[]).await;
    shut_down(handle).await;
}

/// A row an import owns is In Progress, not under the entry the tables put
/// it under: it leaves that entry while the import goes and is back once it
/// ends, so no row is ever under two entries past All.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_leaves_its_stored_entry_while_an_import_owns_it() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let stored = handle
        .load_candidate(&key)
        .await
        .unwrap()
        .expect("the candidate reads back")
        .live
        .standing
        .expect("the candidate is on Found")
        .state();
    // The entry past All Found's menu holds the state under.
    let entry = match stored {
        crate::PendingState::NeedsYou
        | crate::PendingState::LookupError
        | crate::PendingState::Error
        | crate::PendingState::ImportError => PendingFilter::NeedsYou,
        crate::PendingState::Identifying | crate::PendingState::Importing => {
            PendingFilter::InProgress
        }
        crate::PendingState::Identified => PendingFilter::Identified,
        crate::PendingState::Unmatched => PendingFilter::Unmatched,
        crate::PendingState::NotLookedUp => PendingFilter::NotLookedUp,
    };
    assert_ne!(entry, PendingFilter::InProgress);
    let mut list = LiveList::new(&handle, entry).await;
    list.shows(&[&key]).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.shows(&[]).await;

    handle.end_candidate_import_for_test(&key, "import-1").await;
    list.shows(&[&key]).await;
    shut_down(handle).await;
}

/// The filter's counts follow what is running: a claimed import moves its
/// row's count from the entry the tables put it under to In
/// Progress, and back once the import ends, with the list under All showing
/// every row throughout.
#[tokio::test(flavor = "multi_thread")]
async fn the_entry_counts_follow_what_is_running() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let mut list = LiveList::new(&handle, PendingFilter::All).await;
    list.shows(&[&key]).await;
    list.counts(PendingFilter::All, 1).await;
    list.counts(PendingFilter::InProgress, 0).await;

    handle.claim_candidate_for_import(&key, "import-1").await;
    list.counts(PendingFilter::InProgress, 1).await;
    list.counts(PendingFilter::All, 1).await;
    list.shows(&[&key]).await;

    handle.end_candidate_import_for_test(&key, "import-1").await;
    list.counts(PendingFilter::InProgress, 0).await;
    list.counts(PendingFilter::All, 1).await;
    shut_down(handle).await;
}

/// A watched root holding two stored candidates, each with its draft read
/// from its own tags, and the list under All through a window that holds the
/// first item alone: the key on that page, and the key off it.
async fn one_row_on_the_page(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> (ImportServiceHandle, TempDir, LiveImportList, crate::CandidateKey, crate::CandidateKey) {
    let (parts, database, tmp) = setup_test_import_with(providers, http).await;
    let tables = crate::tables::ImportTables::new(database);
    let (first, first_key, _) = unscanned_candidate(&tables, &tmp, "Album").await;
    let (second, second_key, _) = unscanned_candidate(&tables, &tmp, "Other Record").await;
    let root = first.watched_folder_path.clone();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    for candidate in [first, second] {
        crate::store_scan_item(
            &tables,
            &bae_diagnostics::Diagnostics::noop(),
            &root,
            generation,
            &crate::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap();
    }
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    let handle = parts.start().await.unwrap();
    for key in [&first_key, &second_key] {
        handle.select_candidate_file_tags(key.clone()).await.unwrap();
    }
    let list = handle
        .subscribe_list_window(
            ImportListView::default(),
            bae_db::LibraryPageWindow { offset: 0, limit: 1 },
        )
        .await;
    let snapshot = delivered(&list).await;
    let on_page: Vec<crate::CandidateKey> = snapshot
        .windows
        .iter()
        .flat_map(|window| &window.items)
        .filter_map(candidate_key)
        .cloned()
        .collect();
    assert_eq!(on_page.len(), 1, "the page holds one row: {on_page:?}");
    let entries = &snapshot.pending_filter_entries;
    assert_eq!(counted(entries, PendingFilter::All), Some(2));
    assert_eq!(counted(entries, PendingFilter::InProgress), Some(0));
    assert_eq!(counted(entries, PendingFilter::Identified), Some(0));
    let on_page = on_page.into_iter().next().expect("one row");
    let off_page = if on_page == first_key { second_key } else { first_key };
    (handle, tmp, list, on_page, off_page)
}

/// The candidate whose row `item` is.
fn candidate_key(item: &crate::ImportListItem) -> Option<&crate::CandidateKey> {
    match item {
        crate::ImportListItem::Candidate { row, .. } => Some(&row.candidate_key),
        crate::ImportListItem::GroupHeader { .. } | crate::ImportListItem::Invalid { .. } => None,
    }
}

/// The next snapshot the list delivers.
async fn delivered(list: &LiveImportList) -> crate::ImportListSnapshot {
    tokio::time::timeout(HANG_GUARD, list.next())
        .await
        .expect("the list delivers")
        .expect("the list answers")
}

/// How many rows `entries` count under `filter`; `None` when they hold no
/// entry for it, as before the list's first snapshot.
fn counted(entries: &[crate::PendingFilterEntry], filter: PendingFilter) -> Option<u32> {
    entries.iter().find(|entry| entry.filter == filter).map(|entry| entry.count)
}

/// A run starting for a row off the page, under All, changes nothing on the
/// page but moves the row's count to In Progress, so the list delivers it.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_off_the_page_delivers_its_count() {
    let (handle, _tmp, list, on_page, off_page) = one_row_on_the_page(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;

    admit(&handle, &off_page).await;
    let moved = delivered(&list).await;

    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::InProgress), Some(1));
    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::All), Some(2));
    assert_eq!(
        moved
            .windows
            .iter()
            .flat_map(|window| &window.items)
            .filter_map(candidate_key)
            .collect::<Vec<_>>(),
        vec![&on_page],
        "the page holds the same row"
    );
    shut_down(handle).await;
}

/// A pick for a row off the page moves its count to Identified, so the list
/// delivers it.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_off_the_page_delivers_its_count() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, list, _on_page, off_page) =
        one_row_on_the_page(fakes.providers(), bae_util::http::Http::for_test()).await;
    seed_mb_release_with_catalog(&fakes, "off-the-page", "CAT-1");

    pick(&handle, &off_page, "off-the-page").await;
    // The pick's write and its cancelling the row's run each deliver; while
    // the run still goes it counts the row under In Progress.
    let moved = loop {
        let snapshot = delivered(&list).await;
        if counted(&snapshot.pending_filter_entries, PendingFilter::Identified) == Some(1) {
            break snapshot;
        }
    };

    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::Identified), Some(1));
    assert_eq!(counted(&moved.pending_filter_entries, PendingFilter::All), Some(2));
    shut_down(handle).await;
}

/// Deciding an identity persists it and the pane reads it back — the whole of
/// "resume" — with the provider gone. A settled single match wrote the same
/// record, so an auto-importable candidate reads identically.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_reads_back_as_the_same_answer() {
    let fixture = Fixture::new("pick-answer").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);

    // The settled verdict wrote the pick; nothing is routed, so everything
    // below is served from what identification archived.
    fixture.archive("mb-answer-1", "rg-answer-1", &[probed, 0]).await;
    fixture.store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1").await;

    let resumed = fixture.pane(&dir).await.expect("the candidate reads back");
    let link = resumed
        .draft()
        .unwrap()
        .release_link
        .as_ref()
        .and_then(crate::ReleaseLink::pressing)
        .expect("a settled single match is a decision");
    assert_eq!(link.record.key, "mb-answer-1");
    assert_eq!(resumed.draft().unwrap().metadata_draft.tracks.len(), 2);
    assert_eq!(
        resumed.file_evidence,
        vec![crate::FileEvidence {
            signal: crate::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }],
        "the chip says which signal turned the release up, on the file it came from"
    );

    assert_eq!(
        resumed.draft().unwrap().metadata_author,
        crate::MetadataAuthor::Identification,
        "the pane's own value says identification wrote the pick"
    );

    // The row carries the same decision for the sidebar's resume trigger.
    let picked =
        queue_row(&fixture, &key).await.release_link.expect("the row carries the decision");
    assert_eq!(
        picked,
        crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-answer-1".to_string()
            ),
            partners: vec![],
        })
    );

    // A person reading the draft from the folder's own files replaces the
    // draft, not the link: the pane still shows the linked release.
    fixture
        .import
        .select_candidate_file_tags(key.clone())
        .await
        .expect("deciding file metadata succeeds");
    let resumed = fixture.pane(&dir).await.expect("the candidate reads back");
    assert_eq!(
        resumed.draft().unwrap().release_link,
        Some(picked),
        "the candidate stays linked to the release identification picked"
    );
    assert_eq!(
        resumed.draft().unwrap().metadata_provenance,
        Some(crate::MetadataProvenance::FileMetadata)
    );
    assert_eq!(
        resumed.draft().unwrap().metadata_author,
        crate::MetadataAuthor::Person,
        "and the person is now the author of the draft"
    );
    assert!(
        !resumed.draft().unwrap().metadata_draft.is_blank(),
        "and still draws a form, seeded from the folder's own tags"
    );
    assert_eq!(
        resumed.file_evidence,
        vec![crate::FileEvidence {
            signal: crate::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }],
        "the extracted Disc ID still names its source file with the draft re-read"
    );
    assert!(
        fixture.provider.requests().is_empty(),
        "every answer came from the archive: {:?}",
        fixture.provider.requests()
    );
}

/// The sidebar row leads with the identity the candidate is settled on. A
/// manual search settles it on a release identification never named, and the
/// pick is the only record of that — a row reading the stored verdict alone
/// goes on showing the folder name and a placeholder while the pane shows the
/// release, with nothing to move it off.
#[tokio::test(flavor = "multi_thread")]
async fn a_picked_release_is_what_the_row_leads_with() {
    let fixture = Fixture::new("pick-row").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);

    // Identification settled on one release; the user searched and picked
    // another, whose documents the search archived.
    fixture.archive("mb-answer-1", "rg-answer-1", &[probed, 0]).await;
    fixture.store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1").await;
    // The picked release is one identification never fetched, which is what a
    // manual search result is: its documents are archived by the pick itself.
    fixture.provider.route(
        "/release/mb-picked-1?",
        200,
        mb_titled_release_json(
            "mb-picked-1",
            "rg-picked-1",
            "Picked Album Title",
            "Picked Artist Name",
        ),
    );

    // Read the queue on the event the surfaces refresh on, not after the pick
    // has finished settling: the row has to be right the moment it lands.
    let mut events = fixture.import.every_event();
    let picking = {
        let import = fixture.import.clone();
        let key = key.clone();
        tokio::spawn(async move {
            import
                .select_candidate_release(
                    key,
                    bae_identify::PressingLink {
                        record: bae_catalogs::records::LookupRef::new(
                            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                            "mb-picked-1".to_string(),
                        ),
                        partners: vec![],
                    },
                    crate::CatalogApplySteps::unread(),
                )
                .await
        })
    };
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let event = events.recv().await.expect("the pick raises an event");
            if matches!(
                &event,
                crate::ImportEvent::Scan(crate::ScanEvent::CandidateMetadataChanged {
                    candidate_key,
                }) if *candidate_key == key
            ) {
                break;
            }
        }
    })
    .await
    .expect("a successful pick publishes its metadata change");

    picking.await.expect("the pick task runs").expect("picking the searched release succeeds");
    assert_eq!(
        fixture.pane(&dir).await.expect("the candidate reads back").file_evidence,
        vec![crate::FileEvidence {
            signal: crate::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }],
        "a manual pick does not erase the candidate's extracted signal source"
    );
    let row = queue_row(&fixture, &key).await;
    assert_eq!(
        row.release_link,
        Some(crate::ReleaseLink::Pressing(bae_identify::PressingLink {
            record: bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "mb-picked-1".to_string(),
            ),
            partners: vec![],
        })),
        "the row is linked to the release the pick settled it on"
    );
    let summary = row.metadata_summary.expect("the row shows the draft the pick read");
    assert_eq!(summary.album_title, "Picked Album Title");
    assert_eq!(summary.album_artists, "Picked Artist Name");
}

/// A pick is written with the candidate, so the answer, the row's resume
/// record and the identity a bulk import would commit all come back naming the
/// same pressing after a restart — while the evidence keeps saying what
/// identified it, here a disc ID that matched that one release.
#[tokio::test(flavor = "multi_thread")]
async fn a_pick_reads_back_as_the_identity_it_commits() {
    let fixture = Fixture::new("pick-reads-back").await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);

    fixture.archive("mb-answer-1", "rg-answer-1", &[probed, 0]).await;
    fixture.store_settled_verdict(&dir, "mb-answer-1", "rg-answer-1").await;

    let pick = bae_identify::PressingLink {
        record: bae_catalogs::records::LookupRef::new(
            bae_catalogs::catalog::LookupCatalog::MusicBrainz,
            "mb-answer-1".to_string(),
        ),
        partners: vec![],
    };
    fixture
        .import
        .select_candidate_release(key.clone(), pick.clone(), crate::CatalogApplySteps::unread())
        .await
        .expect("picking the release succeeds");

    // Extracted signal provenance belongs to the candidate files, so picking a
    // release does not change it.
    let pane = fixture.pane(&dir).await.expect("the candidate reads back");
    assert_eq!(
        pane.file_evidence,
        vec![crate::FileEvidence {
            signal: crate::EvidenceSignal::DiscId,
            value: SEEDED_DISC_ID.to_string(),
            file_id: SEEDED_DISC_ID_FILE.to_string(),
        }]
    );

    // The row carries the draft and provenance the pane and bulk import consume.
    let row = queue_row(&fixture, &key).await;
    assert_eq!(row.release_link, Some(crate::ReleaseLink::Pressing(pick)));
}

/// The same release, printing the label and the date its own document states —
/// the facts a row's identified hover reads back for that source.
fn release_json_stating_pressing(
    release_id: &str,
    group_id: &str,
    track_lengths: &[u64],
    label: &str,
    date: &str,
) -> String {
    let mut release: serde_json::Value =
        serde_json::from_str(&mb_release_json(release_id, group_id, track_lengths))
            .expect("release fixture parses");
    release["label-info"] = serde_json::json!([{ "label": { "name": label } }]);
    release["date"] = serde_json::json!(date);
    release.to_string()
}

/// The same Discogs release, printing its own label and year — different from
/// the MusicBrainz document's, because two sources describing one pressing can
/// disagree and a row naming both says what each of them says.
fn discogs_release_json_stating_pressing(release_id: &str, label: &str, year: u32) -> String {
    let mut release: serde_json::Value = serde_json::from_str(&discogs_release_json(release_id))
        .expect("the Discogs release fixture parses");
    release["labels"] = serde_json::json!([{ "name": label, "catno": "CAT-1" }]);
    release["year"] = serde_json::json!(year);
    release.to_string()
}

/// The row says which sources its draft was read from, and what each of their
/// own releases states. A pick pairs one source's release with another's into
/// one pressing and claims both, so the row lists both — each stating its own
/// document's label and year, which need not agree.
#[tokio::test(flavor = "multi_thread")]
async fn a_picked_row_states_what_each_claimed_source_says() {
    let fixture = Fixture::new("pick-reading").await;
    fixture.use_discogs().await;
    let dir = fixture.disc_id_candidate("Album");
    let probed = fixture.probed_total_ms(&dir);
    fixture.scan(1).await;
    let key = crate::CandidateKey::folder(&dir);

    fixture.provider.route(
        "/release/mb-stated-1?",
        200,
        release_json_stating_pressing(
            "mb-stated-1",
            "rg-stated-1",
            &[probed, 0],
            "Label Name",
            "1976-04-01",
        ),
    );
    fixture.provider.route(
        "/releases/70000301",
        200,
        discogs_release_json_stating_pressing("70000301", "Other Label", 1988),
    );
    fixture.musicbrainz.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release("70000301".to_string()),
        None,
    )]);

    fixture
        .import
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                    "mb-stated-1".to_string(),
                ),
                partners: vec![bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    "70000301",
                )],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .expect("picking a paired pressing succeeds");

    let reading = queue_row(&fixture, &key).await.reading;
    let crate::triage::TriageReading::Identified { records } = reading else {
        panic!("a picked row reads as identified, got {reading:?}");
    };
    assert_eq!(
        records
            .iter()
            .map(|record| (record.catalog(), record.key(), record.url()))
            .collect::<Vec<_>>(),
        vec![
            (
                bae_catalogs::catalog::Catalog::MusicBrainz,
                "mb-stated-1",
                "https://musicbrainz.org/release/mb-stated-1".to_string(),
            ),
            (
                bae_catalogs::catalog::Catalog::Discogs,
                "70000301",
                "https://www.discogs.com/release/70000301".to_string(),
            ),
        ],
        "the row names every catalog the pick claims, each with its own page"
    );
    assert_eq!(
        records[0].catalog(),
        bae_catalogs::catalog::Catalog::MusicBrainz,
        "the release the draft was read from is the list's first"
    );
}

/// The list's row for one candidate, over the tabs that present a candidate's
/// own reading — which of them it lands in is not what these tests are about.
/// Done presents the library release instead.
async fn queue_row(fixture: &Fixture, key: &crate::CandidateKey) -> crate::TriageRow {
    queue_item(fixture, key).await.0
}

/// An import claiming a candidate writes its standing, so the list is read
/// again at the same request revision, with the row saying the import owns it
/// — waiting for the worker, then taken up, then writing — and what it offers
/// at each. A progress tick within the running import is not written and
/// delivers nothing. The page the claim delivers counts the row In Progress.
#[tokio::test(flavor = "multi_thread")]
async fn a_claimed_import_reaches_its_row_on_the_page() {
    let fixture = Fixture::new("live-state-on-the-page").await;
    let dir = fixture.disc_id_candidate("Album Title");
    let key = crate::CandidateKey::folder(&dir);
    fixture.scan(1).await;

    let list = list_subscription(&fixture, page_window(0, 50)).await;
    let initial = next_page(&list).await;
    let idle = page_live(&initial).remove(&key).expect("the scanned candidate has a row");
    assert!(!idle.facts.importing());
    assert!(!idle.actions.is_empty(), "an idle row offers its commands");
    assert_eq!(counted(&initial.pending_filter_entries, crate::PendingFilter::All), Some(1));
    assert_eq!(counted(&initial.pending_filter_entries, crate::PendingFilter::InProgress), Some(0));

    fixture.import.claim_candidate_for_import(&key, "import-1").await;
    let claimed = next_page(&list).await;
    assert_eq!(
        claimed.view_revision, initial.view_revision,
        "a claimed import changed the list's request"
    );
    assert_eq!(claimed.cause, coven::LiveQueryCause::Write);
    let queued = &page_live(&claimed)[&key];
    assert_eq!(queued.facts.import, Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued)));
    assert_eq!(
        queued.actions,
        vec![crate::CandidateAction::CancelImport, crate::CandidateAction::RevealFolder],
        "a claimed import offers only its cancel, beside showing its folder"
    );
    assert_eq!(counted(&claimed.pending_filter_entries, crate::PendingFilter::All), Some(1));
    assert_eq!(
        counted(&claimed.pending_filter_entries, crate::PendingFilter::InProgress),
        Some(1),
        "the claimed import is counted In Progress"
    );

    // The worker takes the import up: it writes where the import stands,
    // then reports it.
    fixture
        .import
        .move_candidate_import_for_test(
            &key,
            "import-1",
            crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles),
        )
        .await;
    fixture.import.emit_event_for_test(reported(&key, preparing("import-1")));
    let taken_up = &page_live(&next_page(&list).await)[&key];
    assert_eq!(
        taken_up.facts.import,
        Some(crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles)),
        "the worker's first report takes the import off the queue"
    );
    assert_eq!(taken_up.actions, queued.actions);

    // Each step the worker writes reaches the row; a percent within it does
    // not.
    fixture
        .import
        .move_candidate_import_for_test(
            &key,
            "import-1",
            crate::ImportStep::Running(crate::ImportPhase::MeasuringLoudness),
        )
        .await;
    let measuring = &page_live(&next_page(&list).await)[&key];
    assert_eq!(
        measuring.facts.import,
        Some(crate::ImportStep::Running(crate::ImportPhase::MeasuringLoudness))
    );
    fixture.import.emit_event_for_test(reported(
        &key,
        running(crate::ImportPhase::MeasuringLoudness, Some(40)),
    ));
    fixture
        .import
        .move_candidate_import_for_test(
            &key,
            "import-1",
            crate::ImportStep::Running(crate::ImportPhase::Finalizing),
        )
        .await;
    fixture
        .import
        .emit_event_for_test(reported(&key, running(crate::ImportPhase::Finalizing, None)));
    let writing = next_page(&list).await;
    let writing_live = &page_live(&writing)[&key];
    assert_eq!(
        writing_live.facts.import,
        Some(crate::ImportStep::Running(crate::ImportPhase::Finalizing)),
        "the progress tick before it delivered a page of its own"
    );
    assert_eq!(
        writing_live.actions,
        vec![crate::CandidateAction::RevealFolder],
        "an import writing its release offers no cancel it would refuse"
    );

    // A window move is one more request to the same query, and the rows it
    // reads carry where each stands.
    list.set_windows(page_window(0, 49)).unwrap();
    let moved = next_page(&list).await;
    assert_eq!(moved.view_revision, initial.view_revision);
    assert_eq!(moved.cause, coven::LiveQueryCause::Request);
    assert_eq!(
        moved.windows[0].items, writing.windows[0].items,
        "the window move read the rows as they were delivered"
    );
}

/// Where a candidate off the page stands reaches no row: it delivers the page
/// again only for the count it moves. Once a new window puts the candidate on
/// the page, its row carries its standing and the same subscription delivers
/// its changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_off_the_page_reaches_its_row_once_the_window_holds_it() {
    let fixture = Fixture::new("live-state-off-the-page").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    fixture.scan(2).await;

    let list = list_subscription(&fixture, page_window(0, 1)).await;
    let initial = next_page(&list).await;
    let on_page = page_live(&initial);
    assert_eq!(on_page.len(), 1, "the window holds one row");
    let on_key = on_page.into_keys().next().expect("one row");
    let off_key = [first, second]
        .into_iter()
        .map(|dir| crate::CandidateKey::folder(&dir))
        .find(|key| *key != on_key)
        .expect("the other candidate is off the page");

    fixture.import.claim_candidate_for_import(&off_key, "import-off").await;
    let counted_off = next_page(&list).await;
    assert_eq!(counted_off.view_revision, initial.view_revision);
    assert_eq!(counted_off.windows, initial.windows, "the claim off the page reached no row on it");
    assert_eq!(
        counted(&counted_off.pending_filter_entries, crate::PendingFilter::InProgress),
        Some(1)
    );

    fixture.import.claim_candidate_for_import(&on_key, "import-on").await;
    let claimed = next_page(&list).await;
    assert_eq!(claimed.view_revision, initial.view_revision);
    assert_eq!(
        page_live(&claimed)[&on_key].facts.import,
        Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued))
    );
    assert_eq!(counted(&claimed.pending_filter_entries, crate::PendingFilter::InProgress), Some(2));

    list.set_windows(page_window(0, 2)).unwrap();
    let widened = next_page(&list).await;
    assert_eq!(
        widened.view_revision, initial.view_revision,
        "a window is the query's request, not the list's view"
    );
    assert_eq!(widened.cause, coven::LiveQueryCause::Request);
    assert_eq!(
        page_live(&widened)[&off_key].facts.import,
        Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued)),
        "the row the window now holds carries where it stands"
    );

    fixture
        .import
        .move_candidate_import_for_test(
            &off_key,
            "import-off",
            crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles),
        )
        .await;
    fixture.import.emit_event_for_test(reported(&off_key, preparing("import-off")));
    let taken_up = next_page(&list).await;
    assert_eq!(taken_up.view_revision, widened.view_revision);
    let live = page_live(&taken_up);
    assert_eq!(
        live[&off_key].facts.import,
        Some(crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles))
    );
    assert_eq!(
        live[&on_key].facts.import,
        Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued))
    );
}

/// A verdict stored for a row off the page reaches no row on it, but moves
/// the filter's counts, so the list delivers the page again with them.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_off_the_page_delivers_the_counts_it_moves() {
    let fixture = Fixture::new("verdict-off-the-page").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    fixture.set_identify_automatically(false).await;
    fixture.scan(2).await;

    let list = list_subscription(&fixture, page_window(0, 1)).await;
    let initial = next_page(&list).await;
    let on_key = page_live(&initial).into_keys().next().expect("the window holds one row");
    let off_dir = [&first, &second]
        .into_iter()
        .find(|dir| crate::CandidateKey::folder(dir) != on_key)
        .expect("the other candidate is off the page")
        .clone();

    let probed = fixture.probed_total_ms(&off_dir);
    fixture.archive("mb-off-page", "rg-off-page", &[probed, 0]).await;
    fixture.store_settled_verdict(&off_dir, "mb-off-page", "rg-off-page").await;
    let moved = next_page(&list).await;
    assert_ne!(
        moved.pending_filter_entries, initial.pending_filter_entries,
        "the verdict moved a count"
    );
    assert_eq!(counted(&moved.pending_filter_entries, crate::PendingFilter::All), Some(2));
    assert_eq!(moved.windows, initial.windows, "the verdict off the page reached no row on it");
    assert_eq!(page_live(&moved).into_keys().collect::<Vec<_>>(), vec![on_key]);
}

fn preparing(import_id: &str) -> crate::ImportProgress {
    crate::ImportProgress::Preparing {
        import_id: import_id.to_string(),
        step: crate::PrepareStep::ValidatingSourceFiles,
    }
}
