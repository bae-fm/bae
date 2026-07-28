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
    let outputStore: OutputStore

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
    let outputs: Outputs
    let discogs: Discogs
    let automation: Automation
    let subsonic: SubsonicServer
    let export: TrackSave

    /// Cast transport (discovery + switch) and the observable cast state the
    /// playback bar reads.
    let cast: Cast
    let castStore: CastStore

    init(
        appHandle: AppHandle,
        mediaControlService: MediaControlService,
        diagnostics: BridgeDiagnostics,
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
        // `getOutputSnapshot` is infallible — no fallback.
        outputStore = OutputStore(snapshot: appHandle.getOutputSnapshot())
        previewAudio = PreviewAudio(handle: appHandle)
        releaseEditor = ReleaseEditor(handle: appHandle)
        importer = Importer(handle: appHandle)
        outputs = Outputs(handle: appHandle)
        discogs = Discogs(handle: appHandle)
        automation = Automation(handle: appHandle)
        subsonic = SubsonicServer(handle: appHandle)
        export = TrackSave(handle: appHandle)
        cast = Cast(handle: appHandle)
        castStore = CastStore()
        super
            .init(
                appHandle: appHandle,
                mediaControlService: mediaControlService,
                diagnostics: diagnostics,
                config: config,
                initialOutbox: initialOutbox
            )
    }

    override func showError(_ error: DisplayError) {
        uiStore.showError(error)
    }

    /// Wire the live `AppHandle` into the stores: register the common and
    /// desktop projections, subscribe to Rust UI events, register the artwork
    /// analyzer, set up macOS media remote-control bindings, and re-check a
    /// deferred Discogs token.
    /// Called once after construction; previews skip it.
    func wireUp() {
        registerCommonProjections()
        registerProjection(makeExportProjection())
        registerProjection(makeImportCandidatesProjection())
        registerProjection(makeImportCandidateProjection())
        registerProjection(makeImportTriageQueueProjection())
        registerProjection(makeImportLibraryStatusProjection())
        registerProjection(makeCastDevicesProjection())
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
        revalidateDiscogsToken()
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
    private func makeExportProjection() -> Projection<BridgeOutputSnapshot> {
        Projection(
            domain: .outputQueue,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    appHandle.getOutputSnapshot()
                }
            },
            apply: { [outputStore] snapshot in
                outputStore.applySnapshot(snapshot)
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
            apply: { [importStore, uiStore] snapshot in
                importStore.applyImportCandidatesSnapshot(snapshot)
                uiStore.retainFolderCandidateSelection(
                    in: Set(importStore.folderCandidates.keys)
                )
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    /// The sidebar's sections and tab counts. `getImportTriageQueue` is itself
    /// async (it reads stored verdicts and does a live library check), so unlike
    /// the plain candidate snapshot there is no synchronous value to seed
    /// `ImportStore.triageQueue` with at init — it starts empty and this
    /// projection replaces it on each progressive `.importCandidateList`
    /// invalidation.
    ///
    /// `.release` rides along: another import elsewhere landing a release a
    /// Ready row matches has to flip that row to "already in library" without
    /// its own verdict changing, and this is the only projection that re-runs
    /// the live library check.
    private func makeImportTriageQueueProjection() -> Projection<
        BridgeTriageQueue
    > {
        Projection(
            domains: [
                .importCandidateList, .importCandidate, .watchedFolders,
                .release,
            ],
            query: { [appHandle] _ in
                try await appHandle.getImportTriageQueue()
            },
            apply: { [importStore] queue in
                importStore.triageQueue = queue
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
            apply: { [importStore, uiStore] value in
                importStore.applyImportCandidateSnapshot(
                    key: value.key,
                    snapshot: value.snapshot
                )
                uiStore.retainFolderCandidateSelection(
                    in: Set(importStore.folderCandidates.keys)
                )
            },
            onError: { [uiStore] error in uiStore.showError(error) }
        )
    }

    private func makeCastDevicesProjection() -> Projection<[BridgeCastDevice]> {
        Projection(
            domain: .castDevices,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    appHandle.getCastDevices()
                }
            },
            apply: { [castStore] devices in
                castStore.devices = devices
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
