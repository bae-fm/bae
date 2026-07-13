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
            sql.tx()
                .query_row("SELECT 1", [], |_row| Ok(()))
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
    // new semantics: `add_cloud_outbox_delete` changes rows, so coven's
    // zero-rows-changed check does not fire. This is why device-local writes
    // (e.g. once-per-second playback_state) stay silent with no `sql_local`.
    db.add_cloud_outbox_delete("tripwire/local-write")
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
    db.get_release_storage_summaries().await.unwrap();
    db.get_release_identities("missing").await.unwrap(); // identity.rs
    db.load_playback_state().await.unwrap(); // playback.rs
    db.external_blob("missing").await.unwrap(); // blobs.rs
    db.outbox_items().await.unwrap();

    assert_eq!(
        tripwire_hits(),
        0,
        "a pure read still journals through `sql`, or a device-local write \
         changed no rows; captured warnings:\n{}",
        String::from_utf8(buf.lock().unwrap().clone()).unwrap(),
    );
}
