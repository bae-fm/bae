import AppKit
import BaeKit
import os.log

private let logger = Logger.bae("AppService")

/// macOS `AppService`: the shared `BaeKit.AppService` base plus the desktop-only
/// stores and services (import / export / preview / release editing / Discogs /
/// automation) that must not live in the cross-platform package. Named to shadow
/// the base so the many `@Environment(AppService.self)` view reads stay
/// unchanged.
///
/// One `AppService` per unlocked library: when the user switches libraries, a
/// fresh instance (with fresh stores and `uiStore`) is built.
@MainActor
final class AppService: BaeKit.AppService {
    /// Import-flow session state — folder candidates and the preview audio
    /// state. Mixed-writer: core drives event-driven fields (scan/identify state
    /// via the projection registry, preview state via the shared event
    /// dispatcher); views drive user-set fields (mode, selectedCover).
    let importStore: ImportStore

    /// Navigation and selection state — which album is expanded, which release
    /// is selected within each album, scroll-to-album commands. A new library
    /// gets a new `UiStore`.
    let uiStore: UiStore

    /// In-memory export queue mirror — per-release state and summary. The export
    /// projection is the sole writer; the Storage Manager's Exporting pane reads
    /// it.
    let exportStore: ExportStore

    /// The library browser's session state (lists, selections, sort criteria).
    /// `lazy` because its constructor needs `library`/`projectionRegistry`/
    /// `libraryStore` from the `BaeKit.AppService` base, which aren't set until
    /// `super.init()` returns — deferring to first access (from `BaeApp`'s
    /// environment injection) sidesteps that ordering, the same way
    /// `AppDelegate.opener` defers on `self`.
    lazy var libraryBrowseSession = LibraryBrowseSession(
        library: library,
        projectionRegistry: projectionRegistry,
        libraryStore: libraryStore,
        uiStore: uiStore
    )

    // MARK: - Desktop-only domain services

    let previewAudio: PreviewAudio
    let releaseEditor: ReleaseEditor
    let importer: Importer
    let exports: Exports
    let discogs: Discogs
    let automation: Automation
    let export: Export

    init(
        appHandle: AppHandle,
        mediaControlService: MediaControlService,
        uiStore: UiStore,
        config: BridgeConfig,
        initialOutbox: BridgeOutboxSnapshot
    ) {
        self.uiStore = uiStore
        importStore = ImportStore()
        importStore.applyImportCandidatesSnapshot(
            appHandle.getImportCandidates()
        )
        // Seed the Exporting pane from the in-memory export queue.
        // `getExportSnapshot` is infallible — no fallback.
        exportStore = ExportStore(snapshot: appHandle.getExportSnapshot())
        previewAudio = PreviewAudio(handle: appHandle)
        releaseEditor = ReleaseEditor(handle: appHandle)
        importer = Importer(handle: appHandle)
        exports = Exports(handle: appHandle)
        discogs = Discogs(handle: appHandle)
        automation = Automation(handle: appHandle)
        export = Export(handle: appHandle)
        super
            .init(
                appHandle: appHandle,
                mediaControlService: mediaControlService,
                config: config,
                initialOutbox: initialOutbox
            )
    }

    override func showError(_ error: DisplayError) {
        uiStore.showError(error)
    }

    /// Wire the live `AppHandle` into the stores: register the common and
    /// desktop projections, subscribe to Rust UI events, register the artwork
    /// analyzer, set up macOS media remote-control bindings, start watching
    /// every persisted watched folder, and re-check a deferred Discogs token.
    /// Called once after construction; previews skip it.
    func wireUp() {
        registerCommonProjections()
        registerProjection(makeExportProjection())
        registerProjection(makeImportCandidatesProjection())
        registerProjection(makeImportCandidateProjection())
        registerProjection(makeImportLibraryStatusProjection())
        appHandle.subscribeUiEvents(
            callback: UiEventPump(
                sink: UiEventDispatcher.makeSink(
                    appService: self,
                    onUnhandled: DesktopUiEvents.apply
                )
            )
        )
        appHandle.registerArtworkAnalyzer(analyzer: VisionArtworkAnalyzer())
        mediaControlService.activate(
            playback: playback,
            previewAudio: previewAudio,
            playbackStore: playbackStore
        )
        startWatchingImportFolders()
        revalidateDiscogsToken()
    }

    /// Start the core filesystem watcher on every folder persisted from a
    /// previous session. Core's watcher stays live and self-updating from here
    /// on (a debounced `notify` watch re-scans on every on-disk change), so
    /// this only needs to run once per library open — not on every import-tab
    /// visit, which is what re-running it would amount to if it lived behind a
    /// view's `.task`.
    private func startWatchingImportFolders() {
        do {
            try importer.scanWatchedFolders()
        }
        catch {
            uiStore.showError(
                String(
                    localized: "Scan failed: \(error.localizedDescription)"
                )
            )
        }
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

extension AppService {
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

    private func makeImportCandidatesProjection()
        -> Projection<BridgeImportCandidatesSnapshot>
    {
        Projection(
            domains: [.importCandidateList, .watchedFolders],
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    appHandle.getImportCandidates()
                }
            },
            apply: { [importStore] snapshot in
                importStore.applyImportCandidatesSnapshot(snapshot)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private struct ImportCandidateProjectionValue: Sendable {
        let key: String
        let snapshot: BridgeImportCandidateSnapshot?
    }

    private func makeImportCandidateProjection()
        -> Projection<ImportCandidateProjectionValue>
    {
        Projection(
            domain: .importCandidate,
            query: { [appHandle] invalidation in
                guard case .importCandidate(let key) = invalidation else {
                    preconditionFailure(
                        "Import candidate projection received \(invalidation)"
                    )
                }
                let snapshot = try await DetachedWork.run {
                    appHandle.getCandidate(key: key)
                }
                return ImportCandidateProjectionValue(
                    key: key,
                    snapshot: snapshot
                )
            },
            apply: { [importStore] value in
                importStore.applyImportCandidateSnapshot(
                    key: value.key,
                    snapshot: value.snapshot
                )
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeImportLibraryStatusProjection() -> Projection<String> {
        Projection(
            domain: .release,
            query: { invalidation in
                guard case .release(let releaseId) = invalidation else {
                    preconditionFailure(
                        "Import library-status projection received \(invalidation)"
                    )
                }
                return releaseId
            },
            apply: { [importStore] releaseId in
                importStore.removeLibraryStatus(releaseId: releaseId)
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }
}
