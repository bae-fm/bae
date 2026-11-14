//! In Progress follows where each candidate stands: a row shows while its run
//! or import is queued or going, and leaves when it ends — and leaves the
//! entry the tables put it under for as long. Every snapshot counts the
//! filter's entries with each row's standing, and a change that moves a count
//! off the page delivers one. A stored pick reads back on its row as the
//! answer and the identity it committed, leading with the picked release and
//! stating what each claimed source says.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::handle::tests::*;
use crate::identification::tests::*;
use crate::{ImportListView, ImportWindow, LiveImportList, PendingFilter};
use bae_catalogs::fake_catalogs::FakeCatalogs;
use bae_catalogs::test_releases::seed_mb_release_with_catalog;
use bae_catalogs::test_releases::{discogs_release_json, mb_release_json, mb_titled_release_json};
use bae_store::{DbError, ListWindow};
use bae_util::test_channels::HANG_GUARD;
use tempfile::TempDir;

/// What the list under one filter entry shows, as its window and its filter
/// entries last delivered them: the keys of its rows, and each entry's count.
#[derive(Clone, Default)]
struct ListState {
    shown: Option<Vec<crate::CandidateKey>>,
    entries: Vec<crate::PendingFilterEntry>,
}

/// The list under one filter entry, followed through one window over every
/// item and the filter's entries, each its own live query.
struct LiveList {
    _list: LiveImportList,
    state: tokio::sync::watch::Receiver<ListState>,
}

fn keys_of(items: &[crate::ImportListItem]) -> Vec<crate::CandidateKey> {
    items.iter().filter_map(candidate_key).cloned().collect()
}

impl LiveList {
    async fn new(handle: &ImportServiceHandle, filter: PendingFilter) -> Self {
        let list = handle
            .open_list(ImportListView { pending_filter: filter, ..ImportListView::default() })
            .await
            .expect("the list opens");
        let (state, receiver) = tokio::sync::watch::channel(ListState::default());
        let state = std::sync::Arc::new(state);
        let mut window =
            list.subscribe_window(ListWindow::Forward { from: None, until: None, limit: u64::MAX });
        let rows = state.clone();
        tokio::spawn(async move {
            while let Some(event) = window.next().await {
                let window = event.result.expect("the window reads");
                rows.send_modify(|state| state.shown = Some(keys_of(&window.read.rows)));
            }
        });
        let mut entries = list.subscribe_filter_entries();
        tokio::spawn(async move {
            while let Some(entries_read) = entries.next().await {
                state.send_modify(|state| state.entries = entries_read.expect("the entries read"));
            }
        });
        Self { _list: list, state: receiver }
    }

    /// Wait until the list shows exactly `expected`; at once when it already
    /// does, since a change that moves no row delivers nothing.
    async fn shows(&mut self, expected: &[&crate::CandidateKey]) {
        let wanted: Vec<crate::CandidateKey> = expected.iter().map(|key| (*key).clone()).collect();
        let reached = tokio::time::timeout(
            HANG_GUARD,
            self.state.wait_for(|state| state.shown.as_ref() == Some(&wanted)),
        )
        .await
        .is_ok();
        assert!(reached, "the list shows {:?}, not {wanted:?}", self.state.borrow().shown);
    }

    /// How many rows the filter's entries count under `filter`; none before
    /// the first.
    fn counted(&self, filter: PendingFilter) -> Option<u32> {
        counted(&self.state.borrow().entries, filter)
    }

