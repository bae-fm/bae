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

    /// What the main menu's commands for this library act through. It holds
    /// the same service and store instances installed in the view tree.
    let mainAppMenuTarget: MainAppMenuTarget

    /// The percent each exporting release has reached.
    private let outputStore = OutputStore()

    /// Owns the desktop value subscriptions and their apply wiring.
    private let desktopSubscriptions: DesktopSubscriptions
    private let desktopEvents: DesktopEventHandler

    /// The library browser's session state (lists, selections, sort criteria).
    /// Construction wires its subscriptions; `wireUp()` starts delivery
    /// after the complete application service has been initialized.
    private let libraryBrowseSession: LibraryBrowseSession
    private let storageManagerStore: StorageManagerStore
    private let settingsNavigation = SettingsNavigation()

    // MARK: - Desktop-only domain services

    private let previewAudio: PreviewAudio
    private let releaseEditor: ReleaseEditor
    private let artistEditor: ArtistEditor
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
        seed: LibrarySessionSeed,
        deviceName: String
    ) {
        let playbackStore = PlaybackStore()
        let configStore = ConfigStore(
            config: Config(bridge: config),
            libraryName: seed.name,
            storage: seed.storage
        )
        let syncStatusStore = SyncStatusStore()
        let membershipStore = MembershipStore(handle: appHandle)
        let syncActivityStore = SyncActivityStore(
            retryNow: { try await appHandle.retryNow() },
            dismissAbandoned: { record in
                try await appHandle.dismissAbandonedAttachment(record: record)
            }
        )
        let downloadStore = DownloadStore()
        let castStore = CastStore()
        let outboxStore = OutboxStore(uploadsPaused: seed.uploadsPaused)
        let library = Library(handle: appHandle)
        let playback = Playback(handle: appHandle)
        let queue = Queue(handle: appHandle)
        let mediaPaths = MediaPaths(handle: appHandle)
        let imageStore = ImageStore(handle: appHandle)
        let sync = Sync(handle: appHandle, deviceName: deviceName)
        let downloads = Downloads(handle: appHandle)
        let cast = Cast(handle: appHandle)
        let importStore = ImportStore()
        let importer = Importer(handle: appHandle)
        importStore.sessionWriter = CandidateSessionWriter(
            importer: importer,
            reportFailure: { uiStore.showError($0) }
        )
        self.uiStore = uiStore
        self.importStore = importStore
        previewAudio = PreviewAudio(handle: appHandle)
        releaseEditor = ReleaseEditor(handle: appHandle)
        artistEditor = ArtistEditor(handle: appHandle)
        self.importer = importer
        outputs = Outputs(handle: appHandle)
        discogs = Discogs(handle: appHandle)
        automation = Automation(handle: appHandle)
        subsonic = SubsonicServer(handle: appHandle)
        export = TrackSave(handle: appHandle)
        #if BAE_OAUTH_PROVIDERS
            cloudSyncSetup = CloudSyncSetup(
                handle: appHandle,
                deviceName: deviceName
            )
        #endif
        let libraryBrowseSession = LibraryBrowseSession(
            library: library,
            uiStore: uiStore
        )
        self.libraryBrowseSession = libraryBrowseSession
        storageManagerStore = StorageManagerStore(
            library: library,
            onError: { uiStore.showError($0) }
        )
        desktopSubscriptions = DesktopSubscriptions(
            appHandle: appHandle,
            importStore: importStore,
            outputStore: outputStore,
            outboxStore: outboxStore,
            downloadStore: downloadStore,
            uiStore: uiStore
        )
        desktopEvents = DesktopEventHandler(importStore: importStore)
        mainAppMenuTarget = MainAppMenuTarget(
            playbackStore: playbackStore,
            configStore: configStore,
            libraryBrowseSession: libraryBrowseSession,
            uiStore: uiStore,
            library: library,
            playback: playback,
            membershipStore: membershipStore
        )
        let components = AppServiceComponents(
            playbackStore: playbackStore,
            configStore: configStore,
            syncStatusStore: syncStatusStore,
            membershipStore: membershipStore,
            syncActivityStore: syncActivityStore,
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
        desktopEvents.applyPreview(values.preview)
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
            .environment(desktopSubscriptions.importList)
            .environment(desktopSubscriptions.importSelection)
            .environment(libraryBrowseSession)
            .environment(storageManagerStore)
            .environment(settingsNavigation)
            .environment(previewAudio)
            .environment(releaseEditor)
            .environment(artistEditor)
            .environment(importer)
            .environment(outputs)
            .environment(discogs)
            .environment(automation)
            .environment(subsonic)
            .environment(export)
            .environment(outputStore)
            .environment(uiStore)
            .environment(\.previewPosition, importStore.previewPosition.only)
            .environment(\.candidateRuntimes, importStore.candidateRuntimes)
            .environment(\.importPercents, importStore.importPercents)
            .environment(
                \.candidateSignalsPublisher,
                candidateSignalsPublisher
            )
            #if BAE_OAUTH_PROVIDERS
                .environment(cloudSyncSetup)
            #endif
    }

    private var candidateSignalsPublisher:
        AnyPublisher<CandidateSignalsEvent, Never>
    {
        importStore.candidateSignalsSubject.eraseToAnyPublisher()
    }

    /// Where Finder's Open With hands a folder: the same entry the window's
    /// picker and drop use.
    var importFolderEntry: ImportFolderEntry {
        ImportFolderEntry(importer: importer, uiStore: uiStore)
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
