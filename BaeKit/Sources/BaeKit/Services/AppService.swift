import Combine
import Foundation
import Observation
import SwiftUI

/// The shared services one platform `AppService` constructs before handing
/// ownership to its BaeKit base. Its fields are deliberately hidden: the value
/// crosses the superclass-initialization boundary, but none of the retained
/// services can be unpacked afterward.
public struct AppServiceComponents {
    fileprivate let playbackStore: PlaybackStore
    fileprivate let configStore: ConfigStore
    fileprivate let membershipStore: MembershipStore
    fileprivate let syncStatusStore: SyncStatusStore
    fileprivate let syncActivityStore: SyncActivityStore
    fileprivate let downloadStore: DownloadStore
    fileprivate let castStore: CastStore
    fileprivate let outboxStore: OutboxStore
    fileprivate let library: Library
    fileprivate let playback: Playback
    fileprivate let queue: Queue
    fileprivate let mediaPaths: MediaPaths
    fileprivate let imageStore: ImageStore
    fileprivate let sync: Sync
    fileprivate let downloads: Downloads
    fileprivate let cast: Cast

    public init(
        playbackStore: PlaybackStore,
        configStore: ConfigStore,
        syncStatusStore: SyncStatusStore,
        membershipStore: MembershipStore,
        syncActivityStore: SyncActivityStore,
        downloadStore: DownloadStore,
        castStore: CastStore,
        outboxStore: OutboxStore,
        library: Library,
        playback: Playback,
        queue: Queue,
        mediaPaths: MediaPaths,
        imageStore: ImageStore,
        sync: Sync,
        downloads: Downloads,
        cast: Cast
    ) {
        self.playbackStore = playbackStore
        self.configStore = configStore
        self.syncStatusStore = syncStatusStore
        self.membershipStore = membershipStore
        self.syncActivityStore = syncActivityStore
        self.downloadStore = downloadStore
        self.castStore = castStore
        self.outboxStore = outboxStore
        self.library = library
        self.playback = playback
        self.queue = queue
        self.mediaPaths = mediaPaths
        self.imageStore = imageStore
        self.sync = sync
        self.downloads = downloads
        self.cast = cast
    }
}

#if DEBUG
    public struct AppServiceTestState {
        public let nowPlaying: NowPlaying
        public let manualQueueEntryIds: [String]
        public let settings: PlaybackSettings?
    }
#endif

/// Root application object for one unlocked library — the platform-shared base.
/// Not `@Observable`: the reactive state lives on the contained stores. Conforms
/// to `Observable` (empty) so it can sit in SwiftUI's `@Environment`.
///
/// Each platform subclasses this with its own stored services (the desktop
/// import/export layer on macOS) and its own `init` / `wireUp`; the shared
/// stores, domain services, common projections, and media-control plumbing live
/// here. One instance per unlocked library: switching libraries builds a fresh
/// one with fresh stores.
@MainActor
open class AppService: @unchecked Sendable, Observable {
    // The immutable `Sendable` handle and domain services are marked
    // `nonisolated` so they read from any context (the app delegates touch
    // `appHandle`/`sync` off the main actor when tearing a library down). Within
    // a module the compiler infers this for `let`s of `Sendable` type on a
    // `@MainActor` class; across the package boundary it must be explicit.

    /// Handle to bae-library via the uniffi bridge. All library, playback, and
    /// sync requests go through this.
    private nonisolated let appHandle: AppHandle

    /// The process-lifetime telemetry sink (built at launch, before this
    /// library opened). Host-originated events route through it; unlike
    /// `appHandle` it outlives any one library, so switching libraries keeps the
    /// same sink.
    private nonisolated let diagnostics: BridgeDiagnostics

    /// Drives the platform Now Playing surface and remote-control events, kept
    /// in sync with `playbackStore.nowPlaying`.
    private nonisolated let mediaControlService: MediaControlService

    // MARK: - Stores

    private let playbackStore: PlaybackStore
    private let configStore: ConfigStore
    private let syncStatusStore: SyncStatusStore
    private let syncActivityStore: SyncActivityStore
    /// The bytes each downloading release has moved.
    private let downloadStore: DownloadStore
    /// Which device playback is on, and what the picker found while it was open.
    private let castStore: CastStore
    /// Cloud outbox mirror — which releases have uploads queued, the
    /// sync-pause flag, and each upload's transfer.
    private let outboxStore: OutboxStore

