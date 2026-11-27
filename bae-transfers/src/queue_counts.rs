//! Each queue's pause, and what the download queue's counts are read from. A
//! composed read spreads [`StoredDownloadCounts`] to count the queue in its
//! own statement.

use crate::release_queue::{QueueWait, ReleaseQueueCounts, ReleaseQueueProgress};

// Whether the person paused each queue.
sqlive_compose::fragment!(
    QueuePause { paused: bool },
    r#"
    SELECT kept.queue, kept.paused
    FROM release_queues AS kept
"#
);

// How many entries of the download queue are in each phase, and whether it
// holds any entry and any failed one.
sqlive_compose::fragment!(
    DownloadQueueTally {
        queued: u32,
        active: u32,
        failed: u32,
        has_entries: bool,
        has_failures: bool,
    },
    r#"
    SELECT COUNT(*) FILTER (WHERE kept.state = 'queued') AS queued,
           COUNT(*) FILTER (WHERE kept.state = 'active') AS active,
           COUNT(*) FILTER (WHERE kept.state = 'failed') AS failed,
           COUNT(*) > 0 AS has_entries,
           COUNT(*) FILTER (WHERE kept.state = 'failed') > 0 AS has_failures
    FROM download_queue AS kept
"#
);

// The download queue's counts, its pause, and whether coven says the device
// has a usable network, which is NULL where nothing tells.
sqlive_compose::fragment!(
    StoredDownloadCounts { network_usable: Option<bool> },
    r#"
    SELECT tally.*, pause.*, (SELECT usable FROM coven_network) AS network_usable
    FROM @crate::queue_counts::DownloadQueueTally AS tally
    JOIN @crate::queue_counts::QueuePause AS pause ON pause.queue = 'download'
"#
);

impl StoredDownloadCounts {
    /// How many entries the download queue holds in each phase, and what
    /// holds it back.
    pub fn into_counts(self) -> ReleaseQueueCounts {
        let Self { tally, pause, network_usable } = self;
        ReleaseQueueCounts {
            total: ReleaseQueueProgress {
                queued: tally.queued,
                active: tally.active,
                failed: tally.failed,
            },
            has_entries: tally.has_entries,
            has_failures: tally.has_failures,
            wait: QueueWait::of(pause.paused, network_usable),
        }
    }
}
