//! Unified data source abstraction for audio playback.
//!
//! Provides a common interface for reading audio bytes from:
//! - Local files (non-storage releases, or storage releases with local backend)
//! - Cloud storage (storage releases with cloud backend)

use crate::playback::progress::{emit_progress, PlaybackProgress};
use crate::playback::sparse_buffer::{ReaderDemand, SharedSparseBuffer, SparseStreamingBuffer};
use std::sync::{Arc, Weak};
use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::Notify;
use tracing::{debug, error, info};

/// Reads audio data into a sparse buffer for streaming playback.
///
/// Local reads serve plaintext bytes from disk; cloud reads apply the home's
/// at-rest cipher to each blob — decrypting under the library key on an opaque
/// home, or reading the verbatim bytes on a browsable one.
pub trait AudioDataReader: Send + 'static {
    /// Start reading data into the buffer.
    ///
    /// Spawns an async task that follows the readers: it fetches the window each
    /// reader needs next (read-ahead), evicts what they have passed, and idles
    /// when every read-ahead window is full -- so playback starts after one
    /// window and memory stays bounded to a window around the playhead instead
    /// of the whole file. On an opaque home the cloud reader decrypts each
    /// window as it lands; on a browsable one it reads the bytes verbatim.
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    );
}

/// Configuration for reading audio data.
#[derive(Debug, Clone)]
pub struct AudioReadConfig {
    /// Path to the audio file (local path or cloud key)
    pub path: String,
    pub source_size: u64,
}

/// Reads from local filesystem.
///
/// Used for:
/// - Non-storage releases (files at original import location)
/// - Storage releases with local backend
pub struct LocalReader {
    config: AudioReadConfig,
}

impl LocalReader {
    pub fn new(config: AudioReadConfig) -> Self {
        Self { config }
    }
}

fn fail_audio_read(
    progress_tx: &tokio_mpsc::UnboundedSender<PlaybackProgress>,
    buffer: &SharedSparseBuffer,
    log_message: String,
    message: String,
) {
    // A buffer that is already cancelled means teardown ran before this read
    // returned: the user switched away while a reader was parked in an in-flight
    // `read().await` past its top-of-loop cancel check. The Err it eventually
    // returns belongs to the abandoned track. Emitting PlaybackError here would
    // halt the track the user just started, so suppress it and exit cancelled.
    if buffer.is_cancelled() {
        debug!("ignoring read failure on a cancelled buffer: {log_message}");
        return;
    }
    error!("{}", log_message);
    emit_progress(
        progress_tx,
        PlaybackProgress::PlaybackError {
            reason: crate::ui::PlaybackErrorReason::internal(message),
        },
    );
    buffer.cancel();
}

/// Report a fill failure, upgrading the fill's `Weak` to reach the buffer. If
/// the buffer is already gone, the track was torn down before this error
/// surfaced -- there's nothing to halt, so just log and drop it.
fn report_fill_failure(
    progress_tx: &tokio_mpsc::UnboundedSender<PlaybackProgress>,
    buffer: &Weak<SparseStreamingBuffer>,
    log_message: String,
    message: String,
) {
    match buffer.upgrade() {
        Some(buf) => fail_audio_read(progress_tx, &buf, log_message, message),
        None => debug!("ignoring fill failure on a dropped buffer: {log_message}"),
    }
}

/// Hand back the fill-wake signal and a weak buffer ref, consuming the caller's
/// strong ref. The fill follows the buffer's real users (decoder + cache) via
/// the `Weak` and must not keep the buffer alive itself, so the strong ref is
/// dropped here. Both readers prepare their fill this way. The file size is
/// already set at the buffer's construction, so there's nothing else to wire.
fn prepare_fill(buffer: SharedSparseBuffer) -> (Arc<Notify>, Weak<SparseStreamingBuffer>) {
    let wake = buffer.fill_wake_handle();
    (wake, Arc::downgrade(&buffer))
}

impl AudioDataReader for LocalReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    ) {
        // The file size is recorded at the buffer's construction; this reader
        // only needs the path (the fill reads bytes through `fetch`).
        let AudioReadConfig { path, .. } = self.config;
        let (wake, buffer) = prepare_fill(buffer);

        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};

            let file = match tokio::fs::File::open(&path).await {
                Ok(f) => f,
                Err(e) => {
                    let message = format!("Failed to open file {path}: {e}");
                    report_fill_failure(&progress_tx, &buffer, message.clone(), message);
                    return;
                }
            };
            // One handle, seeked per fetch. The fill loop awaits each fetch in
            // turn, so the lock is never actually contended.
            let file = tokio::sync::Mutex::new(file);

            let result = fill_buffer_on_demand(buffer.clone(), wake, |src_off, len| {
                let file = &file;
                async move {
                    let mut f = file.lock().await;
                    f.seek(std::io::SeekFrom::Start(src_off))
                        .await
                        .map_err(|e| format!("seek to {src_off}: {e}"))?;
                    let mut buf = vec![0u8; len as usize];
                    f.read_exact(&mut buf)
                        .await
                        .map_err(|e| format!("read {len} at {src_off}: {e}"))?;
                    Ok(buf)
                }
            })
            .await;

            if let Err(e) = result {
                let message = format!("Read error in file {path}: {e}");
                report_fill_failure(&progress_tx, &buffer, message.clone(), message);
            }
        });
    }
}