    private let membershipStore: MembershipStore
    private let commonSubscriptions: CommonSubscriptions
    private let libraryProjectionStore: LibraryProjectionStore

    // MARK: - Domain services
    //
    // Narrow, per-domain projections of `AppHandle` that views read via
    // `@Environment(...)` instead of reaching for `appService.appHandle`.

    private nonisolated let library: Library
    private nonisolated let playback: Playback
    private nonisolated let queue: Queue
    private nonisolated let mediaPaths: MediaPaths
    /// The app's image pipeline: fetch, decode-at-size, bounded decoded cache.
    private nonisolated let imageStore: ImageStore
    private nonisolated let sync: Sync
    private nonisolated let downloads: Downloads
    /// Cast transport: browse for devices, and move playback to or from one.
    private nonisolated let cast: Cast
    private let playbackValues: PlaybackValueHandler

    #if DEBUG
        private let testAccess: AppServiceTestAccess
    #endif

    /// Build the shared half of an `AppService` around an already-open,
    /// already-unlocked `AppHandle` and the services constructed for it. Pure
    /// field assignment; every side effect lives in the subclass `wireUp()`
    /// after construction.
    public init(
        appHandle: AppHandle,
        mediaControlService: MediaControlService,
        diagnostics: BridgeDiagnostics,
        components: AppServiceComponents
    ) {
        self.appHandle = appHandle
        membershipStore = components.membershipStore
        self.mediaControlService = mediaControlService
        self.diagnostics = diagnostics
        playbackStore = components.playbackStore
        configStore = components.configStore
        syncStatusStore = components.syncStatusStore
        syncActivityStore = components.syncActivityStore
        downloadStore = components.downloadStore
        castStore = components.castStore
        outboxStore = components.outboxStore
        commonSubscriptions = CommonSubscriptions(
            appHandle: appHandle,
            configStore: components.configStore,
            syncStatusStore: components.syncStatusStore,
            syncActivityStore: components.syncActivityStore,
            outboxStore: components.outboxStore,
            downloadStore: components.downloadStore,
            playbackStore: components.playbackStore,
            castStore: components.castStore
        )
        libraryProjectionStore = LibraryProjectionStore(
            library: components.library
        )
        library = components.library
        playback = components.playback
        queue = components.queue
        mediaPaths = components.mediaPaths
        imageStore = components.imageStore
        sync = components.sync
        downloads = components.downloads
        cast = components.cast
        playbackValues = PlaybackValueHandler(
            playbackStore: components.playbackStore,
            castStore: components.castStore
        )
        #if DEBUG
            testAccess = AppServiceTestAccess(
                playbackStore: components.playbackStore
            )
        #endif
    }

    public var libraryId: String { configStore.config.libraryId }
    public var libraryPath: String { configStore.config.libraryPath }

    /// Close the library so nothing of this process holds its store: save
    /// where playback was, stop its services and sync, close the store, and
    /// shut the core's runtime down, off the main actor, writing each step to
    /// `closing` when the app shows the close. The library is closed whether
    /// or not this throws: a failure says
    /// what went wrong while closing it. Removing a library instead calls
    /// `removeLibrary` while this service owns the store.
    public func close(showing closing: LibraryClosingSteps?) async throws {
        let bridgeClosing = closing?.closing
        try await DetachedWork.run { [appHandle] in
            try appHandle.closeLibrary(closing: bridgeClosing)
        }
    }

    public func triggerSync() {
        sync.nudges.triggerSync()
    }

    public nonisolated func renameLibrary(
        to name: String
    ) async throws {
        try await sync.renameLibrary(name)
    }

    /// What removing this library would lose, for the person to confirm
    /// first. The library keeps running.
    public func unsentBeforeRemoval() async throws -> BridgeUnsentWork {
        try await appHandle.unsentBeforeRemoval()
    }

    /// Remove this library from this device, discarding what it hasn't
    /// uploaded, writing each step to `removal`, whose steps the app shows.
    public func removeLibrary(showing removal: LibraryClosingSteps) async throws
        -> BridgeLibraryRemoval
    {
        let removed = try await appHandle.removeLibrary(
            closing: removal.closing
        )
        // The store is gone. Ending the session ends the core's runtime off
        // the main actor, so releasing the handle has nothing left to wait for.
        try await DetachedWork.run { [appHandle] in
            try appHandle.closeLibrary(closing: nil)
        }
        return removed
    }