    /// Wait until the entries count `count` rows under `filter`; at once when
    /// they already do.
    async fn counts(&mut self, filter: PendingFilter, count: u32) {
        let reached = tokio::time::timeout(
            HANG_GUARD,
            self.state.wait_for(|state| counted(&state.entries, filter) == Some(count)),
        )
        .await
        .is_ok();
        assert!(
            reached,
            "the list counts {:?} under {filter:?}, not {count}",
            self.counted(filter)
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

/// A list over every item that holds one row on its page: the window of the
/// first item, closed at the second, whose range the other row is outside
/// of, and the filter's entries, each its own live query.
struct OnePage {
    _list: LiveImportList,
    window: coven::ReconfigurableLiveQuery<ListWindow, ImportWindow, DbError>,
    entries: coven::LiveQuery<Vec<crate::PendingFilterEntry>, DbError>,
}

/// A watched root holding two stored candidates, each with its draft read
/// from its own tags, and the list under All through a window that holds the
/// first item alone: the key on that page, and the key off it.
async fn one_row_on_the_page(
    providers: bae_catalogs::providers::Providers,
    http: bae_util::http::Http,
) -> (ImportServiceHandle, TempDir, OnePage, crate::CandidateKey, crate::CandidateKey) {
    let (parts, db, tmp) = setup_test_import_with(providers, http).await;
    let tables = crate::tables::ImportTables::new(db);
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
    let list = handle.open_list(ImportListView::default()).await.expect("the list opens");
    let mut window =
        list.subscribe_window(ListWindow::Forward { from: None, until: None, limit: 1 });
    let initial = window.next().await.expect("the query stays open").result.unwrap();
    let on_page = keys_of(&initial.read.rows);
    assert_eq!(on_page.len(), 1, "the page holds one row: {on_page:?}");
    // The first delivery closes the window at the row beyond it.
    let beyond = initial.read.beyond.clone().expect("the second row is beyond the page");
    window
        .requests()
        .set(ListWindow::Forward { from: None, until: Some(beyond.clone()), limit: 1 })
        .unwrap();
    let mut entries = list.subscribe_filter_entries();
    let entries_read = entries.next().await.expect("the query stays open").unwrap();
    assert_eq!(counted(&entries_read, PendingFilter::All), Some(2));
    assert_eq!(counted(&entries_read, PendingFilter::InProgress), Some(0));
    assert_eq!(counted(&entries_read, PendingFilter::Identified), Some(0));
    let on_page = on_page.into_iter().next().expect("one row");
    let off_page = if on_page == first_key { second_key } else { first_key };
    (handle, tmp, OnePage { _list: list, window, entries }, on_page, off_page)
}

/// The candidate whose row `item` is.
fn candidate_key(item: &crate::ImportListItem) -> Option<&crate::CandidateKey> {
    match item {
        crate::ImportListItem::Candidate { row, .. } => Some(&row.candidate_key),
        crate::ImportListItem::GroupHeader { .. } | crate::ImportListItem::Invalid { .. } => None,
    }
}

/// How many rows `entries` count under `filter`; `None` when they hold no
/// entry for it, as before the list's first snapshot.
fn counted(entries: &[crate::PendingFilterEntry], filter: PendingFilter) -> Option<u32> {
    entries.iter().find(|entry| entry.filter == filter).map(|entry| entry.count)
}

/// The next filter entries `page` delivers that `accept` admits.
async fn entries_until(
    page: &mut OnePage,
    accept: impl Fn(&[crate::PendingFilterEntry]) -> bool,
) -> Vec<crate::PendingFilterEntry> {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let entries = page.entries.next().await.expect("the query stays open").unwrap();
            if accept(&entries) {
                return entries;
            }
        }
    })
    .await
    .expect("the entries reach what the test waits for")
}

