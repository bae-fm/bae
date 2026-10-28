import SwiftUI

/// How far each downloading release, and each file a pin downloads, has got.
/// The transfer subscriptions land each value in a store of keyed values
/// nothing observes, and the native view drawing that release or file sets
/// itself from it, so arriving bytes run no SwiftUI update. The queue's
/// entries are the Storage Manager inspector's, read for one release in core.
@MainActor
@Observable
public class DownloadStore {
    /// The latest share of each file in flight, by file, which the file's
    /// native view reads each display frame.
    private nonisolated let fileShares = LatestByKey<Double>()
    /// Each downloading release's progress, by release.
    private let releases = KeyedValues<BridgeDownloadTransferProgress>()
    /// The latest progress of each downloading release, which the release's
    /// native views read each display frame.
    private nonisolated let releaseProgress =
        LatestByKey<BridgeDownloadTransferProgress>()

    public init() {}

    /// Apply one change the transfer subscription delivered.
    public func applyTransferChange(_ change: BridgeDownloadTransferChange) {
        switch change {
        case .updated(let releaseId, let progress):
            releases.set(progress, for: releaseId)
            releaseProgress.set(progress, for: releaseId)
        case .removed(let releaseId):
            releases.remove(releaseId)
            releaseProgress.remove(releaseId)
        case .reset(let keyed):
            let all = Dictionary(
                uniqueKeysWithValues: keyed.map { ($0.releaseId, $0.progress) }
            )
            releases.reset(all)
            releaseProgress.reset(all)
        }
    }

    /// Apply one change the pin file transfer subscription delivered, on any
    /// thread.
    public nonisolated func applyFileTransferChange(
        _ change: BridgeFileTransferChange
    ) {
        switch change {
        case .updated(let fileId, let fraction):
            fileShares.set(fraction, for: fileId)
        case .removed(let fileId):
            fileShares.remove(fileId)
        case .reset(let keyed):
            let all = Dictionary(
                uniqueKeysWithValues: keyed.map { ($0.fileId, $0.fraction) }
            )
            fileShares.reset(all)
        }
    }

    /// The latest progress of a downloading release, for the native views
    /// that read it each display frame.
    public nonisolated func downloadProgress(forRelease releaseId: String)
        -> LatestSource<BridgeDownloadTransferProgress>
    {
        LatestSource(store: releaseProgress, key: releaseId)
    }

    /// The latest share a file in flight has moved, for the native view that
    /// reads it each display frame.
    public nonisolated func fileShare(forFile fileId: String)
        -> LatestSource<Double>
    {
        LatestSource(store: fileShares, key: fileId)
    }

    /// The stream of a release's download progress, for the native view that
    /// draws it.
    public func releaseTransfer(forRelease releaseId: String)
        -> KeyedValueSource<BridgeDownloadTransferProgress>
    {
        KeyedValueSource(values: releases, key: releaseId)
    }
}
