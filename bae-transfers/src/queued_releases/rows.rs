//! The queues' read fragments and the reads they compose: each entry's row
//! beside its release's album title, how many entries a queue holds in each
//! phase, and whether it is paused. The downloads list's window is in
//! [`super::download_list`].

use super::tables::download_request;
use crate::download_snapshot::DownloadRequest;
use crate::release_queue::{
    QueueWait, QueuedRelease, ReleaseQueueCounts, ReleaseQueueOp, ReleaseQueueProgress,
    ReleaseQueueRow, ReleaseQueueState,
};
use bae_records::records::fragments::{AlbumTitle, ReleaseAlbum};
use bae_store::{statement_row, DbError};
use coven::SqlReadContext;

// Each entry of the download queue, with its place in the order queued.
sqlive_compose::fragment!(
    DownloadQueueRow { file_count: u64, total_size: u64, kind: crate::DownloadKind },
    r#"
    SELECT kept.position, kept.release_id, kept.created_at, kept.file_count, kept.total_size,
           kept.state, kept.error, kept.kind, kept.target_dir
    FROM download_queue AS kept
"#
);

// Each entry of the export and save queue, with its place in the order queued.
#[cfg(any(desktop, test, feature = "test-utils"))]
sqlive_compose::fragment!(
    OutputQueueRow { file_count: u64, total_size: u64 },
    r#"
    SELECT kept.position, kept.release_id, kept.created_at, kept.file_count, kept.total_size,
           kept.state, kept.error, kept.target_dir, kept.kind, kept.preset,
           kept.filename_pattern, kept.folder_pattern, kept.extras
    FROM output_queue AS kept
"#
);

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

