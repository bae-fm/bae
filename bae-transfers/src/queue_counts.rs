//! Each queue's pause, and what the download queue's counts are read from. A
//! composed read spreads [`StoredDownloadCounts`] to count the queue in its
//! own statement.

use crate::release_queue::{QueueWait, ReleaseQueueCounts, ReleaseQueueProgress};

// Whether the person paused each queue.
coven::fragment!(
    QueuePause { paused: bool },
    r#"
    SELECT kept.queue, kept.paused
    FROM release_queues AS kept
"#
);

// How many entries of the download queue are in each phase, and whether it
// holds any entry and any failed one.
coven::fragment!(
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

// Whether coven says the device has a usable network: one row, NULL where
// nothing tells.
coven::fragment!(
    NetworkUsable { network_usable: Option<bool> },
    r#"
    SELECT (SELECT usable FROM coven_network) AS network_usable
"#
);

// What the download queue is doing: whether it holds an entry waiting or
// running, whether the person paused it, the release it runs, and whether
// coven says the device has a usable network.
coven::fragment!(
    DownloadQueueWork {
        download_working: bool,
        download_paused: bool,
        download_network: Option<bool>,
    },
    r#"
    SELECT EXISTS (SELECT 1 FROM download_queue AS held
                   WHERE held.state IN ('queued', 'active')) AS download_working,
           pause.paused AS download_paused,
           (SELECT running.release_id FROM download_queue AS running
            WHERE running.state = 'active') AS download_active,
           network.network_usable AS download_network
    FROM @crate::queue_counts::QueuePause AS pause
    JOIN @crate::queue_counts::NetworkUsable AS network ON 1
    WHERE pause.queue = 'download'
"#
);

impl DownloadQueueWork {
    /// While the queue holds work: what holds it back, and the release it
    /// runs.
    pub fn into_run(self) -> Option<(Option<QueueWait>, Option<String>)> {
        self.download_working.then(|| {
            (QueueWait::of(self.download_paused, self.download_network), self.download_active)
        })
    }
}

// What the export and save queue is doing, as [`DownloadQueueWork`] reads the
// download queue; it needs no network.
coven::fragment!(
    OutputQueueWork { output_working: bool, output_paused: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM output_queue AS held
                   WHERE held.state IN ('queued', 'active')) AS output_working,
           pause.paused AS output_paused,
           (SELECT running.release_id FROM output_queue AS running
            WHERE running.state = 'active') AS output_active
    FROM @crate::queue_counts::QueuePause AS pause
    WHERE pause.queue = 'output'
"#
);

impl OutputQueueWork {
    /// While the queue holds work: what holds it back, and the release it
    /// runs.
    pub fn into_run(self) -> Option<(Option<QueueWait>, Option<String>)> {
        self.output_working.then(|| (QueueWait::of(self.output_paused, None), self.output_active))
    }
}

// The download queue's counts, its pause, and the network.
coven::fragment!(
    StoredDownloadCounts,
    r#"
    SELECT tally.*, pause.*, network.*
    FROM @crate::queue_counts::DownloadQueueTally AS tally
    JOIN @crate::queue_counts::QueuePause AS pause ON pause.queue = 'download'
    JOIN @crate::queue_counts::NetworkUsable AS network ON 1
"#
);

impl StoredDownloadCounts {
    /// How many entries the download queue holds in each phase, and what
    /// holds it back.
    pub fn into_counts(self) -> ReleaseQueueCounts {
        let Self { tally, pause, network } = self;
        ReleaseQueueCounts {
            total: ReleaseQueueProgress {
                queued: tally.queued,
                active: tally.active,
                failed: tally.failed,
            },
            has_entries: tally.has_entries,
            has_failures: tally.has_failures,
            wait: QueueWait::of(pause.paused, network.network_usable),
        }
    }
}
