import AppKit
import BaeKit
import Combine
import SwiftUI
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
final class AppService: BaeKit.AppService, @unchecked Sendable {
    /// Import-flow session state — folder candidates and the preview audio
    /// state. Mixed-writer: core drives scan/identify and preview state through
    /// retained values; views drive user-set fields (mode, selectedCover).
    private let importStore: ImportStore

    /// Navigation and selection state — which album is expanded, which release
    /// is selected within each album, scroll-to-album commands. A new library
    /// gets a new `UiStore`.
    private let uiStore: UiStore

    /// The focused receiver for menu commands that act on this library.
    /// It holds the same service and store instances installed in the view tree.
    let mainAppMenuTarget: MainAppMenuTarget

    /// In-memory export queue mirror — per-release state and summary. The export
    /// value stream is the sole writer; the Storage Manager's Exporting pane reads
    /// it.
    private let outputStore: OutputStore

    /// Owns the desktop value subscriptions and their apply wiring.
    private let desktopSubscriptions: DesktopSubscriptions
    private let desktopEvents: DesktopEventHandler

    /// The library browser's session state (lists, selections, sort criteria).
    /// Construction wires its subscriptions; `wireUp()` starts delivery
    /// after the complete application service has been initialized.
    private let libraryBrowseSession: LibraryBrowseSession
    private let storageManagerStore: StorageManagerStore

    // MARK: - Desktop-only domain services

    private let previewAudio: PreviewAudio
    private let releaseEditor: ReleaseEditor
    private let importer: Importer
    private let outputs: Outputs
    private let discogs: Discogs
    private let automation: Automation
    private let subsonic: SubsonicServer
    private let export: TrackSave
    #if BAE_OAUTH_PROVIDERS
        private let cloudSyncSetup: CloudSyncSetup
    #endif

    // This is the composition root: every retained capability is constructed
    // here and assigned directly to its owner before the superclass starts.
    // swiftlint:disable:next function_body_length
    init(
        appHandle: AppHandle,
        mediaControlService: MediaControlService,
        diagnostics: BridgeDiagnostics,
        uiStore: UiStore,
        config: BridgeConfig,
        initialOutbox: BridgeOutboxSnapshot
    ) {
        let playbackStore = PlaybackStore()
        let configStore = ConfigStore(config: Config(bridge: config))
        let syncStatusStore = SyncStatusStore()
        let libraryStore = LibraryStore()
        let downloadStore = DownloadStore(
            snapshot: appHandle.getDownloadSnapshot()
        )
        let castStore = CastStore()
        let outboxStore = OutboxStore(snapshot: initialOutbox)
        let library = Library(handle: appHandle)
        let playback = Playback(handle: appHandle)
        let queue = Queue(handle: appHandle)
        let mediaPaths = MediaPaths(handle: appHandle)
        let imageStore = ImageStore(handle: appHandle)
        let sync = Sync(handle: appHandle)
        let downloads = Downloads(handle: appHandle)
        let cast = Cast(handle: appHandle)
        let importStore = ImportStore()
        let importer = Importer(handle: appHandle)
        self.uiStore = uiStore
        self.importStore = importStore
        // Seed the Exporting pane from the in-memory export queue.
        // `getOutputSnapshot` is infallible — no fallback.
        outputStore = OutputStore(snapshot: appHandle.getOutputSnapshot())
        previewAudio = PreviewAudio(handle: appHandle)
        releaseEditor = ReleaseEditor(
            handle: appHandle,
            outboxStore: outboxStore
        )
        self.importer = importer
        outputs = Outputs(handle: appHandle)
        discogs = Discogs(handle: appHandle)
        automation = Automation(handle: appHandle)
        subsonic = SubsonicServer(handle: appHandle)
        export = TrackSave(handle: appHandle)
        #if BAE_OAUTH_PROVIDERS
            cloudSyncSetup = CloudSyncSetup(handle: appHandle)
        #endif
        libraryBrowseSession = LibraryBrowseSession(
            library: library,
            libraryStore: libraryStore,
            uiStore: uiStore
        )
        storageManagerStore = StorageManagerStore(
            library: library,
            libraryStore: libraryStore,
            onError: { uiStore.showError($0) }
        )
        desktopSubscriptions = DesktopSubscriptions(
            appHandle: appHandle,
            importStore: importStore,
            outputStore: outputStore,
            uiStore: uiStore
        )
        desktopEvents = DesktopEventHandler(
            importStore: importStore,
            mediaControlService: mediaControlService
        )
        mainAppMenuTarget = MainAppMenuTarget(
            playbackStore: playbackStore,
            configStore: configStore,
            libraryStore: libraryStore,
            importStore: importStore,
            uiStore: uiStore,
            library: library,
            playback: playback,
            importer: importer
        )
        let components = AppServiceComponents(
            playbackStore: playbackStore,
            configStore: configStore,
            syncStatusStore: syncStatusStore,
            libraryStore: libraryStore,
            downloadStore: downloadStore,
            castStore: castStore,
            outboxStore: outboxStore,
            library: library,
            playback: playback,
            queue: queue,
            mediaPaths: mediaPaths,
            imageStore: imageStore,
            sync: sync,
            downloads: downloads,
            cast: cast
        )
        super
            .init(
                appHandle: appHandle,
                mediaControlService: mediaControlService,
                diagnostics: diagnostics,
                components: components
            )
    }

