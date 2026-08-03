#![cfg(feature = "test-utils")]
//! Tripwire: pure reads must run on coven's read-only companion connection
//! (`CovenHandle::sql_read`), which attaches no changeset session — so coven's
//! "journaled sql transaction changed nothing" warning never fires for a read.
//! coven now gates that warning on the transaction changing zero rows (a
//! `total_changes` delta), not on an empty synced capture — so a device-local
//! write on the plain `sql`/`call_sql` path is silent (it changes rows), and
//! only a read left on `sql` trips it.
//!
//! This asserts the property rather than leaving it to a `RUST_LOG=warn` grep
//! over the suite output. libtest captures each test's stdout and, for a
//! passing test, discards it unless `--nocapture` is passed — so a plain suite
//! run surfaces no warnings whether or not a read regressed, which is why the
//! grep is vacuous. A dedicated test with a positive control (below) doesn't
//! depend on that `--nocapture` plumbing and fails loudly on a regression. It
//! installs its own process-global subscriber recording into a shared buffer,
//! so it captures the warning coven logs from its own connection thread (a
//! thread-local subscriber would miss that cross-thread emit).

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use bae_core::db::Database;

/// A substring of the warning coven logs when a journaled `sql` transaction
/// changed zero rows (coven-core `database.rs`).
const TRIPWIRE: &str = "changed nothing";

#[derive(Clone)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn pure_reads_run_off_the_sync_journal() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = SharedBuf(buf.clone());
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .init();

    let tmp = tempfile::TempDir::new().unwrap();
    let db = Database::new_test(
        tmp.path().join("tripwire.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();

    let tripwire_hits = || {
        String::from_utf8(buf.lock().unwrap().clone())
            .unwrap()
            .matches(TRIPWIRE)
            .count()
    };

    // Positive control: a read-only closure driven through the raw journaled
    // `sql()` path. A `SELECT` changes zero rows, so coven fires the warning —
    // exactly the case a misrouted read hits. This proves the process-global
    // subscriber actually observes coven's cross-thread emit, so the negative
    // assertions below are not vacuous.
    db.handle()
        .sql(|sql| {
            sql.query_row("SELECT 1", [], |_row| Ok(()))
                .map_err(coven::CovenError::from)
        })
        .await
        .unwrap();
    assert!(
        tripwire_hits() >= 1,
        "expected a zero-change journaled sql() transaction to trip coven's \
         warning, so the assertions below are not vacuous",
    );

    buf.lock().unwrap().clear();

    // A device-local write on the plain journaled path must NOT warn under the
    // new semantics: saving `playback_state` changes rows, so coven's
    // zero-rows-changed check does not fire. This is why device-local writes
    // (e.g. the once-per-second playback_state save this is) stay silent with
    // no `sql_local`.
    db.save_playback_state(&bae_core::db::DbPlaybackState {
        context: None,
        manual: "off".to_string(),
        repeat: "off".to_string(),
        current_track_id: None,
        position_ms: None,
        volume: 1.0,
        is_muted: false,
    })
    .await
    .unwrap();

    // The migrated reads — at least one per db/client file. Each must resolve
    // on coven's read-only companion connection, journaling nothing. Empty
    // results are fine; the point is which connection ran the query.
    db.find_album_by_id("missing").await.unwrap(); // album.rs
    db.get_album_count().await.unwrap();
    db.get_albums(&[]).await.unwrap();
    db.find_artist_by_id("missing").await.unwrap(); // artist.rs
    db.get_artist_count().await.unwrap();
    db.find_track_by_id("missing").await.unwrap(); // track.rs
    db.get_all_track_ids().await.unwrap();
    db.find_release_by_id("missing").await.unwrap(); // release.rs
    db.get_release_identities("missing").await.unwrap(); // identity.rs
    db.load_playback_state().await.unwrap(); // playback.rs
                                             // `external_blob` needs a real row to bind (coven errors on a missing one),
                                             // so the blobs.rs read here is the one that answers for any id.
    db.has_pending_cloud_upload("missing").await.unwrap(); // blobs.rs
    db.outbox_queue().await.unwrap();

    assert_eq!(
        tripwire_hits(),
        0,
        "a pure read still journals through `sql`, or a device-local write \
         changed no rows; captured warnings:\n{}",
        String::from_utf8(buf.lock().unwrap().clone()).unwrap(),
    );

    // Writers asked to write what is already there. Each one decides there is
    // nothing to do, and must reach that decision before it opens a journaled
    // transaction — a write that writes nothing is the same misrouted read as a
    // `SELECT` on `sql`.
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();

    db.clear_playback_state().await.unwrap();
    db.clear_playback_state().await.unwrap(); // the row is already gone
    db.add_watched_import_folder(root).await.unwrap();
    assert!(!db.add_watched_import_folder(root).await.unwrap());
    db.set_import_candidate_skipped(root, "Album", true)
        .await
        .unwrap();
    assert!(!db
        .set_import_candidate_skipped(root, "Album", true)
        .await
        .unwrap());
    db.set_import_candidate_skipped(root, "Never Skipped", false)
        .await
        .unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(!db
        .finish_folder_scan(root, generation - 1, None)
        .await
        .unwrap());
    assert!(!db
        .save_import_candidate_verdict(&bae_core::db::NewImportCandidateVerdict {
            content_hash: "hash-with-no-row".to_string(),
            folder_path: format!("{root}/Album"),
            verdict: "{}".to_string(),
            probed_total_duration_ms: 0,
            expected_edit_revision: 7,
            identity_pick: None,
        })
        .await
        .unwrap());
    assert!(!db
        .remove_watched_import_folder("/nothing/watches/this")
        .await
        .unwrap());

    assert_eq!(
        tripwire_hits(),
        0,
        "a writer with nothing to write opened a journaled transaction anyway; \
         captured warnings:\n{}",
        String::from_utf8(buf.lock().unwrap().clone()).unwrap(),
    );
}