/// Create the appropriate audio reader based on where the track data lives.
///
/// Reads a local file when `source` resolves one (this device's copy, or a
/// still-pending upload's original). A pending upload whose source is gone
/// errors before any cloud read — the object may not exist yet. A `Remote`
/// source is a remote track read through the home's at-rest cipher (decrypt on
/// an opaque home, verbatim on a browsable one), or report sync disconnected if
/// no cloud home is configured. An `Unreachable` source is a local track
/// whose local file is gone — there is nowhere to read it.
pub fn create_audio_reader(
    source: crate::library::manager::ReadableFileSource,
    file_id: &str,
    cloud_key: &str,
    library_manager: &crate::library::LibraryManager,
    make_read_config: impl FnOnce(String) -> AudioReadConfig,
) -> Result<Box<dyn AudioDataReader>, crate::playback::PlaybackError> {
    use crate::library::manager::ReadableFileSource;
    use crate::playback::PlaybackError;

    match source {
        ReadableFileSource::Local(local_path) => {
            let read_config = make_read_config(local_path.display().to_string());
            Ok(Box::new(LocalReader::new(read_config)))
        }
        ReadableFileSource::UploadPendingSourceMissing => Err(PlaybackError::UploadPending),
        ReadableFileSource::Remote => {
            // A remote track's audio is a cloud-only object sealed under the
            // home's at-rest cipher: read it through that cipher where the read
            // needs the home and cipher, or report sync disconnected if no cloud
            // home is configured. An opaque home's cipher needs the library key;
            // a remote release always has an unlocked library, so a missing
            // cipher there is a broken invariant, surfaced as an error rather
            // than masked. A browsable home's cipher is plaintext and always
            // present. The object key is the resolved `cloud_key` (the row's
            // readable `cloud_path`, or the hashed `storage_path` default).
            if let Some(cloud_home) = library_manager.get_cloud_home() {
                let cipher = library_manager.cloud_blob_cipher().ok_or_else(|| {
                    PlaybackError::not_found("blob cipher for remote cloud file", file_id)
                })?;
                let read_config = make_read_config(cloud_key.to_string());
                Ok(Box::new(CloudReader::new(read_config, cloud_home, cipher)))
            } else {
                Err(PlaybackError::SyncDisconnected)
            }
        }
        ReadableFileSource::Unreachable => {
            // A local track's audio never leaves the user's disk, so a
            // missing local source is simply gone — never a cloud read.
            Err(PlaybackError::not_found("playable file location", file_id))
        }
    }
}

/// Reads from cloud storage through the home's at-rest cipher: an opaque home's
/// `Encrypted` cipher decrypts each remote blob under the library key, a
/// browsable home's `Plaintext` cipher reads the verbatim bytes.
pub struct CloudReader {
    config: AudioReadConfig,
    cloud_home: Arc<dyn crate::storage::cloud::CloudHome>,
    cipher: coven::sync::cloud_storage::CloudCipher,
}

impl CloudReader {
    pub fn new(
        config: AudioReadConfig,
        cloud_home: Arc<dyn crate::storage::cloud::CloudHome>,
        cipher: coven::sync::cloud_storage::CloudCipher,
    ) -> Self {
        Self {
            config,
            cloud_home,
            cipher,
        }
    }
}

impl AudioDataReader for CloudReader {
    fn start_reading(
        self: Box<Self>,
        buffer: SharedSparseBuffer,
        progress_tx: tokio_mpsc::UnboundedSender<PlaybackProgress>,
    ) {
        let config = self.config;
        let cloud_home = self.cloud_home;
        let cipher = self.cipher;
        let (wake, buffer) = prepare_fill(buffer);

        tokio::spawn(async move {
            let source_size = config.source_size;
            info!("CloudReader: source_size={source_size}");

            // One reader per track: on an encrypted home the nonce header is
            // fetched once and reused across every range read (a full-file stream
            // issues many); a plaintext home reads each window verbatim. Every
            // remote blob is master-scoped (see `BaeBlobSource`).
            let reader = crate::storage::BlobRangeReader::new(
                cloud_home,
                &cipher,
                coven::blob::ResolvedScope::Master,
                config.path.clone(),
                source_size,
            );

            let result = fill_buffer_on_demand(buffer.clone(), wake, |src_off, len| {
                let fut = reader.read(src_off, len);
                async move { fut.await.map_err(|e| e.to_string()) }
            })
            .await;

            if let Err(e) = result {
                let error = e.to_string();
                report_fill_failure(
                    &progress_tx,
                    &buffer,
                    format!("Cloud download failed: {error}"),
                    error,
                );
            }
        });
    }
}

const CLOUD_STREAM_READ_SIZE: u64 = crate::encryption::CHUNK_SIZE as u64 * 16;

/// The minimum the fill keeps buffered ahead of a reader, regardless of its
/// track ceiling. A few windows so the decoder's ring can't underrun: it's the
/// backstop when a reader's track ceiling isn't set yet (during probe). The
/// reader's track ceiling normally reaches much further (the rest of the current
/// track).
const MIN_READAHEAD: u64 = CLOUD_STREAM_READ_SIZE * 4;

/// How far behind a reader's position buffered bytes are retained before being
/// evicted. A margin for the decoder's brief backward reads; everything older is
/// dropped so memory stays bounded to a window around the playhead.
const KEEP_BEHIND: u64 = CLOUD_STREAM_READ_SIZE;

/// The next region to fetch, given each live reader's demand ascending by
/// position: serve the lowest-position reader first so the playing track stays
/// fed before a further-ahead reader (e.g. a gapless preload). With a track
/// ceiling set, the reader is fetched up to it -- the rest of the current track,
/// and no further, so a short track doesn't over-fetch into the next one. Without
/// one (during probe, before the decoder positions the reader), the fill keeps
/// `MIN_READAHEAD` ahead so the ring can't underrun. Returns `None` when every
/// reader is buffered up to its target -- the fill then idles.
///
/// There is no backfill: the buffer only holds the current track(s) around the
/// live readers, never the whole album. A backward seek into an evicted region
/// re-publishes a demand there and the fill re-fetches that window.
fn pick_window_gap(
    buffer: &SharedSparseBuffer,
    windows: &[ReaderDemand],
    total: u64,
) -> Option<(u64, u64)> {
    for demand in windows {
        let ceil = match demand.ceiling {
            Some(c) => c.min(total),
            None => (demand.pos + MIN_READAHEAD).min(total),
        };
        if let Some(gap) = buffer.next_gap(demand.pos, ceil) {
            return Some(gap);
        }
    }
    None
}

