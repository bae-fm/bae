import AppKit
import os.log

private let logger = Logger.bae("AppService")

/// Root application object. Not @Observable — reactive state lives on the
/// contained stores. Conforms to Observable (empty — no tracked properties)
/// so it can be placed in SwiftUI's @Environment.
///
/// One `AppService` per unlocked library: when the user switches libraries,
/// a fresh instance (with fresh stores and `uiStore`) is built.
@MainActor
final class AppService: @unchecked Sendable, Observable {
    /// Handle to bae-core via the uniffi bridge. All library, playback,
    /// import, and sync requests go through this.
    let appHandle: AppHandle

    /// Drives macOS Now Playing info and remote control events (play/pause
    /// from AirPods, menubar media keys, etc.). Kept in sync with
    /// `playbackStore.nowPlaying`.
    let mediaControlService = MediaControlService()

    /// Playback mirror — `nowPlaying`, queue, volume, mute, repeat mode.
    /// Reducer is the sole writer.
    let playbackStore: PlaybackStore

    /// Library configuration mirror — `Config` (library, discogs, sync
    /// settings). Reducer is the sole writer.
    let configStore: ConfigStore

    /// Import-flow session state — folder candidates and the preview audio
    /// state. Mixed-writer: reducer drives event-driven fields (scan, identify,
    /// preview state); views drive user-set fields (mode, selectedCover).
    let importStore: ImportStore

    /// Entity slices — the library cache. Keyed-by-id maps of the albums
    /// and releases the client currently knows about. Populated by
    /// event reducers, paginated-list ingest, and on-demand loaders; views
    /// read fields at the leaf through `@Observable`. See
    /// `Services/PaginatedList.swift` and `notes/app-state-paginated-lists.md`
    /// for the slice / list / intern / clear vocabulary.
    let libraryStore: LibraryStore

    /// Navigation and selection state — which album is expanded, which
    /// release is selected within each album, scroll-to-album commands.
    /// Persists across view transitions within a library session; a new
    /// library gets a new `UiStore`.
    let uiStore: UiStore

    /// Cloud outbox processing mirror — queue depth, per-item state, summary.
    /// Reducer is the sole writer; the Storage Manager's queue panel reads it.
    let outboxStore: OutboxStore

    /// In-memory download (pin) queue mirror — per-release state and summary.
    /// Reducer is the sole writer; the Storage Manager's Downloads pane reads it.
    let downloadStore: DownloadStore

    /// In-memory export queue mirror — per-release state and summary. Reducer is
    /// the sole writer; the Storage Manager's Exporting pane reads it.
    let exportStore: ExportStore

    /// Query-backed projections refreshed by scoped invalidation events.
    let projectionRegistry = ProjectionRegistry()
    private var projectionRegistrations: [ProjectionRegistration] = []

    // MARK: - Domain services
    //
    // Narrow, per-domain projections of `AppHandle` that views read via
    // `@Environment(...)` instead of reaching for `appService.appHandle`.
    // Constructed here at one place; injected one-by-one by `BaeApp`.

    let mediaPaths: MediaPaths
    let playback: Playback
    let queue: Queue
    let previewAudio: PreviewAudio
    let library: Library
    let releaseEditor: ReleaseEditor
    let importer: Importer
    let sync: Sync
    let downloads: Downloads
    let exports: Exports
    let discogs: Discogs
    let automation: Automation
    let export: Export

