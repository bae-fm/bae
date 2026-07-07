import Foundation

/// Controls for the in-memory download (pin) queue: enqueue releases to pin,
/// pause/resume the queue, cancel a release's download, and retry failed ones.
/// Mirrors the outbox controls on `Sync`, wrapping the corresponding
/// `handle.*` methods. The queue itself lives in bae-core; the Downloads pane
/// reads its state from `DownloadStore` (the reducer is the sole writer).
final class Downloads: Sendable, Observable {
    /// Enqueue releases to pin for offline. They join the serial download
    /// queue; the worker drains them one at a time. Fire-and-forget — progress
    /// and queue state arrive via `downloadQueueChanged` events.
    let queuePins: @Sendable (_ releaseIds: [String]) async -> Void
    /// Unpin a release: coven moves its blobs from storage/pinned/ to the
    /// evictable cache; the release stays Remote and playable.
    let unpinRelease: @Sendable (_ releaseId: String) async throws -> Void
    /// Pause or resume the download queue. In-flight downloads finish; the
    /// queue stops starting new ones until resumed.
    let setDownloadsPaused: @Sendable (_ paused: Bool) -> Void
    /// Cancel a release's download — drops a queued/failed entry or aborts the
    /// in-flight one (the release stays cloud-only).
    let cancelDownload: @Sendable (_ releaseId: String) -> Void
    /// Retry every failed download now (flips them back to queued).
    let retryDownloads: @Sendable () -> Void

    init(
        queuePins: @escaping @Sendable ([String]) async -> Void = { _ in },
        unpinRelease: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        setDownloadsPaused: @escaping @Sendable (Bool) -> Void = { _ in },
        cancelDownload: @escaping @Sendable (String) -> Void = { _ in },
        retryDownloads: @escaping @Sendable () -> Void = {}
    ) {
        self.queuePins = queuePins
        self.unpinRelease = unpinRelease
        self.setDownloadsPaused = setDownloadsPaused
        self.cancelDownload = cancelDownload
        self.retryDownloads = retryDownloads
    }

    convenience init(handle: any AppHandleProtocol) {
        self.init(
            queuePins: { await handle.queuePinReleases(releaseIds: $0) },
            unpinRelease: { try await handle.unpinRelease(releaseId: $0) },
            setDownloadsPaused: { handle.setDownloadsPaused(paused: $0) },
            cancelDownload: { handle.cancelDownload(releaseId: $0) },
            retryDownloads: { handle.retryDownloads() }
        )
    }

    // periphery:ignore
    static let stub = Downloads()
}
