//! The transfer screens' plain reads: the download and output queues' windows
//! and counts, and the activity's bytes to send.

use super::seed::{sizes, SEEDS};
use bae_store::test_fixtures::screen_cost::{list_costs, plain_cost, size_name, ListShape};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn transfers_plain_reads() {
    for size in sizes() {
        let (db, _copy) = SEEDS.seeded_copy(size).await;
        let at = size_name(size);
        list_costs(
            &db,
            size,
            &ListShape {
                metric: "downloads/queued",
                select: "q.position, q.release_id, q.state, q.error, a.title",
                source: "FROM download_queue q JOIN releases r ON r.id = q.release_id \
                         JOIN albums a ON a.id = r.album_id WHERE 1",
                key: "q.position",
                tie: "q.release_id",
                descending: false,
            },
        )
        .await;
        let counts = "SELECT COUNT(*), COUNT(*) FILTER (WHERE state = 'failed'), \
                      EXISTS (SELECT 1 FROM download_queue) FROM download_queue";
        plain_cost(&db, &format!("downloads/counts@{at}"), Some(2.0), counts, vec![]).await;
        let outputs = "SELECT q.position, q.release_id, q.state, a.title \
                       FROM output_queue q JOIN releases r ON r.id = q.release_id \
                       JOIN albums a ON a.id = r.album_id ORDER BY q.position";
        let rows =
            plain_cost(&db, &format!("outputs/queued@{at}"), Some(2.0), outputs, vec![]).await;
        assert!(rows > 0);
        // The bytes a sync waits to send: the files in the upload queue, per
        // release.
        let sending = "SELECT f.release_id, COUNT(*), SUM(coven.size) \
                       FROM coven_upload_queue q \
                       JOIN release_files f ON f.file = q.file \
                       JOIN coven_files coven ON coven.id = q.file \
                       GROUP BY f.release_id";
        let rows =
            plain_cost(&db, &format!("activity/upload-bytes@{at}"), Some(2.0), sending, vec![])
                .await;
        assert!(rows > 0, "files wait to upload");
    }
}
