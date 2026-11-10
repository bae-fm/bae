//! The commit lock every pane control's write takes: whatever else holds it
//! must not wait on reading files, a write prepared before an import's claim
//! cannot land after it, and a durable write that panics fails its caller.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::handle::tests::*;
use crate::tests::import::TestImport;
use crate::ImportError;
use bae_store::Store;
use bae_util::test_channels::HANG_GUARD;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use tempfile::TempDir;

/// [`setup_test_import`] reporting its bugs to `diagnostics`.
async fn setup_test_import_reporting_to(
    diagnostics: bae_diagnostics::Diagnostics,
) -> (TestImport, Store, TempDir) {
    setup_test_import_over(
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
        diagnostics,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn an_edit_prepared_before_a_claim_cannot_land_after_the_claim() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let title_before =
        pane(&handle, &key).await.draft().unwrap().metadata_draft.clone().album_title;
    let commit = handle.services.folder_state_commit.lock("hold for a test").await;
    let edit = tokio::spawn({
        let handle = handle.clone();
        let key = key.clone();
        async move {
            handle
                .set_candidate_edit_field(
                    &key,
                    crate::DraftFieldEdit::Text {
                        field: crate::CandidateEditField::AlbumTitle,
                        value: "Racing edit".to_string(),
                    },
                )
                .await
        }
    });
    tokio::time::timeout(HANG_GUARD, handle.services.folder_state_commit.waiting_reaches(1))
        .await
        .expect("the edit parks on the commit lock");
    handle.runtime.claim_for_import(&key, "import-1", false).await.unwrap();
    drop(commit);

    assert!(matches!(edit.await.unwrap(), Err(crate::ImportError::CandidateImportInProgress)));
    assert_eq!(pane(&handle, &key).await.draft().unwrap().metadata_draft.album_title, title_before);
    shut_down(handle).await;
}

/// Reads tags as the app does, except that once the test holds a folder, a
/// read of any file under it reports itself and then waits until the test
/// opens the gate — a volume that answers only when the test says so.
struct HeldTagReader {
    held: Mutex<Option<PathBuf>>,
    entered: tokio::sync::mpsc::UnboundedSender<PathBuf>,
    gate: Arc<(Mutex<bool>, Condvar)>,
}

impl HeldTagReader {
    fn new() -> (Arc<Self>, tokio::sync::mpsc::UnboundedReceiver<PathBuf>) {
        let (entered, entered_rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Arc::new(Self {
                held: Mutex::new(None),
                entered,
                gate: Arc::new((Mutex::new(false), Condvar::new())),
            }),
            entered_rx,
        )
    }

    fn hold(&self, folder: &Path) {
        *self.held.lock().unwrap() = Some(folder.to_path_buf());
    }

    fn open(&self) {
        let (lock, condition) = &*self.gate;
        *lock.lock().unwrap() = true;
        condition.notify_all();
    }
}

impl crate::file_tag_reader::FileTagReader for HeldTagReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::file_tag_reader::FileTagRead, crate::file_tag_reader::TagReadFailure> {
        let held = self.held.lock().unwrap().as_ref().is_some_and(|held| path.starts_with(held));
        if held {
            // The receiver is gone once the test has stopped watching, which
            // changes nothing about the read.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a test that stopped watching for entry needs no report"
            )]
            let _ = self.entered.send(path.to_path_buf());
            let (lock, condition) = &*self.gate;
            let mut open = lock.lock().unwrap();
            while !*open {
                open = condition.wait(open).unwrap();
            }
        }
        crate::file_tag_reader::LoftyFileTagReader.read(path)
    }
}

/// Opens the gate when dropped, so a failed assertion lets the held read
/// finish and the test reports the failure instead of hanging at shutdown.
struct OpensOnDrop(Arc<HeldTagReader>);

impl Drop for OpensOnDrop {
    fn drop(&mut self) {
        self.0.open();
    }
}

/// A folder of two real audio files under `root`.
fn audio_folder(root: &Path, name: &str) {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac");
    for (index, fixture) in ["01 Test Track 1.flac", "02 Test Track 2.flac"].into_iter().enumerate()
    {
        std::fs::copy(fixtures.join(fixture), folder.join(format!("{:02} Track.flac", index + 1)))
            .unwrap();
    }
}

/// Retitle `key` and wait for its pane to read the new title back, as long as
/// that takes.
async fn retitle(handle: &ImportServiceHandle, key: &crate::CandidateKey) {
    let mut pane_query = handle.subscribe_candidate(Some(key.clone()));
    pane_query.next().await.expect("the query stays open").result.unwrap();
    handle
        .set_candidate_edit_field(
            key,
            crate::DraftFieldEdit::Text {
                field: crate::CandidateEditField::AlbumTitle,
                value: "Retitled".to_string(),
            },
        )
        .await
        .unwrap();
    loop {
        let projection = pane_query
            .next()
            .await
            .expect("the query stays open")
            .result
            .unwrap()
            .expect("the candidate is still stored");
        if projection.draft().unwrap().metadata_draft.album_title == "Retitled" {
            return;
        }
    }
}

