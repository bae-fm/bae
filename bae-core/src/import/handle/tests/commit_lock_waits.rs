//! What a pane control waits on. Every one of them takes the folder-state
//! commit lock, so whatever else holds that lock sets how long a click takes
//! to land. Reading files — a folder's tags, over a network share that answers
//! a read in hundreds of milliseconds — is never done while holding it: a
//! scan of a large share would otherwise hold every click behind it.

use super::*;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

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

impl crate::import::file_tag_snapshot::FileTagReader for HeldTagReader {
    fn read(
        &self,
        path: &Path,
    ) -> Result<crate::import::file_tag_snapshot::FileTagRead, crate::import::ImportError> {
        let held = self
            .held
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|held| path.starts_with(held));
        if held {
            // The receiver is gone once the test has stopped watching, which
            // changes nothing about the read.
            let _ = self.entered.send(path.to_path_buf());
            let (lock, condition) = &*self.gate;
            let mut open = lock.lock().unwrap();
            while !*open {
                open = condition.wait(open).unwrap();
            }
        }
        crate::import::file_tag_snapshot::LoftyFileTagReader.read(path)
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
    for (index, fixture) in ["01 Test Track 1.flac", "02 Test Track 2.flac"]
        .into_iter()
        .enumerate()
    {
        std::fs::copy(
            fixtures.join(fixture),
            folder.join(format!("{:02} Track.flac", index + 1)),
        )
        .unwrap();
    }
}

/// Retitle `key` and wait for its pane to read the new title back, as long as
/// that takes.
async fn retitle(handle: &ImportServiceHandle, key: &str) {
    let mut pane_query = handle.subscribe_candidate_pane(key);
    pane_query.next().await.into_result().unwrap();
    handle
        .set_candidate_edit_field(
            key,
            crate::import::CandidateEditField::AlbumTitle,
            "Retitled".to_string(),
        )
        .await
        .unwrap();
    loop {
        let projection = pane_query
            .next()
            .await
            .into_result()
            .unwrap()
            .expect("the candidate is still stored");
        if projection.metadata_draft.album_title == "Retitled" {
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
    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, _hash) = picked_candidate(&manager, &tmp, "Album").await;
    // The scan below seeds a new folder's draft from its tags, which is the
    // read under test.
    manager.set_prefill_with_file_metadata(true).unwrap();
    let slow_root = tmp.path().join("slow share");
    audio_folder(&slow_root, "Other Album");
    let (reader, mut entered) = HeldTagReader::new();
    reader.hold(&slow_root);
    let _opens = OpensOnDrop(reader.clone());
    let handle = manager
        .start_import_service_reading_tags_with(tokio::runtime::Handle::current(), reader.clone());
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileMetadata,
        )
        .await
        .unwrap();
    handle
        .add_watched_folder(slow_root.to_string_lossy().into_owned())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), entered.recv())
        .await
        .expect("the scan reads the new folder's tags")
        .expect("the reader reports its reads");

    let landed = tokio::time::timeout(Duration::from_secs(5), retitle(&handle, &key)).await;
    reader.open();
    shut_down(handle).await;
    landed.expect("the edit lands without waiting for the scan's tag reads");
}

/// Two candidates, each under a watched root of its own, served by a handle
/// whose tag reads the returned reader can hold — and the keys of both, with
/// the folder of the first. The second is read as its own tags already; the
/// first has had its tags read by nobody.
async fn two_candidates() -> (
    ImportServiceHandle,
    Arc<HeldTagReader>,
    tokio::sync::mpsc::UnboundedReceiver<PathBuf>,
    (String, PathBuf),
    String,
    [TempDir; 2],
) {
    let (manager, tmp) = setup_test_manager().await;
    let other = TempDir::new().unwrap();
    let (held_candidate, held_key, _) = picked_candidate(&manager, &tmp, "Held Album").await;
    let (_, edited_key, _) = picked_candidate(&manager, &other, "Edited Album").await;
    let (reader, entered) = HeldTagReader::new();
    let handle = manager
        .start_import_service_reading_tags_with(tokio::runtime::Handle::current(), reader.clone());
    handle
        .select_candidate_metadata_provenance(
            edited_key.clone(),
            crate::import::MetadataProvenance::FileMetadata,
        )
        .await
        .unwrap();
    (
        handle,
        reader,
        entered,
        (held_key, held_candidate.path),
        edited_key,
        [tmp, other],
    )
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
        async move {
            handle
                .select_candidate_metadata_provenance(
                    held_key,
                    crate::import::MetadataProvenance::FileMetadata,
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(10), entered.recv())
        .await
        .expect("the pick reads the folder's tags")
        .expect("the reader reports its reads");

    let landed = tokio::time::timeout(Duration::from_secs(5), retitle(&handle, &edited_key)).await;
    reader.open();
    let picked = pick.await.unwrap();
    shut_down(handle).await;
    landed.expect("the edit lands without waiting for the pick's tag reads");
    picked.expect("the pick lands once its read finishes");
}

/// Taking a track out of a folder redraws its draft from the tags of the
/// tracks left. While that read waits on the volume, an edit in another
/// candidate's pane lands.
#[tokio::test(flavor = "multi_thread")]
async fn a_pane_edit_lands_while_a_file_decision_reads_another_folders_tags() {
    let (handle, reader, mut entered, (held_key, held_folder), edited_key, _tmp) =
        two_candidates().await;
    let _opens = OpensOnDrop(reader.clone());
    handle
        .select_candidate_metadata_provenance(
            held_key.clone(),
            crate::import::MetadataProvenance::FileMetadata,
        )
        .await
        .unwrap();
    handle
        .library_manager
        .set_prefill_with_file_metadata(true)
        .unwrap();
    reader.hold(&held_folder);
    let decision = tokio::spawn({
        let handle = handle.clone();
        async move {
            handle
                .set_file_role(
                    held_key,
                    "01 Track.flac".to_string(),
                    crate::import::folder_scanner::FileRoleChoice::NotATrack,
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(10), entered.recv())
        .await
        .expect("the decision reads the folder's tags")
        .expect("the reader reports its reads");

    let landed = tokio::time::timeout(Duration::from_secs(5), retitle(&handle, &edited_key)).await;
    reader.open();
    let decided = decision.await.unwrap();
    shut_down(handle).await;
    landed.expect("the edit lands without waiting for the decision's tag reads");
    decided.expect("the decision lands once its read finishes");
}