    /// Build an `AppService` around an already-open, already-unlocked
    /// `AppHandle` and a config snapshot the caller has already read off
    /// it. Pure field assignment — every side effect (event subscription,
    /// artwork analyzer registration, media-control setup, restore-code
    /// persistence) lives in `BaeApp.openLibrary` after construction.
    init(
        appHandle: AppHandle,
        uiStore: UiStore,
        config: BridgeConfig
    ) {
        self.appHandle = appHandle
        self.uiStore = uiStore
        playbackStore = PlaybackStore()
        configStore = ConfigStore(
            config: Config(bridge: config),
            syncReady: appHandle.isSyncReady()
        )
        importStore = ImportStore()
        libraryStore = LibraryStore()
        // Seed the queue panel with the current outbox so it renders correct
        // state on first open, before any `outboxChanged` event arrives. A
        // read failure isn't fatal — log it and start empty; the next event
        // refreshes the panel.
        let initialOutbox: BridgeOutboxSnapshot
        do {
            initialOutbox = try appHandle.getOutboxSnapshot()
        }
        catch {
            logger.error("Failed to seed outbox snapshot: \(error)")
            initialOutbox = OutboxStore.emptySnapshot
        }
        outboxStore = OutboxStore(snapshot: initialOutbox)
        // Seed the Downloads pane from the in-memory download queue. The read is
        // infallible — `getDownloadSnapshot` never throws — so unlike the outbox
        // above there's no fallback branch.
        downloadStore = DownloadStore(snapshot: appHandle.getDownloadSnapshot())
        // Seed the Exporting pane from the in-memory export queue. Like the
        // download read above, `getExportSnapshot` is infallible — no fallback.
        exportStore = ExportStore(snapshot: appHandle.getExportSnapshot())
        mediaPaths = MediaPaths(handle: appHandle)
        playback = Playback(handle: appHandle)
        queue = Queue(handle: appHandle)
        previewAudio = PreviewAudio(handle: appHandle)
        library = Library(handle: appHandle)
        releaseEditor = ReleaseEditor(handle: appHandle)
        importer = Importer(handle: appHandle)
        sync = Sync(handle: appHandle)
        downloads = Downloads(handle: appHandle)
        exports = Exports(handle: appHandle)
        discogs = Discogs(handle: appHandle)
        automation = Automation(handle: appHandle)
        export = Export(handle: appHandle)
    }

    /// Wire the live `AppHandle` into the stores: subscribe to Rust UI
    /// events, register the artwork analyzer, set up macOS media
    /// remote-control bindings. Called once by `BaeApp.openLibrary`
    /// after construction; previews skip it.
    func wireUp() {
        registerRootProjections()
        appHandle.subscribeUiEvents(
            callback: UiEventHandler(
                playbackStore: playbackStore,
                configStore: configStore,
                importStore: importStore,
                libraryStore: libraryStore,
                projectionRegistry: projectionRegistry,
                appService: self,
                uiStore: uiStore,
                outboxStore: outboxStore,
                downloadStore: downloadStore,
                exportStore: exportStore
            )
        )
        appHandle.registerArtworkAnalyzer(analyzer: VisionArtworkAnalyzer())
        mediaControlService.setupRemoteCommands(
            playback: playback,
            previewAudio: previewAudio,
            playbackStore: playbackStore
        )
        revalidateDiscogsToken()
    }

    private func registerRootProjections() {
        precondition(projectionRegistrations.isEmpty)
        projectionRegistrations = [
            projectionRegistry.register(makeQueueProjection()),
            projectionRegistry.register(makeConfigProjection()),
            projectionRegistry.register(makeSyncStatusProjection()),
            projectionRegistry.register(makeOutboxProjection()),
            projectionRegistry.register(makeDownloadProjection()),
            projectionRegistry.register(makeExportProjection()),
            projectionRegistry.register(makeReleaseDetailProjection()),
        ]
    }

    private func makeQueueProjection() -> Projection<BridgeQueueSnapshot> {
        Projection(
            domain: .queue,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    try appHandle.getQueueSnapshot()
                }
            },
            apply: { [self] snapshot in
                applyQueueSnapshot(snapshot)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
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
            onError: { [uiStore] error in uiStore.showError(error) }
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
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeOutboxProjection() -> Projection<BridgeOutboxSnapshot> {
        Projection(
            domain: .outbox,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    try appHandle.getOutboxSnapshot()
                }
            },
            apply: { [outboxStore] snapshot in
                outboxStore.applySnapshot(snapshot)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
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
                downloadStore.applySnapshot(snapshot)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeExportProjection() -> Projection<BridgeExportSnapshot> {
        Projection(
            domain: .exportQueue,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    appHandle.getExportSnapshot()
                }
            },
            apply: { [exportStore] snapshot in
                exportStore.applySnapshot(snapshot)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
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
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
        mediaControlService.updateCommandAvailability(
            hasNext: snapshot.hasNext,
            hasPrevious: snapshot.hasPrevious
        )
    }

    /// Re-check a Discogs key that was saved while offline. App-launch half of
    /// the deferred validation (the settings tab covers tab-open, a real search
    /// covers use-time). Core no-ops unless the stored key is `Unvalidated`, so
    /// this calls unconditionally rather than inspecting the status here.
    private func revalidateDiscogsToken() {
        let discogs = discogs
        Task {
            do { try await discogs.revalidateDiscogsToken() }
            catch {
                logger.error("Discogs revalidation on launch failed: \(error)")
            }
        }
    }

}