    #if os(macOS)
        public func activateMediaControls(previewAudio: PreviewAudio) {
            mediaControlService.activate(
                playback: playback,
                previewAudio: previewAudio,
                playbackStore: playbackStore
            )
        }

        public func deactivateMediaControls() {
            mediaControlService.deactivate(playbackStore: playbackStore)
        }

        public func registerArtworkAnalyzer(
            _ analyzer: ArtworkAnalyzerCallback
        ) {
            appHandle.registerArtworkAnalyzer(analyzer: analyzer)
        }
    #endif

    /// Report that a host UI screen was opened, as a typed telemetry event,
    /// through the process-lifetime telemetry sink. Infallible — telemetry must
    /// never affect navigation. This is the only host-originated event;
    /// everything else the core emits itself. Local logging is separate
    /// (`BaeLogger`) and stays on-device.
    public nonisolated func reportScreen(_ screen: BridgeScreen) {
        diagnostics.event(event: .screenOpened(screen: screen))
    }

    /// Route a caught error to the platform's error surface. An error core says
    /// has no line — a cancellation — is dropped rather than surfaced empty.
    public func showError(_ error: any Error) {
        guard let displayed = DisplayError(error) else { return }
        showError(displayed)
    }

    /// Route a playback failure. It is not a Swift `Error`, so it needs its own
    /// door; it drops on a nil line for the same reason.
    public func showError(_ reason: BridgePlaybackErrorReason) {
        guard let displayed = DisplayError(reason) else { return }
        showError(displayed)
    }

    /// Route a display error to the platform's error surface. The macOS
    /// subclass overrides this to route through its global alert (`UiStore`).
    open func showError(_ error: DisplayError) {
        preconditionFailure(
            "macOS AppService must override showError to route through UiStore"
        )
    }

    public func startCommonSubscriptions() {
        membershipStore.start()
        commonSubscriptions.start(
            applyPlayback: { [weak self] values in
                guard let self else { return }
                playbackValues.apply(values)
                // A playback failure has no screen of its own: it shows
                // wherever the person is, once.
                if let reason = playbackValues.failureToShow(values) {
                    showError(reason)
                }
                mediaControlService.applyValues(
                    values,
                    appHandle: appHandle
                )
                applyPlatformPlaybackValues(values)
            },
            applyTimeline: { [weak self] timeline in
                self?.mediaControlService.applyTimeline(timeline)
            },
            applyQueueAddition: { [weak self] addition in
                self?.playbackValues.applyQueueAddition(addition)
            },
            applyQueue: { [weak self] snapshot in
                self?.playbackValues.applyQueueSnapshot(snapshot)
                self?.mediaControlService
                    .updateCommandAvailability(
                        hasNext: snapshot.hasNext,
                        hasPrevious: snapshot.hasPrevious
                    )
            },
            onError: { [weak self] error in self?.showError(error) }
        )
    }

    open func applyPlatformPlaybackValues(_: BridgePlaybackValues) {}

    #if DEBUG
        public var stateForTesting: AppServiceTestState {
            testAccess.state
        }

        public var queueItemsAddedPublisherForTesting: AnyPublisher<Int, Never>
        {
            testAccess.queueItemsAddedPublisher
        }

        public var syncStatusStoreForTesting: SyncStatusStore {
            syncStatusStore
        }

        public var syncActivityStoreForTesting: SyncActivityStore {
            syncActivityStore
        }

    #endif
}

extension AppService {
    public func installSharedEnvironment<Content: View>(
        _ content: Content
    ) -> some View {
        content
            .environment(playbackStore)
            .environment(configStore)
            .environment(syncStatusStore)
            .environment(membershipStore)
            .environment(syncActivityStore)
            .environment(libraryProjectionStore)
            .environment(downloadStore)
            .environment(outboxStore)
            .environment(downloads)
            .environment(imageStore)
            .environment(library)
            .environment(playback)
            .environment(queue)
            .environment(sync)
            .environment(cast)
            .environment(castStore)
            .environment(mediaPaths)
    }
}