/// A scan storing a folder it has not seen before reads that folder's tags to
/// seed its draft. While that read is still waiting on the volume, an edit in
/// the pane of a candidate from another folder lands, and the pane's own live
/// query delivers it.
#[tokio::test(flavor = "multi_thread")]
async fn a_pane_edit_lands_while_a_scan_reads_another_folders_tags() {
    let (parts, db, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let (_candidate, key, _hash) = picked_candidate(&tables, &tmp, "Album").await;
    let slow_root = tmp.path().join("slow share");
    audio_folder(&slow_root, "Other Album");
    let (reader, mut entered) = HeldTagReader::new();
    reader.hold(&slow_root);
    let _opens = OpensOnDrop(reader.clone());
    let handle = parts.start_reading_tags_with(reader.clone()).await.unwrap();
    handle.select_candidate_file_tags(key.clone()).await.unwrap();
    handle.add_watched_folder(slow_root.clone()).await.unwrap();
    tokio::time::timeout(HANG_GUARD, entered.recv())
        .await
        .expect("the scan reads the new folder's tags")
        .expect("the reader reports its reads");

    let landed = tokio::time::timeout(HANG_GUARD, retitle(&handle, &key)).await;
    reader.open();
    shut_down(handle).await;
    landed.expect("the edit lands without waiting for the scan's tag reads");
}

/// Two candidates, each under a watched root of its own, served by a handle
/// whose tag reads the returned reader can hold — and the keys of both, with
/// the folder of the first. The second is read as its own tags already; the
/// first is stored as a scan that could not read its tags, so nothing has
/// read them.
async fn two_candidates() -> (
    ImportServiceHandle,
    Arc<HeldTagReader>,
    tokio::sync::mpsc::UnboundedReceiver<PathBuf>,
    (crate::CandidateKey, PathBuf),
    crate::CandidateKey,
    [TempDir; 2],
) {
    let (parts, db, tmp) = setup_test_import().await;
    let tables = crate::tables::ImportTables::new(db);
    let other = TempDir::new().unwrap();
    let (mut held_candidate, held_key, _) = unscanned_candidate(&tables, &tmp, "Held Album").await;
    std::fs::write(held_candidate.path.join("Disc.cue"), SECOND_FILE_IN_TWO).unwrap();
    held_candidate.files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &held_candidate.path,
        held_candidate.scope,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    rescan_unread_into(&tables, held_candidate.clone()).await;
    let (_, edited_key, _) = picked_candidate(&tables, &other, "Edited Album").await;
    let (reader, entered) = HeldTagReader::new();
    let handle = parts.start_reading_tags_with(reader.clone()).await.unwrap();
    handle.select_candidate_file_tags(edited_key.clone()).await.unwrap();
    (handle, reader, entered, (held_key, held_candidate.path), edited_key, [tmp, other])
}

/// Picking a folder's own tags reads them when nothing has read them yet.
/// While that read waits on the volume, an edit in another candidate's pane
/// lands.
#[tokio::test(flavor = "multi_thread")]
async fn a_pane_edit_lands_while_a_pick_reads_another_folders_tags() {
    let (handle, reader, mut entered, (held_key, held_folder), edited_key, _tmp) =
        two_candidates().await;
    let _opens = OpensOnDrop(reader.clone());
    reader.hold(&held_folder);
    let pick = tokio::spawn({
        let handle = handle.clone();
        async move { handle.select_candidate_file_tags(held_key).await }
    });
    tokio::time::timeout(HANG_GUARD, entered.recv())
        .await
        .expect("the pick reads the folder's tags")
        .expect("the reader reports its reads");

    let landed = tokio::time::timeout(HANG_GUARD, retitle(&handle, &edited_key)).await;
    reader.open();
    let picked = pick.await.unwrap();
    shut_down(handle).await;
    landed.expect("the edit lands without waiting for the pick's tag reads");
    picked.expect("the pick lands once its read finishes");
}

/// Ignoring a folder's sheet redraws its draft from the tags of the audio it
/// gives back. While that read waits on the volume, an edit in another
/// candidate's pane lands.
#[tokio::test(flavor = "multi_thread")]
async fn a_pane_edit_lands_while_a_file_decision_reads_another_folders_tags() {
    let (handle, reader, mut entered, (held_key, held_folder), edited_key, _tmp) =
        two_candidates().await;
    let _opens = OpensOnDrop(reader.clone());
    handle.select_candidate_file_tags(held_key.clone()).await.unwrap();
    reader.hold(&held_folder);
    let decision = tokio::spawn({
        let handle = handle.clone();
        async move {
            handle
                .set_sheet_disc(
                    held_key,
                    "Disc.cue".to_string(),
                    crate::folder_scanner::SheetDisc::Ignored,
                )
                .await
        }
    });
    tokio::time::timeout(HANG_GUARD, entered.recv())
        .await
        .expect("the decision reads the folder's tags")
        .expect("the reader reports its reads");

    let landed = tokio::time::timeout(HANG_GUARD, retitle(&handle, &edited_key)).await;
    reader.open();
    let decided = decision.await.unwrap();
    shut_down(handle).await;
    landed.expect("the edit lands without waiting for the decision's tag reads");
    decided.expect("the decision lands once its read finishes");
}

/// The panic comes back to the caller as an internal error, and the crash
/// reporter hears of it once.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_that_panics_fails_its_caller_and_is_reported() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let (parts, _, _tmp) = setup_test_import_reporting_to(diagnostics).await;
    let handle = parts.start().await.expect("start the import service");

    let result: Result<(), ImportError> = handle
        .committed(async {
            if true {
                panic!("the write's bookkeeping broke");
            }
            Ok(())
        })
        .await;

    let Err(ImportError::Internal { detail }) = result else {
        panic!("a panicked write fails as internal, got {result:?}");
    };
    assert_eq!(detail, "a candidate write panicked: the write's bookkeeping broke");
    assert_eq!(bugs.reports(), vec![("a candidate write".to_string(), detail)]);
}