// [`DownloadQueueTally`] for the export and save queue.
sqlive_compose::fragment!(
    OutputQueueTally {
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
    FROM output_queue AS kept
"#
);

// The download queue's counts, its pause, and whether coven says the device
// has a usable network, which is NULL where nothing tells.
sqlive_compose::read!(
    DownloadQueueTotals { network_usable: Option<bool> },
    r#"
    SELECT tally.*, pause.*, (SELECT usable FROM coven_network) AS network_usable
    FROM @crate::queued_releases::rows::DownloadQueueTally AS tally
    JOIN @crate::queued_releases::rows::QueuePause AS pause ON pause.queue = 'download'
"#
);

// The export and save queue's counts and its pause.
sqlive_compose::read!(
    OutputQueueTotals,
    r#"
    SELECT tally.*, pause.*
    FROM @crate::queued_releases::rows::OutputQueueTally AS tally
    JOIN @crate::queued_releases::rows::QueuePause AS pause ON pause.queue = 'output'
"#
);

// The download queue's entry for one release, beside its album's title.
#[cfg(desktop)]
sqlive_compose::read!(
    DownloadOfRelease(release: String),
    r#"
    SELECT entry.*, release.*, album.*
    FROM @crate::queued_releases::rows::DownloadQueueRow AS entry
    LEFT JOIN @bae_records::records::fragments::ReleaseAlbum AS release
           ON release.release_id = entry.release_id
    LEFT JOIN @bae_records::records::fragments::AlbumTitle AS album
           ON album.album_id = release.album_id
    WHERE entry.release_id = :release
"#
);

// The export and save queue's entry for one release, beside its album's title.
#[cfg(desktop)]
sqlive_compose::read!(
    OutputOfRelease(release: String),
    r#"
    SELECT entry.*, release.*, album.*
    FROM @crate::queued_releases::rows::OutputQueueRow AS entry
    LEFT JOIN @bae_records::records::fragments::ReleaseAlbum AS release
           ON release.release_id = entry.release_id
    LEFT JOIN @bae_records::records::fragments::AlbumTitle AS album
           ON album.album_id = release.album_id
    WHERE entry.release_id = :release
"#
);

// Every entry of the export and save queue in the order queued, each beside
// its album's title.
#[cfg(any(desktop, test, feature = "test-utils"))]
sqlive_compose::read!(
    OutputQueueEntries,
    r#"
    SELECT entry.*, release.*, album.*
    FROM @crate::queued_releases::rows::OutputQueueRow AS entry
    LEFT JOIN @bae_records::records::fragments::ReleaseAlbum AS release
           ON release.release_id = entry.release_id
    LEFT JOIN @bae_records::records::fragments::AlbumTitle AS album
           ON album.album_id = release.album_id
    ORDER BY entry.position
"#
);

/// An entry's phase from its stored `state` and `error`, or what of them no
/// writer here stores together.
fn phase(state: &str, error: Option<String>) -> Result<ReleaseQueueState<()>, String> {
    match (state, error) {
        ("queued", None) => Ok(ReleaseQueueState::Queued),
        ("active", None) => Ok(ReleaseQueueState::Active { progress: () }),
        ("failed", Some(error)) => Ok(ReleaseQueueState::Failed { error }),
        (state, error) => Err(format!("queue state {state:?} with error {error:?}")),
    }
}

/// What a queue's pane shows of a release: nothing once the library no longer
/// holds it. Every release the library holds is filed under an album it holds.
fn queued_release(
    release_id: &str,
    release: Option<ReleaseAlbum>,
    album: Option<AlbumTitle>,
) -> Result<Option<QueuedRelease>, DbError> {
    match (release, album) {
        (None, _) => Ok(None),
        (Some(_), Some(album)) => Ok(Some(QueuedRelease { title: album.into_title() })),
        (Some(_), None) => Err(DbError::unreadable(
            format!("the album of queued release {release_id}"),
            None::<String>,
        )),
    }
}

impl DownloadQueueRow {
    /// The entry as the pane shows it, beside its release.
    pub(super) fn into_row(
        self,
        release: Option<ReleaseAlbum>,
        album: Option<AlbumTitle>,
    ) -> Result<ReleaseQueueRow<DownloadRequest, ()>, DbError> {
        let unreadable = |problem: String| {
            DbError::unreadable(format!("the download of release {}", self.release_id), problem)
        };
        let target_dir = self.target_dir.map(bae_store::path_column::decode).transpose()?;
        let payload = download_request(self.kind, target_dir).map_err(unreadable)?;
        let state = phase(&self.state, self.error).map_err(unreadable)?;
        let release = queued_release(&self.release_id, release, album)?;
        Ok(ReleaseQueueRow {
            op: ReleaseQueueOp {
                release_id: self.release_id,
                created_at: self.created_at,
                file_count: self.file_count,
                total_size: self.total_size,
                payload,
                state,
            },
            release,
        })
    }
}

#[cfg(any(desktop, test, feature = "test-utils"))]
impl OutputQueueRow {
    /// The entry as a reader of the queue shows it, beside its release.
    fn into_row(
        self,
        release: Option<ReleaseAlbum>,
        album: Option<AlbumTitle>,
    ) -> Result<ReleaseQueueRow<crate::OutputRequest, ()>, DbError> {
        let unreadable = |problem: String| {
            DbError::unreadable(format!("the output of release {}", self.release_id), problem)
        };
        let payload = super::tables::output_request(
            bae_store::path_column::decode(self.target_dir)?,
            &self.kind,
            [self.preset, self.filename_pattern, self.folder_pattern, self.extras],
        )
        .map_err(unreadable)?;
        let state = phase(&self.state, self.error).map_err(unreadable)?;
        let release = queued_release(&self.release_id, release, album)?;
        Ok(ReleaseQueueRow {
            op: ReleaseQueueOp {
                release_id: self.release_id,
                created_at: self.created_at,
                file_count: self.file_count,
                total_size: self.total_size,
                payload,
                state,
            },
            release,
        })
    }
}

/// How many entries the download queue holds in each phase, and what holds it
/// back.
pub(super) fn download_counts_on(sql: &SqlReadContext<'_>) -> Result<ReleaseQueueCounts, DbError> {
    let DownloadQueueTotalsRow { tally, pause, network_usable } =
        statement_row(sql, &DownloadQueueTotalsParams {})?;
    Ok(ReleaseQueueCounts {
        total: ReleaseQueueProgress {
            queued: tally.queued,
            active: tally.active,
            failed: tally.failed,
        },
        has_entries: tally.has_entries,
        has_failures: tally.has_failures,
        wait: QueueWait::of(pause.paused, network_usable),
    })
}

/// How many entries the export and save queue holds in each phase, and
/// whether it is paused: it needs no network.
pub(super) fn output_counts_on(sql: &SqlReadContext<'_>) -> Result<ReleaseQueueCounts, DbError> {
    let OutputQueueTotalsRow { tally, pause } = statement_row(sql, &OutputQueueTotalsParams {})?;
    Ok(ReleaseQueueCounts {
        total: ReleaseQueueProgress {
            queued: tally.queued,
            active: tally.active,
            failed: tally.failed,
        },
        has_entries: tally.has_entries,
        has_failures: tally.has_failures,
        wait: QueueWait::of(pause.paused, None),
    })
}

/// `release_id`'s entry of the download queue.
#[cfg(desktop)]
pub(super) fn download_of_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseQueueRow<DownloadRequest, ()>>, DbError> {
    bae_store::statement_optional(
        sql,
        &DownloadOfReleaseParams { release: release_id.to_string() },
    )?
    .map(|row| row.entry.into_row(row.release, row.album))
    .transpose()
}

/// `release_id`'s entry of the export and save queue.
#[cfg(desktop)]
pub(super) fn output_of_release_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<ReleaseQueueRow<crate::OutputRequest, ()>>, DbError> {
    bae_store::statement_optional(sql, &OutputOfReleaseParams { release: release_id.to_string() })?
        .map(|row| row.entry.into_row(row.release, row.album))
        .transpose()
}

/// Every entry of the export and save queue, in the order queued.
#[cfg(any(desktop, test, feature = "test-utils"))]
pub(super) fn output_rows_on(
    sql: &SqlReadContext<'_>,
) -> Result<Vec<ReleaseQueueRow<crate::OutputRequest, ()>>, DbError> {
    bae_store::statement_rows(sql, &OutputQueueEntriesParams {})?
        .into_iter()
        .map(|row| row.entry.into_row(row.release, row.album))
        .collect()
}
