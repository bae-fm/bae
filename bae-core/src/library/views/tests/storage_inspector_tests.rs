//! The Storage Manager inspector's read of one release.

use crate::library::output_snapshot::{OutputKind, OutputRequest};
use crate::library::queued_releases::tables_tests::{download, enqueue_releases};
use crate::library::release_queue::ReleaseQueueEntry;
use bae_db::test_fixtures::*;

/// The inspector's read of one release holds its entry in each queue and
/// nothing of another release's, and reads no release the library does not
/// hold.
#[cfg(desktop)]
#[tokio::test]
async fn the_inspectors_read_holds_the_releases_own_entries() {
    let (db, _tmp) = temp_db().await;
    db.call(|sql| {
        seed_artist_album_release(sql)?;
        Ok(())
    })
    .await
    .unwrap();
    enqueue_releases(&db, vec![download(RELEASE_A), download(RELEASE_B)]).await.unwrap();
    enqueue_releases(
        &db,
        vec![ReleaseQueueEntry {
            release_id: RELEASE_B.to_string(),
            created_at: 3,
            payload: OutputRequest {
                target_dir: "/Users/ana/Music/Copies".into(),
                kind: OutputKind::Export,
            },
        }],
    )
    .await
    .unwrap();

    let mut live = crate::library::views::LibraryViews::new(db.clone())
        .subscribe_storage_inspector(Some(RELEASE_B.to_string()));
    let read = live.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(read, None, "the library holds no RELEASE_B");

    live.requests().set(Some(RELEASE_A.to_string())).expect("the query is open");
    let read = loop {
        let event = live.next().await.expect("the query stays open");
        if event.request.as_deref() == Some(RELEASE_A) {
            break event.result.unwrap().expect("the library holds RELEASE_A");
        }
    };
    assert_eq!(read.entries.download.map(|row| row.op.release_id).as_deref(), Some(RELEASE_A));
    assert!(read.entries.output.is_none(), "only RELEASE_B is exporting");
}
