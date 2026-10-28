import Foundation

/// Controls for the download (pin) queue: enqueue releases to pin,
/// pause/resume the queue, and cancel downloads.
/// Mirrors the outbox controls on `Sync`, wrapping the corresponding
/// `handle.*` methods. The queue itself lives in bae-core; the Storage
/// Manager's inspector reads a release's entry in it, and `DownloadStore`
/// holds the bytes each download has moved.
public final class Downloads: Sendable, Observable {
    /// Enqueue releases to pin locally. They join the serial download
    /// queue; the worker drains them one at a time. The queue arrives through
    /// its live read, and the bytes each download moves through its transfers.
    public let queuePins:
        @Sendable (_ releaseIds: [String]) async throws -> Void
    /// Unpin a release: coven moves its files from storage/pinned/ to the
    /// evictable cache; the release stays Remote and playable.
    public let unpinRelease:
        @Sendable (_ releaseId: String) async throws -> Void
    /// Pause or resume the download queue. In-flight downloads finish; the
    /// queue stops starting new ones until resumed.
    public let setDownloadsPaused:
        @Sendable (_ paused: Bool) async throws -> Void
    /// Cancel a release's download — drops a queued/failed entry or aborts the
    /// in-flight one (the release stays cloud-only).
    public let cancelDownload:
        @Sendable (_ releaseId: String) async throws -> Void
    /// How many downloads a pin fetches at once (1...8). A persisted device-local
    /// config write, unlike the runtime pause/cancel controls: it throws on an
    /// out-of-range value or a failed write, so the picker can snap back. Takes
    /// effect the next time the library's coven handle opens.
    public let setMaxConcurrentDownloads:
        @Sendable (_ n: UInt32) async throws -> Void

    public init(
        queuePins: @escaping @Sendable ([String]) async throws -> Void = { _ in
        },
        unpinRelease: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        setDownloadsPaused: @escaping @Sendable (Bool) async throws -> Void = {
            _ in
        },
        cancelDownload: @escaping @Sendable (String) async throws -> Void = {
            _ in
        },
        setMaxConcurrentDownloads:
            @escaping @Sendable (UInt32) async throws -> Void =
            {
                _ in
            }
    ) {
        self.queuePins = queuePins
        self.unpinRelease = unpinRelease
        self.setDownloadsPaused = setDownloadsPaused
        self.cancelDownload = cancelDownload
        self.setMaxConcurrentDownloads = setMaxConcurrentDownloads
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(
            queuePins: { try await handle.queuePinReleases(releaseIds: $0) },
            unpinRelease: { try await handle.unpinRelease(releaseId: $0) },
            setDownloadsPaused: {
                try await handle.setDownloadsPaused(paused: $0)
            },
            cancelDownload: { try await handle.cancelDownload(releaseId: $0) },
            setMaxConcurrentDownloads: {
                try await handle.setMaxConcurrentDownloads(n: $0)
            }
        )
    }

    #if DEBUG
        // periphery:ignore
        public static func stub() -> Downloads { Downloads() }
    #endif
}
