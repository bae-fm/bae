import Foundation
import Observation

/// Root application object for one unlocked library. Not `@Observable` — the
/// reactive state lives on the contained stores. Conforms to `Observable`
/// (empty) so it can sit in SwiftUI's `@Environment`.
///
/// The iOS counterpart of the macOS `AppService`, minus the desktop-only wiring
/// (import / export / preview / release-editor / discogs and the Vision artwork
/// analyzer) — those bridge methods are gated off the iOS xcframework, so
/// referencing them wouldn't compile.
@MainActor
final class AppService: Observable {
    /// Handle to bae-core via the uniffi bridge. All library, playback, and
    /// sync requests go through this.
    let appHandle: AppHandle

    /// Drives the iOS Now Playing info and lock-screen / Control-Center
    /// remote commands, and owns the `AVAudioSession` the cpal sink needs.
    let mediaControlService = MediaControlService()

    // ── Stores ───────────────────────────────────────────────────────────

    let playbackStore = PlaybackStore()
    let configStore: ConfigStore
    let libraryStore = LibraryStore()
    let downloadStore: DownloadStore
    let projectionRegistry = ProjectionRegistry()
    private var projectionRegistrations: [ProjectionRegistration] = []

    // ── Domain services (narrow projections of `AppHandle`) ───────────────

    let library: Library
    let playback: Playback
    let queue: Queue
    let mediaPaths: MediaPaths
    let sync: Sync

    init(appHandle: AppHandle, config: BridgeConfig) {
        self.appHandle = appHandle
        configStore = ConfigStore(
            config: Config(bridge: config),
            syncReady: appHandle.isSyncReady()
        )
        downloadStore = DownloadStore(snapshot: appHandle.getDownloadSnapshot())
        // `prefetchRelease` (desktop import flow) is unavailable on iOS, so
        // build the read services from the iOS-available closures explicitly
        // rather than the `init(handle:)` convenience that wires it.
        library = Library(
            getAlbumCount: { try appHandle.getAlbumCount() },
            getAlbumPage: {
                try appHandle.getAlbumPage(
                    sortCriteria: $0,
                    offset: $1,
                    limit: $2
                )
            },
            getComposerCount: { try appHandle.getComposerCount() },
            getComposerPage: {
                try appHandle.getComposerPage(
                    sortCriterion: $0,
                    offset: $1,
                    limit: $2
                )
            },
            getComposerDetail: {
                try appHandle.getComposerDetail(artistId: $0)
            },
            getWorkDetail: { try appHandle.getWorkDetail(workId: $0) },
            searchLibrary: { try await appHandle.searchLibrary(query: $0) },
            findReleaseDetail: { try appHandle.findReleaseDetail(releaseId: $0) },
            resolveToTrackIds: { try appHandle.resolveToTrackIds(ids: $0) }
        )
        playback = Playback(handle: appHandle)
        // All nine queue ops are in the common impl block (present on iOS), so
        // the convenience init wires them directly.
        queue = Queue(handle: appHandle)
        // `fetchCoverBytes` (remote cover-art search) is desktop-only; iOS has
        // no import flow, so build `MediaPaths` without it.
        mediaPaths = MediaPaths(
            filePath: { try appHandle.filePath(fileId: $0) },
            fetchCoverImageBytes: {
                try await appHandle.fetchCoverImageBytes(releaseId: $0)
            },
            fetchGalleryBytes: {
                try await appHandle.fetchGalleryBytes(releaseId: $0, source: $1)
            }
        )
        sync = Sync(handle: appHandle)
    }

    /// Subscribe to the live event stream and register media-control remote
    /// commands. Called once after construction.
    func wireUp() {
        registerRootProjections()
        appHandle.subscribeUiEvents(
            callback: UiEventHandler(
                appService: self
            )
        )
        mediaControlService.setupRemoteCommands(
            playback: playback,
            playbackStore: playbackStore
        )
    }

    private func registerRootProjections() {
        precondition(projectionRegistrations.isEmpty)
        projectionRegistrations = [
            projectionRegistry.register(makeConfigProjection()),
            projectionRegistry.register(makeSyncStatusProjection()),
            projectionRegistry.register(makeDownloadProjection()),
            projectionRegistry.register(makeReleaseDetailProjection()),
        ]
    }

    private struct ConfigProjectionValue: Sendable {
        let config: BridgeConfig
        let syncReady: Bool
    }

    private func makeConfigProjection() -> Projection<ConfigProjectionValue> {
        Projection(
            domain: .config,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    ConfigProjectionValue(
                        config: appHandle.getConfig(),
                        syncReady: appHandle.isSyncReady()
                    )
                }
            },
            apply: { [configStore] value in
                configStore.applyConfigSnapshot(
                    value.config,
                    syncReady: value.syncReady
                )
            },
            onError: { [configStore] error in configStore.showError(error) }
        )
    }

    private func makeSyncStatusProjection()
        -> Projection<BridgeSyncStatusSnapshot>
    {
        Projection(
            domain: .syncStatus,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    appHandle.getSyncStatus()
                }
            },
            apply: { [configStore] snapshot in
                configStore.applySyncStatusSnapshot(snapshot)
            },
            onError: { [configStore] error in configStore.showError(error) }
        )
    }

    private func makeDownloadProjection()
        -> Projection<BridgeDownloadSnapshot>
    {
        Projection(
            domain: .downloadQueue,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    appHandle.getDownloadSnapshot()
                }
            },
            apply: { [downloadStore] snapshot in
                downloadStore.snapshot = snapshot
            },
            onError: { [configStore] error in configStore.showError(error) }
        )
    }

    private struct ReleaseDetailProjectionValue: Sendable {
        let releaseId: String
        let release: BridgeRelease?
    }

    private func makeReleaseDetailProjection()
        -> Projection<ReleaseDetailProjectionValue>
    {
        Projection(
            domain: .release,
            query: { [appHandle] invalidation in
                guard case .release(let releaseId) = invalidation else {
                    preconditionFailure(
                        "Release projection received \(invalidation)"
                    )
                }
                let release = try await DetachedWork.run {
                    try appHandle.findReleaseDetail(releaseId: releaseId)
                }
                return ReleaseDetailProjectionValue(
                    releaseId: releaseId,
                    release: release
                )
            },
            apply: { [libraryStore] value in
                libraryStore.applyReleaseDetailSnapshot(
                    releaseId: value.releaseId,
                    bridge: value.release
                )
            },
            onError: { [configStore] error in configStore.showError(error) }
        )
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
        mediaControlService.updateCommandAvailability(
            hasNext: snapshot.hasNext,
            hasPrevious: snapshot.hasPrevious
        )
    }
}