/// A run starting for a row off the page, under All, changes nothing on the
/// page but moves the row's count to In Progress, so the entries deliver it.
#[tokio::test(flavor = "multi_thread")]
async fn a_run_off_the_page_delivers_its_count() {
    let (handle, _tmp, mut page, _on_page, off_page) = one_row_on_the_page(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;

    admit(&handle, &off_page).await;
    let moved =
        entries_until(&mut page, |entries| counted(entries, PendingFilter::InProgress) == Some(1))
            .await;

    assert_eq!(counted(&moved, PendingFilter::All), Some(2));
    // Coven does not narrow the import list's windows by their range (the placement
    // reads headers through a correlated EXISTS, and Found members sort through a
    // join on unit), so any write the list reads reruns every window.
    assert!(page.window.is_marked_for_rerun(), "the window reruns for a write it cannot rule out");
    shut_down(handle).await;
}

/// A pick for a row off the page moves its count to Identified, so the
/// entries deliver it.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_off_the_page_delivers_its_count() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, mut page, _on_page, off_page) =
        one_row_on_the_page(fakes.providers(), bae_util::http::Http::for_test()).await;
    seed_mb_release_with_catalog(&fakes, "off-the-page", "CAT-1");

    pick(&handle, &off_page, "off-the-page").await;
    // The pick's write and its cancelling the row's run each deliver; while
    // the run still goes it counts the row under In Progress.
    let moved =
        entries_until(&mut page, |entries| counted(entries, PendingFilter::Identified) == Some(1))
            .await;

    assert_eq!(counted(&moved, PendingFilter::All), Some(2));
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
    assert_eq!(summary.album_artists.plain_text(), "Picked Artist Name");
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

    let list = list_subscription(&fixture, page_window(50)).await;
    let initial = next_page(&list).await;
    let idle = page_live(&initial).remove(&key).expect("the scanned candidate has a row");
    assert!(!idle.facts.importing());
    assert!(!idle.actions.is_empty(), "an idle row offers its commands");
    assert_eq!(counted(&initial.pending_filter_entries, crate::PendingFilter::All), Some(1));
    assert_eq!(counted(&initial.pending_filter_entries, crate::PendingFilter::InProgress), Some(0));

    fixture.import.claim_candidate_for_import(&key, "import-1").await;
    let claimed = next_page(&list).await;
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
    list.set_window(page_window(49));
    let moved = next_page(&list).await;
    assert_eq!(
        moved.windows[0].rows, writing.windows[0].rows,
        "the window move read the rows as they were delivered"
    );
}

/// Where a candidate off the page stands reaches no row and reruns no
/// window: it moves the count the filter's entries hold. Once a new window
/// puts the candidate on the page, its row carries its standing and the same
/// subscription delivers its changes.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_off_the_page_reaches_its_row_once_the_window_holds_it() {
    let fixture = Fixture::new("live-state-off-the-page").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    fixture.scan(2).await;

    let list =
        fixture.import.open_list(crate::ImportListView::default()).await.expect("the list opens");
    let mut window = list.subscribe_window(page_window(1));
    let initial = window.next().await.expect("the query stays open").result.unwrap();
    let on_page = keys_of(&initial.read.rows);
    assert_eq!(on_page.len(), 1, "the window holds one row");
    let on_key = on_page.into_iter().next().expect("one row");
    let off_key = [first, second]
        .into_iter()
        .map(|dir| crate::CandidateKey::folder(&dir))
        .find(|key| *key != on_key)
        .expect("the other candidate is off the page");
    // The first delivery closes the window at the row beyond it.
    let beyond = initial.read.beyond.clone().expect("the second row is beyond");
    let requests = window.requests();
    requests.set(ListWindow::Forward { from: None, until: Some(beyond), limit: 1 }).unwrap();
    let mut entries = list.subscribe_filter_entries();
    entries.next().await.expect("the query stays open").unwrap();

    fixture.import.claim_candidate_for_import(&off_key, "import-off").await;
    let counted_off = loop {
        let entries = entries.next().await.expect("the query stays open").unwrap();
        if counted(&entries, crate::PendingFilter::InProgress) == Some(1) {
            break entries;
        }
    };
    // Coven does not narrow the import list's windows by their range (the placement
    // reads headers through a correlated EXISTS, and Found members sort through a
    // join on unit), so any write the list reads reruns every window.
    assert!(window.is_marked_for_rerun(), "the window reruns for a write it cannot rule out");
    assert_eq!(counted(&counted_off, crate::PendingFilter::All), Some(2));

    fixture.import.claim_candidate_for_import(&on_key, "import-on").await;
    let claimed = loop {
        let value = window.next().await.expect("the query stays open").result.unwrap();
        if page_live_of(&value).get(&on_key).is_some_and(|live| live.facts.importing()) {
            break value;
        }
    };
    assert_eq!(
        page_live_of(&claimed)[&on_key].facts.import,
        Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued))
    );

    requests.set(ListWindow::Forward { from: None, until: None, limit: 2 }).unwrap();
    let widened = loop {
        let value = window.next().await.expect("the query stays open").result.unwrap();
        if value.read.rows.len() == 2 {
            break value;
        }
    };
    assert_eq!(
        page_live_of(&widened)[&off_key].facts.import,
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
    let live = loop {
        let value = window.next().await.expect("the query stays open").result.unwrap();
        let live = page_live_of(&value);
        if live[&off_key].facts.import
            == Some(crate::ImportStep::Preparing(crate::PrepareStep::ValidatingSourceFiles))
        {
            break live;
        }
    };
    assert_eq!(
        live[&on_key].facts.import,
        Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued))
    );
}