/// Fill the buffer from the source, following the readers: fetch the window each
/// reader needs next (read-ahead), evict what they have passed, and idle when
/// every read-ahead window is full -- resuming when a reader advances or seeks.
/// The buffer addresses the file directly (buffer offset == source byte offset)
/// and `fetch(off, len)` returns exactly `len` plaintext source bytes.
///
/// Holds only a `Weak`: the fill follows the buffer's real users (the decoder's
/// readers and the service's shared-buffer cache) and never keeps an abandoned
/// buffer alive. When the last user drops it, the upgrade fails (or the buffer's
/// `Drop` wakes a parked fill) and the loop exits. Until then: playback starts
/// after the first window, memory stays bounded to a window around the playhead,
/// and a seek into an evicted region re-fetches just that window. The strong ref
/// is released before every `await` so the buffer can be freed while we wait; no
/// lock is held across `fetch().await`.
async fn fill_buffer_on_demand<F, Fut>(
    buffer: Weak<SparseStreamingBuffer>,
    wake: Arc<Notify>,
    fetch: F,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    F: Fn(u64, u64) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    // The file size is fixed at the buffer's construction.
    let total = match buffer.upgrade() {
        Some(buf) => buf.get_total_size(),
        None => {
            debug!("fill_buffer_on_demand: buffer dropped before start, nothing to fill");
            return Ok(());
        }
    };

    loop {
        // Acquire a strong ref for this iteration's synchronous work only.
        let Some(buf) = buffer.upgrade() else {
            // The buffer's last real user is gone: nothing left to fill for.
            debug!("fill_buffer_on_demand: buffer dropped, stopping");
            return Ok(());
        };

        // Full cancel = track change / stop: the writer abandons the buffer.
        if buf.is_cancelled() {
            debug!("fill_buffer_on_demand: buffer cancelled, stopping");
            return Ok(());
        }

        // Each live reader's demand, ascending by position. `first()` is the
        // eviction floor (the most-behind reader); the whole set drives
        // read-ahead. One query serves both.
        let windows = buf.demand_windows();

        // Free bytes every reader has already passed so memory stays bounded to
        // the current track(s) around the playhead.
        if let Some(floor) = windows.first() {
            buf.evict_before(floor.pos.saturating_sub(KEEP_BEHIND));
        }

        let gap = pick_window_gap(&buf, &windows, total);
        // Release the strong ref before any await so the buffer can be freed
        // (and this fill woken via its `Drop`) while we wait.
        drop(buf);

        let Some((gap_start, gap_end)) = gap else {
            // Every reader's read-ahead window is full: park until a reader
            // advances, seeks, the buffer is cancelled, or it is dropped. The
            // wake stores a permit, so a signal during this check isn't lost.
            wake.notified().await;
            continue;
        };

        let window = CLOUD_STREAM_READ_SIZE.min(gap_end - gap_start);
        let data = fetch(gap_start, window)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

        if data.len() != window as usize {
            return Err(format!(
                "Source read returned {} bytes for requested {window} at {gap_start}",
                data.len()
            )
            .into());
        }

        // Re-acquire to store the window; if the buffer vanished during the
        // fetch, drop the bytes and exit.
        let Some(buf) = buffer.upgrade() else {
            debug!(
                "fill_buffer_on_demand: buffer dropped during fetch, \
                 discarding {window} bytes at {gap_start}"
            );
            return Ok(());
        };
        buf.append_at(gap_start, &data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encryption::{EncryptionService, CHUNK_SIZE};
    use crate::playback::sparse_buffer::create_sparse_buffer;
    use crate::storage::cloud::{CloudHome, CloudHomeError, CloudHomeJoinInfo};
    use coven::sync::cloud_storage::CloudCipher;
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tokio::time::timeout;

    /// A cloud home holding exactly one encrypted blob at a known key — enough to
    /// drive the real `CloudReader` decrypt path.
    struct OneBlobCloud {
        key: String,
        blob: Vec<u8>,
    }

    #[async_trait::async_trait]
    impl CloudHome for OneBlobCloud {
        async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError> {
            if key == self.key {
                Ok(self.blob.clone())
            } else {
                Err(CloudHomeError::NotFound(key.to_string()))
            }
        }
        async fn write(
            &self,
            _key: &str,
            _data: Vec<u8>,
            _progress: &crate::storage::cloud::UploadProgress<'_>,
        ) -> Result<(), CloudHomeError> {
            unimplemented!("not exercised by CloudReader read")
        }
        async fn read_range(
            &self,
            key: &str,
            start: u64,
            end: u64,
        ) -> Result<Vec<u8>, CloudHomeError> {
            if key != self.key {
                return Err(CloudHomeError::NotFound(key.to_string()));
            }

            let start = usize::try_from(start)
                .map_err(|e| CloudHomeError::Storage(format!("invalid range start: {e}")))?;
            let end = usize::try_from(end)
                .map_err(|e| CloudHomeError::Storage(format!("invalid range end: {e}")))?;
            if start > end || end > self.blob.len() {
                return Err(CloudHomeError::Storage(format!(
                    "range {start}..{end} outside blob length {}",
                    self.blob.len()
                )));
            }

            Ok(self.blob[start..end].to_vec())
        }
        async fn list(&self, _: &str) -> Result<Vec<String>, CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn delete(&self, _: &str) -> Result<(), CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn exists(&self, _: &str) -> Result<bool, CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn grant_access(&self, _: &str) -> Result<CloudHomeJoinInfo, CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn revoke_access(&self, _: &str) -> Result<(), CloudHomeError> {
            unimplemented!("not exercised")
        }
    }

    /// Ordered record of `read_range(start, end)` calls, shared with a test.
    type ReadLog = Arc<Mutex<Vec<(u64, u64)>>>;

    /// Per-offset gate: a range read parks until its `start` is released. Used
    /// to prove a seek's window is fetched and unblocks the reader while other
    /// windows are still held — every read blocks by default; the test releases
    /// only the offsets it wants served.
    struct ReadGate {
        released: Mutex<std::collections::HashSet<u64>>,
        notify: tokio::sync::Notify,
    }

    impl ReadGate {
        fn new() -> Self {
            Self {
                released: Mutex::new(std::collections::HashSet::new()),
                notify: tokio::sync::Notify::new(),
            }
        }

        async fn wait_for(&self, start: u64) {
            loop {
                // Arm the notification before checking so a release between the
                // check and the await isn't lost.
                let notified = self.notify.notified();
                if self.released.lock().unwrap().contains(&start) {
                    return;
                }
                notified.await;
            }
        }

        fn release(&self, start: u64) {
            self.released.lock().unwrap().insert(start);
            self.notify.notify_waiters();
        }
    }

    /// A cloud home that exposes only range reads. Playback of encrypted byte
    /// ranges must not depend on a full-object download. Counts nonce-header
    /// reads (a range at offset 0 of `NONCE_SIZE` bytes) separately so a test
    /// can assert the header is fetched exactly once across many windows, and can
    /// inject a queued range-read error (optionally after cancelling a buffer).
    struct RangeOnlyCloud {
        inner: OneBlobCloud,
        full_reads: Arc<AtomicUsize>,
        range_reads: Arc<AtomicUsize>,
        nonce_reads: Arc<AtomicUsize>,
        range_read_error: Mutex<Option<CloudHomeError>>,
        /// When set, the queued `range_read_error` fires only after cancelling
        /// this buffer — modelling teardown cancelling the buffer while a reader
        /// is parked in an in-flight `read().await` past its top-of-loop cancel
        /// check, then the read returning a genuine error.
        cancel_before_error: Mutex<Option<SharedSparseBuffer>>,
    }

    #[async_trait::async_trait]
    impl CloudHome for RangeOnlyCloud {
        async fn read(&self, key: &str) -> Result<Vec<u8>, CloudHomeError> {
            self.full_reads.fetch_add(1, Ordering::SeqCst);
            Err(CloudHomeError::Storage(format!(
                "full read should not be used for {key}"
            )))
        }
        async fn write(
            &self,
            _key: &str,
            _data: Vec<u8>,
            _progress: &crate::storage::cloud::UploadProgress<'_>,
        ) -> Result<(), CloudHomeError> {
            unimplemented!("not exercised by CloudReader read")
        }
        async fn read_range(
            &self,
            key: &str,
            start: u64,
            end: u64,
        ) -> Result<Vec<u8>, CloudHomeError> {
            self.range_reads.fetch_add(1, Ordering::SeqCst);
            if start == 0 && end == crate::encryption::NONCE_SIZE as u64 {
                self.nonce_reads.fetch_add(1, Ordering::SeqCst);
            }
            if let Some(error) = self.range_read_error.lock().unwrap().take() {
                if let Some(buffer) = self.cancel_before_error.lock().unwrap().take() {
                    buffer.cancel();
                }
                return Err(error);
            }
            self.inner.read_range(key, start, end).await
        }
        async fn list(&self, _: &str) -> Result<Vec<String>, CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn delete(&self, _: &str) -> Result<(), CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn exists(&self, _: &str) -> Result<bool, CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn grant_access(&self, _: &str) -> Result<CloudHomeJoinInfo, CloudHomeError> {
            unimplemented!("not exercised")
        }
        async fn revoke_access(&self, _: &str) -> Result<(), CloudHomeError> {
            unimplemented!("not exercised")
        }
    }

    /// Drain a sparse buffer to the end, returning everything read.
    fn drain(buffer: &SharedSparseBuffer) -> Vec<u8> {
        let mut reader = buffer.new_reader();
        let mut out = Vec::new();
        let mut chunk = vec![0u8; 1024];
        loop {
            match reader.read(&mut chunk) {
                Some(0) | None => break,
                Some(n) => out.extend_from_slice(&chunk[..n]),
            }
        }
        out
    }

    async fn drain_async(buffer: SharedSparseBuffer) -> Vec<u8> {
        tokio::task::spawn_blocking(move || drain(&buffer))
            .await
            .expect("drain task")
    }

    async fn next_playback_error(
        progress_rx: &mut tokio_mpsc::UnboundedReceiver<PlaybackProgress>,
        context: &str,
    ) -> String {
        use crate::ui::{PlaybackErrorReason, UiError};
        match progress_rx.recv().await.expect(context) {
            PlaybackProgress::PlaybackError {
                reason:
                    PlaybackErrorReason::Diagnostic {
                        error: UiError::Diagnostic { detail, .. },
                    },
            } => detail,
            other => panic!("expected diagnostic playback error, got: {other:?}"),
        }
    }

    fn full_file_config(path: impl Into<String>, source_size: u64) -> AudioReadConfig {
        AudioReadConfig {
            path: path.into(),
            source_size,
        }
    }

    /// Build a `CloudReader` over `cloud` for `file_id` reading through `cipher`
    /// — the same reader `create_audio_reader`'s cloud arm constructs, built
    /// directly here so the read-path tests don't stand up a `LibraryManager`.
    /// The storage path matches production's `storage_path(file_id)`.
    fn cloud_reader_for(
        cloud: Arc<dyn CloudHome>,
        file_id: &str,
        cipher: CloudCipher,
        source_size: u64,
    ) -> Box<CloudReader> {
        let config = full_file_config(crate::storage::local::storage_path(file_id), source_size);
        Box::new(CloudReader::new(config, cloud, cipher))
    }

    /// The playback decrypt path: a remote track whose audio lives only in the
    /// cloud, encrypted with the library master key, is read back through the
    /// `CloudReader` and recovered byte-for-byte. It exercises `CloudReader` ->
    /// coven's `BlobRangeReader` -> `EncryptionService::decrypt_range_with_offset`.
    #[tokio::test]
    async fn cloud_reader_decrypts_remote_audio_with_master_key() {
        let master_key = [9u8; 32];
        let plaintext = b"remote cloud audio for exactly one release".to_vec();
        // Encrypt as the upload outbox does: with the library master key.
        let encrypted = EncryptionService::from_key(master_key).encrypt(&plaintext);

        let file_id = "file-remote-1";
        let storage_key = crate::storage::local::storage_path(file_id);
        let cloud: Arc<dyn CloudHome> = Arc::new(OneBlobCloud {
            key: storage_key,
            blob: encrypted,
        });

        let reader = cloud_reader_for(
            cloud,
            file_id,
            CloudCipher::Encrypted(EncryptionService::from_key(master_key)),
            plaintext.len() as u64,
        );

        let buffer = create_sparse_buffer(plaintext.len() as u64);
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let actual = drain_async(buffer.clone()).await;

        assert!(
            !buffer.is_cancelled(),
            "decrypt with the right key must succeed"
        );
        assert_eq!(
            actual, plaintext,
            "CloudReader must recover the audio after decrypting with the master key"
        );
    }

    /// The browsable-home playback path: a remote track whose audio lives only
    /// in the cloud, stored verbatim (a browsable home seals nothing), is read
    /// back through a `Plaintext` `CloudReader` and recovered byte-for-byte. The
    /// upload writes the bytes as-is, so the read must not look for a nonce or
    /// try to decrypt. Mirror of the opaque decrypt test with the home's cipher
    /// flipped to `Plaintext` and the stored blob left unencrypted.
    #[tokio::test]
    async fn cloud_reader_reads_browsable_remote_audio_verbatim() {
        // A browsable home stores the audio in the clear: the stored blob IS the
        // plaintext, with no nonce header.
        let plaintext = b"remote cloud audio on a browsable home".to_vec();

        let file_id = "file-browsable-1";
        let storage_key = crate::storage::local::storage_path(file_id);
        let cloud: Arc<dyn CloudHome> = Arc::new(OneBlobCloud {
            key: storage_key,
            blob: plaintext.clone(),
        });

        let reader = cloud_reader_for(
            cloud,
            file_id,
            CloudCipher::Plaintext,
            plaintext.len() as u64,
        );

        let buffer = create_sparse_buffer(plaintext.len() as u64);
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let actual = drain_async(buffer.clone()).await;

        assert!(
            !buffer.is_cancelled(),
            "a verbatim read on a browsable home must succeed"
        );
        assert_eq!(
            actual, plaintext,
            "CloudReader must recover the verbatim audio from a browsable home"
        );
    }

    #[tokio::test]
    async fn cloud_reader_streams_remote_full_file_without_full_download() {
        let master_key = [8u8; 32];
        // Span several `CLOUD_STREAM_READ_SIZE` windows so the nonce-read-once
        // assertion has multiple windows to be true across.
        let plaintext: Vec<u8> = (0..CHUNK_SIZE * 40)
            .map(|i| ((i * 17) % 251) as u8)
            .collect();
        let encrypted = EncryptionService::from_key(master_key).encrypt(&plaintext);

        let file_id = "file-remote-full-range-1";
        let storage_key = crate::storage::local::storage_path(file_id);
        let full_reads = Arc::new(AtomicUsize::new(0));
        let range_reads = Arc::new(AtomicUsize::new(0));
        let nonce_reads = Arc::new(AtomicUsize::new(0));
        let cloud: Arc<dyn CloudHome> = Arc::new(RangeOnlyCloud {
            inner: OneBlobCloud {
                key: storage_key,
                blob: encrypted,
            },
            full_reads: full_reads.clone(),
            range_reads: range_reads.clone(),
            nonce_reads: nonce_reads.clone(),
            range_read_error: Mutex::new(None),
            cancel_before_error: Mutex::new(None),
        });

        let reader = cloud_reader_for(
            cloud,
            file_id,
            CloudCipher::Encrypted(EncryptionService::from_key(master_key)),
            plaintext.len() as u64,
        );

        let buffer = create_sparse_buffer(plaintext.len() as u64);
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let actual = drain_async(buffer.clone()).await;

        assert!(
            !buffer.is_cancelled(),
            "encrypted cloud full-file playback must not depend on full-object reads"
        );
        assert_eq!(actual, plaintext);
        assert_eq!(
            full_reads.load(Ordering::SeqCst),
            0,
            "encrypted full-file playback must not call CloudHome::read"
        );
        assert!(
            range_reads.load(Ordering::SeqCst) >= 2,
            "encrypted full-file playback must read the nonce and encrypted chunks"
        );
        assert_eq!(
            nonce_reads.load(Ordering::SeqCst),
            1,
            "the nonce header is read once and reused across every streamed window"
        );
    }

    /// A genuine range read failure on a LIVE buffer (no teardown) surfaces a
    /// PlaybackError so the service can stop the frozen Playing/Loading state.
    /// The buffer ends cancelled only as a consequence of the failure.
    #[tokio::test]
    async fn cloud_reader_reports_range_failure_on_live_buffer() {
        let file_id = "file-remote-range-error-1";
        let storage_key = crate::storage::local::storage_path(file_id);
        let range_reads = Arc::new(AtomicUsize::new(0));
        let cloud: Arc<dyn CloudHome> = Arc::new(RangeOnlyCloud {
            inner: OneBlobCloud {
                key: storage_key,
                blob: Vec::new(),
            },
            full_reads: Arc::new(AtomicUsize::new(0)),
            range_reads: range_reads.clone(),
            nonce_reads: Arc::new(AtomicUsize::new(0)),
            range_read_error: Mutex::new(Some(CloudHomeError::Storage(
                "mock range read failure: checksum mismatch".to_string(),
            ))),
            cancel_before_error: Mutex::new(None),
        });
        let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();

        let reader = cloud_reader_for(
            cloud,
            file_id,
            CloudCipher::Encrypted(EncryptionService::from_key([3u8; 32])),
            4096,
        );

        let buffer = create_sparse_buffer(4096);
        reader.start_reading(buffer.clone(), progress_tx);
        let actual = drain_async(buffer.clone()).await;

        assert!(actual.is_empty());
        assert!(buffer.is_cancelled());
        assert_eq!(
            range_reads.load(Ordering::SeqCst),
            1,
            "encrypted read should fail while fetching the nonce header"
        );
        let error = next_playback_error(
            &mut progress_rx,
            "cloud range failure should emit playback error",
        )
        .await;
        // The failure on the nonce read (the first range read, asserted above)
        // surfaces the provider's error body to the player.
        assert!(
            error.contains("checksum mismatch"),
            "expected provider body error, got: {error}",
        );
    }

    /// Teardown (track switch) cancels the buffer while a reader is parked in an
    /// in-flight `read().await` past its top-of-loop cancel check; the read then
    /// returns a genuine error. That error belongs to the abandoned track, so it
    /// must NOT surface a PlaybackError — otherwise the service's HaltOnError
    /// self-subscription would stop the track the user just switched to. The
    /// buffer still ends cancelled.
    #[tokio::test]
    async fn cloud_reader_suppresses_failure_after_buffer_cancelled() {
        let file_id = "file-remote-range-error-cancelled-1";
        let storage_key = crate::storage::local::storage_path(file_id);
        let range_reads = Arc::new(AtomicUsize::new(0));
        let buffer = create_sparse_buffer(4096);
        let cloud: Arc<dyn CloudHome> = Arc::new(RangeOnlyCloud {
            inner: OneBlobCloud {
                key: storage_key,
                blob: Vec::new(),
            },
            full_reads: Arc::new(AtomicUsize::new(0)),
            range_reads: range_reads.clone(),
            nonce_reads: Arc::new(AtomicUsize::new(0)),
            range_read_error: Mutex::new(Some(CloudHomeError::Storage(
                "mock range read failure".to_string(),
            ))),
            // Cancel the buffer mid-read, then fail the read: the reader's
            // top-of-loop cancel check already passed before this read began.
            cancel_before_error: Mutex::new(Some(buffer.clone())),
        });
        let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();

        let reader = cloud_reader_for(
            cloud,
            file_id,
            CloudCipher::Encrypted(EncryptionService::from_key([3u8; 32])),
            4096,
        );

        reader.start_reading(buffer.clone(), progress_tx);
        let actual = drain_async(buffer.clone()).await;

        assert!(actual.is_empty());
        assert!(
            buffer.is_cancelled(),
            "the buffer must still end cancelled so the reader stops"
        );
        assert_eq!(
            range_reads.load(Ordering::SeqCst),
            1,
            "the failing read is the nonce-header fetch"
        );
        assert!(
            progress_rx.try_recv().is_err(),
            "a failure on an already-cancelled buffer must not emit a PlaybackError"
        );
    }

    /// Sabotage check: the wrong key must NOT decrypt the blob — the reader
    /// cancels the buffer. Guards against playback silently pairing a blob with
    /// a key that isn't the one it was encrypted under.
    #[tokio::test]
    async fn cloud_reader_with_wrong_key_cancels() {
        let master_key = [9u8; 32];
        let wrong_key = [1u8; 32];
        let plaintext = b"remote cloud audio".to_vec();
        let encrypted = EncryptionService::from_key(master_key).encrypt(&plaintext);

        let file_id = "file-remote-2";
        let storage_key = crate::storage::local::storage_path(file_id);
        let cloud: Arc<dyn CloudHome> = Arc::new(OneBlobCloud {
            key: storage_key,
            blob: encrypted,
        });

        let reader = cloud_reader_for(
            cloud,
            file_id,
            CloudCipher::Encrypted(EncryptionService::from_key(wrong_key)),
            plaintext.len() as u64,
        );

        let buffer = create_sparse_buffer(plaintext.len() as u64);
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let actual = drain_async(buffer.clone()).await;

        assert!(
            buffer.is_cancelled(),
            "the wrong key must fail decryption and cancel the read"
        );
        assert!(actual.is_empty());
    }

    const WINDOW: u64 = CLOUD_STREAM_READ_SIZE;

    /// Spawn the production `fill_buffer_on_demand` loop over an in-memory
    /// `blob`, filling `buffer`. The fetch serves `blob[start..end]` directly —
    /// source offsets map identity into the buffer — so the seek-ordering and
    /// eviction assertions read against plaintext offsets. Records every
    /// `(start, end)` fetch into the returned log and, when `gate` is `Some`,
    /// blocks each fetch until its `start` is released. This drives the same fill
    /// loop the readers use, isolated from the encrypt/cloud fetch source so the
    /// loop's demand and eviction behavior is what's under test.
    fn start_recording_fill(
        blob: Vec<u8>,
        gate: Option<Arc<ReadGate>>,
        buffer: &SharedSparseBuffer,
    ) -> ReadLog {
        let read_log: ReadLog = Arc::new(Mutex::new(Vec::new()));
        let blob = Arc::new(blob);
        let (wake, weak) = prepare_fill(buffer.clone());
        let log = read_log.clone();
        tokio::spawn(async move {
            // Normal teardown (the buffer cancelled or its weak ref dropped)
            // returns `Ok(())`; only a real fetch failure or short read returns
            // `Err`. So `expect` here fails the test on a genuine fill error
            // without firing on clean teardown.
            fill_buffer_on_demand(weak, wake, move |start, len| {
                let blob = blob.clone();
                let log = log.clone();
                let gate = gate.clone();
                async move {
                    let end = start + len;
                    log.lock().unwrap().push((start, end));
                    if let Some(gate) = &gate {
                        gate.wait_for(start).await;
                    }
                    Ok(blob[start as usize..end as usize].to_vec())
                }
            })
            .await
            .expect("recording fill failed");
        });
        read_log
    }

    /// A seek fetches only the demanded window — never the skipped prefix. With
    /// the reader parked at `6*WINDOW` before the fill runs, the fill fetches
    /// that window (and its read-ahead) and never touches `[WINDOW, 6*WINDOW)`:
    /// there is no backfill, so opening a late track doesn't drag the whole
    /// earlier prefix down. A sequential fill would fetch `[WINDOW, 2*WINDOW)`
    /// long before the target.
    #[tokio::test]
    async fn seek_fetches_only_the_demanded_window_not_the_skipped_prefix() {
        let source_size = 8 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        let buffer = create_sparse_buffer(source_size);
        // Register the seek demand BEFORE the fill loop starts so the first fetch
        // is demand-driven (this is the seek case: the reader already exists).
        let mut reader = buffer.new_reader();
        assert!(reader.seek(6 * WINDOW));

        let read_log = start_recording_fill(blob, None, &buffer);

        // Read one byte at the seek target from the real blocking unit.
        let one = tokio::task::spawn_blocking(move || {
            let mut b = [0u8; 1];
            let n = reader.read(&mut b);
            // Hold the reader until after the read so its demand stays registered.
            drop(reader);
            n
        })
        .await
        .expect("seek read task");
        assert_eq!(one, Some(1), "the seek target byte must be served");

        // Stop the fill, then snapshot the fetch order.
        buffer.cancel();
        let log = read_log.lock().unwrap().clone();

        assert!(
            log.iter().any(|&(start, _)| start >= 6 * WINDOW),
            "a window at or past the seek target must have been fetched; log: {log:?}"
        );
        assert!(
            !log.iter()
                .any(|&(start, _)| (WINDOW..6 * WINDOW).contains(&start)),
            "no window in the skipped prefix [{}, {}) may be fetched — there is no \
             backfill; log: {:?}",
            WINDOW,
            6 * WINDOW,
            log,
        );
    }

    /// Playing a file front-to-back recovers every byte, yet the buffer never
    /// holds more than a window around the playhead: the fill evicts what the
    /// reader has passed instead of accumulating the whole file in RAM. Sabotage
    /// check — drop the `evict_before` call and the buffer grows to the full
    /// `source_size`, failing the bound.
    #[tokio::test]
    async fn forward_play_evicts_so_memory_stays_bounded() {
        let source_size = 16 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        let buffer = create_sparse_buffer(source_size);
        let _read_log = start_recording_fill(blob.clone(), None, &buffer);

        let out = drain_async(buffer.clone()).await;
        assert_eq!(out, blob, "front-to-back play must recover the whole file");

        // No ceiling is set on this reader, so it fetches the cushion ahead and
        // evicts behind: the whole file passes through but only a window's worth
        // is retained.
        let bound = MIN_READAHEAD + KEEP_BEHIND + 2 * WINDOW;
        let buffered = buffer.total_buffered();
        assert!(
            buffered <= bound,
            "buffer must stay windowed: held {buffered} bytes, bound {bound}, \
             whole file {source_size}"
        );
    }

    /// A reader with a track ceiling set has the fill buffer the whole track
    /// ahead of it -- up to the ceiling -- not just the fixed cushion, and it
    /// stops at the ceiling rather than fetching the rest of the file. This is
    /// the "keep the whole current track" behavior: a single-file album fetches
    /// its current track's byte range; a per-track file sets the ceiling to the
    /// whole file. Sabotage -- ignore the ceiling and fetch only the cushion --
    /// and the reader stalls well short of the ceiling, failing the wait.
    #[tokio::test]
    async fn a_ceiling_fetches_the_whole_track_then_stops() {
        let source_size = 16 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        // The track ends at 12 windows -- past the cushion (MIN_READAHEAD, 4
        // windows) and short of the 16-window file.
        let ceiling = 12 * WINDOW;
        let buffer = create_sparse_buffer(source_size);
        let mut reader = buffer.new_reader();
        reader.set_readahead_ceiling(ceiling);
        assert!(reader.seek(0));
        let _read_log = start_recording_fill(blob, None, &buffer);

        // The reader sits at 0 (held, not draining); the fill should buffer up to
        // the ceiling. Yield to let the (current-thread) fill run.
        let mut reached = false;
        for _ in 0..2000 {
            if buffer.is_buffered(ceiling - 1) {
                reached = true;
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            reached,
            "the fill must buffer up to the track ceiling ({}W), not stop at the \
             {}W cushion",
            ceiling / WINDOW,
            MIN_READAHEAD / WINDOW,
        );
        assert!(
            !buffer.is_buffered(ceiling + WINDOW),
            "the fill must stop at the ceiling, not fetch the rest of the file"
        );

        drop(reader);
        buffer.cancel();
    }

    /// A backward seek into an already-evicted region re-fetches just that
    /// window and serves the read. The fill is still alive after the forward
    /// pass, so evicted bytes are recoverable on demand — proven by the window
    /// being fetched a second time and the re-read returning the correct bytes.
    #[tokio::test]
    async fn backward_seek_into_evicted_region_refetches_the_window() {
        let source_size = 16 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();

        let buffer = create_sparse_buffer(source_size);
        let read_log = start_recording_fill(blob.clone(), None, &buffer);

        let reader_buffer = buffer.clone();
        let (n, got) = tokio::task::spawn_blocking(move || {
            let mut reader = reader_buffer.new_reader();
            // Play forward well past the second window so [WINDOW, 2*WINDOW) is
            // evicted (it is more than KEEP_BEHIND behind the playhead).
            let mut sink = vec![0u8; 10 * WINDOW as usize];
            let mut filled = 0;
            while filled < sink.len() {
                match reader.read(&mut sink[filled..]) {
                    Some(0) => panic!(
                        "unexpected EOF at {filled} reading a {}-byte file",
                        16 * WINDOW
                    ),
                    None => panic!("unexpected cancel at {filled}"),
                    Some(read) => filled += read,
                }
            }
            // Seek back into the evicted window and read it again.
            assert!(reader.seek(WINDOW));
            let mut b = [0u8; 32];
            let n = reader.read(&mut b);
            (n, b)
        })
        .await
        .expect("forward-then-back task");

        let n = n.expect("the backward-seek read returned data, not EOF/cancel");
        let target = WINDOW as usize;
        assert_eq!(
            &got[..n],
            &blob[target..target + n],
            "re-fetched bytes must match the file at the seek target"
        );

        buffer.cancel();
        let fetches_at_window = read_log
            .lock()
            .unwrap()
            .iter()
            .filter(|&&(start, _)| start == WINDOW)
            .count();
        assert!(
            fetches_at_window >= 2,
            "the window at {WINDOW} must be fetched twice (once forward, once \
             after it was evicted and sought back into); fetches: {fetches_at_window}"
        );
    }

    /// Spawned through the production `start_reading` path, the fill holds only
    /// a `Weak` (it never keeps the buffer alive) and parks when no reader has a
    /// demand. Dropping the buffer's last strong ref frees it, and the buffer's
    /// `Drop` wakes the parked fill so the task exits — releasing its clone of
    /// the wake handle. Sabotage either half — `start_reading` keeping a strong
    /// ref, or removing the `Drop` wake — and an assertion fails. Yields, not a
    /// timer, advance the single-threaded test runtime.
    #[tokio::test]
    async fn start_reading_fill_follows_the_buffer_and_exits_when_it_is_dropped() {
        let source_size = 4 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&blob).unwrap();
        temp_file.flush().unwrap();

        let buffer = create_sparse_buffer(source_size);
        // Our own clone of the wake handle; the fill takes another via
        // `start_reading`, and the buffer holds the third.
        let wake = buffer.fill_wake_handle();
        let reader = Box::new(LocalReader::new(full_file_config(
            temp_file.path().to_str().expect("temp path is UTF-8"),
            source_size,
        )));
        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let weak = Arc::downgrade(&buffer);

        // Let the spawned fill run to its park. With no reader it fetches nothing
        // and holds only a weak ref, leaving the test as the buffer's sole owner.
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            Arc::strong_count(&buffer),
            1,
            "start_reading's fill must hold only a Weak, not keep the buffer alive"
        );

        // Drop the last strong ref: the buffer frees and its Drop wakes the
        // parked fill, which upgrades to None and exits, dropping its wake clone.
        drop(buffer);
        assert!(weak.upgrade().is_none(), "the buffer must be freed");
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            Arc::strong_count(&wake),
            1,
            "the parked fill task must exit when the buffer is dropped, not leak"
        );
    }

    /// A reader blocked at the seek target unblocks as soon as that target's
    /// window lands — it does not wait for the prefix to download. Every read is
    /// gated; only the demanded window is released, yet the read returns while
    /// `[WINDOW, 6*WINDOW)` are still unfetched. A purely sequential fill would
    /// park on the front window (offset 0, never released) and never reach the
    /// target, so the reader would hang.
    #[tokio::test]
    async fn blocked_reader_unblocks_on_demanded_window_not_sequential_catch_up() {
        let source_size = 8 * WINDOW;
        let blob: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
        let gate = Arc::new(ReadGate::new());

        let buffer = create_sparse_buffer(source_size);
        let mut reader = buffer.new_reader();
        assert!(reader.seek(6 * WINDOW));
        let read_log = start_recording_fill(blob, Some(gate.clone()), &buffer);

        // Release ONLY the demanded window; everything else stays parked.
        gate.release(6 * WINDOW);

        let read_handle = tokio::task::spawn_blocking(move || {
            let mut b = [0u8; 1];
            let n = reader.read(&mut b);
            drop(reader);
            n
        });

        let outcome = timeout(Duration::from_secs(10), read_handle).await;

        // Whatever happened, unblock every parked reader/fill so their threads
        // exit (a reader stuck on the condvar can't otherwise be cancelled) — and
        // snapshot the read log before threads can fetch anything more.
        let log = read_log.lock().unwrap().clone();
        buffer.cancel();
        gate.release(0);

        let served = outcome
            .expect("a blocked reader must unblock once its demanded window lands, not hang")
            .expect("read task");
        assert_eq!(served, Some(1), "the demanded window served the read");

        // The skipped prefix [WINDOW, 6*WINDOW) must still be unfetched — the
        // reader did not wait for a sequential crawl up to the target.
        assert!(
            !log.iter()
                .any(|&(start, _)| (WINDOW..6 * WINDOW).contains(&start)),
            "no window in [{}, {}) should be fetched yet; read order: {:?}",
            WINDOW,
            6 * WINDOW,
            log,
        );
    }

    #[tokio::test]
    async fn test_local_file_reader_full_file() {
        // Create temp file with test data
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, this is test audio data for streaming!";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();

        let config = full_file_config(
            temp_file.path().to_str().expect("temp path is UTF-8"),
            test_data.len() as u64,
        );

        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(test_data.len() as u64);

        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let result = drain_async(buffer.clone()).await;

        assert_eq!(result, test_data);
    }

    /// A deep seek into a large local file is served through the shared
    /// demand-driven fill loop: a reader parked at `6*WINDOW` before the loop
    /// starts reads the exact file bytes at that offset (not the prefix). The
    /// byte pattern is position-dependent, so wrong-offset fill would mismatch.
    #[tokio::test]
    async fn local_seek_into_large_file_serves_target_via_shared_fill() {
        let source_size = 8 * WINDOW;
        let data: Vec<u8> = (0..source_size).map(|i| (i % 251) as u8).collect();
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();

        let config = full_file_config(
            temp_file.path().to_str().expect("temp path is UTF-8"),
            source_size,
        );
        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(source_size);

        // Register the seek demand before the fill loop starts (the seek case).
        let mut seek_reader = buffer.new_reader();
        assert!(seek_reader.seek(6 * WINDOW));

        let (progress_tx, _progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);

        let chunk_len = 4096usize;
        let read = tokio::task::spawn_blocking(move || {
            let mut out = vec![0u8; chunk_len];
            let n = seek_reader.read(&mut out);
            drop(seek_reader);
            (n, out)
        });
        let (n, out) = timeout(Duration::from_secs(10), read)
            .await
            .expect("a deep local seek must be served, not hang")
            .expect("seek read task");

        let n = n.expect("seek read returned data");
        let target = 6 * WINDOW as usize;
        assert_eq!(
            &out[..n],
            &data[target..target + n],
            "the bytes at the seek target must match the file at that offset"
        );
    }

    #[tokio::test]
    async fn test_local_file_reader_nonexistent_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"deleted before read";
        temp_file.write_all(test_data).unwrap();
        temp_file.flush().unwrap();
        let path = temp_file
            .path()
            .to_str()
            .expect("temp path is UTF-8")
            .to_string();
        drop(temp_file);

        let config = full_file_config(path, test_data.len() as u64);

        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(test_data.len() as u64);

        let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let result = drain_async(buffer.clone()).await;

        // Buffer should be cancelled (error case)
        assert!(
            buffer.is_cancelled(),
            "Buffer should be cancelled for nonexistent file"
        );
        assert!(result.is_empty());
        let error = next_playback_error(
            &mut progress_rx,
            "local open failure should emit playback error",
        )
        .await;
        assert!(
            error.contains("Failed to open file"),
            "expected open failure, got: {error}"
        );
    }

    #[tokio::test]
    async fn test_local_file_reader_read_error_reports_error() {
        let dir = tempfile::tempdir().unwrap();
        let config = full_file_config(dir.path().to_str().expect("temp path is UTF-8"), 1);

        let reader = Box::new(LocalReader::new(config));
        let buffer = create_sparse_buffer(1);

        let (progress_tx, mut progress_rx) = tokio_mpsc::unbounded_channel();
        reader.start_reading(buffer.clone(), progress_tx);
        let result = drain_async(buffer.clone()).await;

        assert!(buffer.is_cancelled());
        assert!(result.is_empty());
        let error = next_playback_error(
            &mut progress_rx,
            "local read failure should emit playback error",
        )
        .await;
        assert!(
            error.contains("Read error in file"),
            "expected read failure, got: {error}"
        );
    }
}
