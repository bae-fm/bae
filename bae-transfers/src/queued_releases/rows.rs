//! The queues' read fragments and the reads they compose: each entry's row
//! beside its release's album title, how many entries a queue holds in each
//! phase, and whether it is paused. The downloads list's window is in
//! [`super::download_list`].

use super::tables::{download_request, QueuedPayload};
use crate::download_snapshot::DownloadRequest;
use crate::release_queue::{
    QueueWait, QueuedRelease, ReleaseQueueCounts, ReleaseQueueEntry, ReleaseQueueOp,
    ReleaseQueueProgress, ReleaseQueueRow, ReleaseQueueState,
};
use bae_records::records::fragments::{AlbumTitle, ReleaseAlbum};
use bae_store::ReadContext;
use bae_store::{statement_row, DbError};

// Each entry of the download queue, with its place in the order queued.
coven::fragment!(
    DownloadQueueRow { file_count: u64, total_size: u64, kind: crate::DownloadKind },
    r#"
    SELECT kept.position, kept.release_id, kept.created_at, kept.file_count, kept.total_size,
           kept.state, kept.error, kept.kind, kept.target_dir
    FROM download_queue AS kept
"#
);

// Each entry of the export and save queue, with its place in the order queued.
coven::fragment!(
    OutputQueueRow { file_count: u64, total_size: u64 },
    r#"
    SELECT kept.position, kept.release_id, kept.created_at, kept.file_count, kept.total_size,
           kept.state, kept.error, kept.target_dir, kept.kind, kept.preset,
           kept.filename_pattern, kept.folder_pattern, kept.extras
    FROM output_queue AS kept
"#
);