/// What is running for each candidate a window holds, by key.
fn page_live_of(
    window: &ImportWindow,
) -> std::collections::BTreeMap<crate::CandidateKey, crate::CandidateLiveState> {
    window
        .read
        .rows
        .iter()
        .filter_map(|item| match item {
            crate::ImportListItem::Candidate { row, live, .. } => {
                Some((row.candidate_key.clone(), live.clone()))
            }
            _ => None,
        })
        .collect()
}

/// A verdict stored for a row off the page reaches no row on it, and moves
/// the filter's counts.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_off_the_page_delivers_the_counts_it_moves() {
    let fixture = Fixture::new("verdict-off-the-page").await;
    let first = fixture.disc_id_candidate("First");
    let second = fixture.disc_id_candidate("Second");
    std::fs::write(second.join("notes.txt"), "distinct candidate").unwrap();
    fixture.set_identify_automatically(false).await;
    fixture.scan(2).await;

    let list =
        fixture.import.open_list(crate::ImportListView::default()).await.expect("the list opens");
    let mut window = list.subscribe_window(page_window(1));
    let initial = window.next().await.expect("the query stays open").result.unwrap();
    let on_key = page_live_of(&initial).into_keys().next().expect("the window holds one row");
    let beyond = initial.read.beyond.clone().expect("the second row is beyond");
    window
        .requests()
        .set(ListWindow::Forward { from: None, until: Some(beyond), limit: 1 })
        .unwrap();
    let mut entries = list.subscribe_filter_entries();
    let before = entries.next().await.expect("the query stays open").unwrap();
    let off_dir = [&first, &second]
        .into_iter()
        .find(|dir| crate::CandidateKey::folder(dir) != on_key)
        .expect("the other candidate is off the page")
        .clone();

    let probed = fixture.probed_total_ms(&off_dir);
    fixture.archive("mb-off-page", "rg-off-page", &[probed, 0]).await;
    fixture.store_settled_verdict(&off_dir, "mb-off-page", "rg-off-page").await;
    let moved = entries.next().await.expect("the query stays open").unwrap();
    assert_ne!(moved, before, "the verdict moved a count");
    assert_eq!(counted(&moved, crate::PendingFilter::All), Some(2));
    // Coven does not narrow the import list's windows by their range (the placement
    // reads headers through a correlated EXISTS, and Found members sort through a
    // join on unit), so any write the list reads reruns every window.
    assert!(window.is_marked_for_rerun(), "the window reruns for a write it cannot rule out");
}

fn preparing(import_id: &str) -> crate::ImportProgress {
    crate::ImportProgress::Preparing {
        import_id: import_id.to_string(),
        step: crate::PrepareStep::ValidatingSourceFiles,
    }
}