    override func showError(_ error: DisplayError) {
        uiStore.showError(error)
    }

    override func applyPlatformPlaybackValues(_ values: BridgePlaybackValues) {
        desktopEvents.apply(values.preview)
    }

    /// Wire the live `AppHandle` into the stores: start the common and desktop
    /// value subscriptions, subscribe to Rust UI events, register the artwork
    /// analyzer, set up macOS media remote-control bindings, and re-check a
    /// deferred Discogs token.
    /// Called once after construction; previews skip it.
    func wireUp() {
        libraryBrowseSession.start()
        startCommonSubscriptions()
        desktopSubscriptions.start()
        subscribeUIEvents { [weak self] event in
            guard let self else { return }
            DesktopUiEvents.apply(event, appService: self)
        }
        registerArtworkAnalyzer(VisionArtworkAnalyzer())
        activateMediaControls(previewAudio: previewAudio)
        revalidateDiscogsToken()
    }

    func installEnvironment<Content: View>(_ content: Content) -> some View {
        installSharedEnvironment(content)
            .environment(importStore)
            .environment(libraryBrowseSession)
            .environment(storageManagerStore)
            .environment(previewAudio)
            .environment(releaseEditor)
            .environment(importer)
            .environment(outputs)
            .environment(discogs)
            .environment(automation)
            .environment(subsonic)
            .environment(export)
            .environment(outputStore)
            .environment(uiStore)
            .environment(
                \.playbackPositionPublisher,
                playbackPositionPublisher
            )
            .environment(\.previewProgressPublisher, previewProgressPublisher)
            .environment(\.importLoudnessPublisher, importLoudnessPublisher)
            #if BAE_OAUTH_PROVIDERS
                .environment(cloudSyncSetup)
            #endif
    }

    static func installEnvironment<Content: Scene>(
        _ content: Content,
        from service: AppService?
    ) -> some Scene {
        BaeKit.AppService.installSharedEnvironment(content, from: service)
            .environment(service?.importStore)
            .environment(service?.libraryBrowseSession)
            .environment(service?.storageManagerStore)
            .environment(service?.previewAudio)
            .environment(service?.releaseEditor)
            .environment(service?.importer)
            .environment(service?.outputs)
            .environment(service?.discogs)
            .environment(service?.automation)
            .environment(service?.subsonic)
            .environment(service?.export)
            .environment(service?.outputStore)
            .environment(service?.uiStore)
            .environment(
                \.playbackPositionPublisher,
                service?.playbackPositionPublisher
                    ?? Empty().eraseToAnyPublisher()
            )
            .environment(
                \.previewProgressPublisher,
                service?.previewProgressPublisher
                    ?? Empty().eraseToAnyPublisher()
            )
            .environment(
                \.importLoudnessPublisher,
                service?.importLoudnessPublisher
                    ?? Empty().eraseToAnyPublisher()
            )
            #if BAE_OAUTH_PROVIDERS
                .environment(service?.cloudSyncSetup)
            #endif
    }

    private var previewProgressPublisher:
        AnyPublisher<PreviewProgressEvent, Never>
    {
        importStore.previewProgressSubject.eraseToAnyPublisher()
    }

    private var importLoudnessPublisher:
        AnyPublisher<ImportLoudnessProgressEvent?, Never>
    {
        importStore.importLoudnessSubject.eraseToAnyPublisher()
    }

    func addWatchedFolder(path: String) async throws {
        try await importer.addWatchedFolder(path)
    }

    func applyDesktopUIEvent(_ event: BridgeUiEvent) {
        desktopEvents.apply(event)
    }

    #if DEBUG
        // periphery:ignore
        var hasDisplayedErrorForTesting: Bool {
            uiStore.lastError != nil
        }
    #endif

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