// How many entries of the export and save queue are in each phase, and
// whether it holds any entry and any failed one.
coven::fragment!(
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

// Whether coven says the device has a usable network.
coven::read!(
    writer CurrentNetwork,
    r#"
    SELECT network.*
    FROM @crate::queue_counts::NetworkUsable AS network
"#
);

// What the download worker looks at before each entry: the network, and the
// entry it takes next, unless the queue is paused or coven says the device has
// no network. It takes the active entry if there is one, else the first
// queued. A network coven cannot tell about does not hold the queue.
coven::read!(
    writer NextDownload,
    r#"
    SELECT network.*, entry.*
    FROM @crate::queue_counts::NetworkUsable AS network
    LEFT JOIN @crate::queued_releases::rows::DownloadQueueRow AS entry
           ON entry.release_id = (
               SELECT waiting.release_id FROM download_queue AS waiting
               WHERE waiting.state IN ('active', 'queued')
                 AND NOT (SELECT pause.paused FROM release_queues AS pause
                          WHERE pause.queue = 'download')
                 AND network.network_usable IS NOT 0
               ORDER BY waiting.state = 'active' DESC, waiting.position LIMIT 1)
"#
);

// The entry the export and save worker takes next, unless the queue is paused:
// the active entry if there is one, else the first queued.
coven::read!(
    writer NextOutput,
    r#"
    SELECT entry.*
    FROM @crate::queued_releases::rows::OutputQueueRow AS entry
    WHERE entry.release_id = (
        SELECT waiting.release_id FROM output_queue AS waiting
        WHERE waiting.state IN ('active', 'queued')
          AND NOT (SELECT pause.paused FROM release_queues AS pause
                   WHERE pause.queue = 'output')
        ORDER BY waiting.state = 'active' DESC, waiting.position LIMIT 1)
"#
);

coven::read!(
    writer DownloadQueueTotals,
    r#"
    SELECT counts.*
    FROM @crate::queue_counts::StoredDownloadCounts AS counts
"#
);

// The export and save queue's counts and its pause.
coven::read!(
    writer OutputQueueTotals,
    r#"
    SELECT tally.*, pause.*
    FROM @crate::queued_releases::rows::OutputQueueTally AS tally
    JOIN @crate::queue_counts::QueuePause AS pause ON pause.queue = 'output'
"#
);

// Every entry of the export and save queue in the order queued, each beside
// its album's title.
#[cfg(any(desktop, test, feature = "test-utils"))]
coven::read!(
    writer OutputQueueEntries,
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

#[cfg(desktop)]
impl crate::release_queue::ReleaseQueueEntries {
    /// A release's entries in the download and export queues, from each
    /// queue's row of it, beside the release and its album's title.
    pub fn of_rows(
        download: Option<DownloadQueueRow>,
        output: Option<OutputQueueRow>,
        release: &ReleaseAlbum,
        album: &AlbumTitle,
    ) -> Result<Self, DbError> {
        Ok(Self {
            download: download
                .map(|row| row.into_row(Some(release.clone()), Some(album.clone())))
                .transpose()?,
            output: output
                .map(|row| row.into_row(Some(release.clone()), Some(album.clone())))
                .transpose()?,
        })
    }
}

/// What the worker finds as it looks for its next entry.
pub struct NextUp<P: QueuedPayload> {
    /// The entry to take, if the queue has one it may take now.
    pub(crate) entry: Option<ReleaseQueueEntry<P>>,
    /// What the look saw of the network.
    pub(crate) network: P::Network,
}

/// Why the stored entry of `release_id` can't be read.
fn unreadable_entry(release_id: &str, problem: String) -> DbError {
    DbError::unreadable(format!("the queue entry of release {release_id}"), problem)
}

pub(super) fn next_download_on(sql: &ReadContext<'_>) -> Result<NextUp<DownloadRequest>, DbError> {
    let row = statement_row(sql, &NextDownloadParams {})?;
    Ok(NextUp {
        entry: row.entry.map(DownloadQueueRow::into_entry).transpose()?,
        network: row.network.network_usable,
    })
}

pub(super) fn next_output_on(
    sql: &ReadContext<'_>,
) -> Result<NextUp<crate::OutputRequest>, DbError> {
    let entry = bae_store::statement_optional(sql, &NextOutputParams {})?
        .map(|row| row.entry.into_entry())
        .transpose()?;
    Ok(NextUp { entry, network: () })
}

/// Whether coven says the device has a usable network; `None` where nothing
/// tells.
pub(super) fn subscribe_network_usable(
    store: &bae_store::Store,
) -> coven::LiveQuery<Option<bool>, DbError> {
    store.subscribe_row(CurrentNetworkParams {}).process(|row| Ok(row.network.network_usable))
}

/// What every queue's row stores of an entry, whatever the entry carries.
struct StoredEntry {
    release_id: String,
    created_at: i64,
    file_count: u64,
    total_size: u64,
    state: String,
    error: Option<String>,
}

impl StoredEntry {
    /// The entry as a reader of its queue shows it, with `payload` what its
    /// queue's own columns say, beside its release.
    fn into_row<P>(
        self,
        payload: Result<P, String>,
        release: Option<ReleaseAlbum>,
        album: Option<AlbumTitle>,
    ) -> Result<ReleaseQueueRow<P, ()>, DbError> {
        let unreadable = |problem| unreadable_entry(&self.release_id, problem);
        let payload = payload.map_err(unreadable)?;
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

impl DownloadQueueRow {
    /// The entry as the worker takes it.
    fn into_entry(self) -> Result<ReleaseQueueEntry<DownloadRequest>, DbError> {
        let target_dir = self.target_dir.map(bae_store::path_column::decode).transpose()?;
        let payload = download_request(self.kind, target_dir)
            .map_err(|problem| unreadable_entry(&self.release_id, problem))?;
        Ok(ReleaseQueueEntry { release_id: self.release_id, created_at: self.created_at, payload })
    }

    /// The entry as the pane shows it, beside its release.
    pub(super) fn into_row(
        self,
        release: Option<ReleaseAlbum>,
        album: Option<AlbumTitle>,
    ) -> Result<ReleaseQueueRow<DownloadRequest, ()>, DbError> {
        let target_dir = self.target_dir.map(bae_store::path_column::decode).transpose()?;
        let payload = download_request(self.kind, target_dir);
        StoredEntry {
            release_id: self.release_id,
            created_at: self.created_at,
            file_count: self.file_count,
            total_size: self.total_size,
            state: self.state,
            error: self.error,
        }
        .into_row(payload, release, album)
    }
}

impl OutputQueueRow {
    /// The entry as the worker takes it.
    fn into_entry(self) -> Result<ReleaseQueueEntry<crate::OutputRequest>, DbError> {
        let payload = super::tables::output_request(
            bae_store::path_column::decode(self.target_dir)?,
            &self.kind,
            [self.preset, self.filename_pattern, self.folder_pattern, self.extras],
        )
        .map_err(|problem| unreadable_entry(&self.release_id, problem))?;
        Ok(ReleaseQueueEntry { release_id: self.release_id, created_at: self.created_at, payload })
    }

    /// The entry as a reader of the queue shows it, beside its release.
    #[cfg(any(desktop, test, feature = "test-utils"))]
    fn into_row(
        self,
        release: Option<ReleaseAlbum>,
        album: Option<AlbumTitle>,
    ) -> Result<ReleaseQueueRow<crate::OutputRequest, ()>, DbError> {
        let payload = super::tables::output_request(
            bae_store::path_column::decode(self.target_dir)?,
            &self.kind,
            [self.preset, self.filename_pattern, self.folder_pattern, self.extras],
        );
        StoredEntry {
            release_id: self.release_id,
            created_at: self.created_at,
            file_count: self.file_count,
            total_size: self.total_size,
            state: self.state,
            error: self.error,
        }
        .into_row(payload, release, album)
    }
}

/// How many entries the download queue holds in each phase, and what holds it
/// back.
pub(super) fn download_counts_on(sql: &ReadContext<'_>) -> Result<ReleaseQueueCounts, DbError> {
    Ok(statement_row(sql, &DownloadQueueTotalsParams {})?.counts.into_counts())
}

/// Follow how many downloads are in each phase, and whether the queue is
/// paused.
pub(super) fn subscribe_download_counts(
    store: &bae_store::Store,
) -> coven::LiveQuery<ReleaseQueueCounts, DbError> {
    store.subscribe_row(DownloadQueueTotalsParams {}).process(|row| Ok(row.counts.into_counts()))
}

/// How many entries the export and save queue holds in each phase, and
/// whether it is paused: it needs no network.
pub(super) fn output_counts_on(sql: &ReadContext<'_>) -> Result<ReleaseQueueCounts, DbError> {
    Ok(output_counts_of(statement_row(sql, &OutputQueueTotalsParams {})?))
}

/// The export and save queue's counts as their row holds them.
pub(super) fn output_counts_of(row: OutputQueueTotalsRow) -> ReleaseQueueCounts {
    let OutputQueueTotalsRow { tally, pause } = row;
    ReleaseQueueCounts {
        total: ReleaseQueueProgress {
            queued: tally.queued,
            active: tally.active,
            failed: tally.failed,
        },
        has_entries: tally.has_entries,
        has_failures: tally.has_failures,
        wait: QueueWait::of(pause.paused, None),
    }
}

/// Every entry of the export and save queue, in the order queued.
#[cfg(any(desktop, test, feature = "test-utils"))]
pub(super) fn output_rows_on(
    sql: &ReadContext<'_>,
) -> Result<Vec<ReleaseQueueRow<crate::OutputRequest, ()>>, DbError> {
    output_rows_of(bae_store::statement_rows(sql, &OutputQueueEntriesParams {})?)
}

/// The export and save queue's entries as their read returned them.
#[cfg(any(desktop, test, feature = "test-utils"))]
pub(super) fn output_rows_of(
    rows: Vec<OutputQueueEntriesRow>,
) -> Result<Vec<ReleaseQueueRow<crate::OutputRequest, ()>>, DbError> {
    rows.into_iter().map(|row| row.entry.into_row(row.release, row.album)).collect()
}
