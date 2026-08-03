import Foundation
import Observation

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

    /// Handle to bae-core via the uniffi bridge. All library, playback, and
    /// sync requests go through this.
    public nonisolated let appHandle: AppHandle

    /// The process-lifetime telemetry sink (built at launch, before this
    /// library opened). Host-originated events route through it; unlike
    /// `appHandle` it outlives any one library, so switching libraries keeps the
    /// same sink.
    public nonisolated let diagnostics: BridgeDiagnostics

    /// Drives the platform Now Playing surface and remote-control events, kept
    /// in sync with `playbackStore.nowPlaying`.
    public nonisolated let mediaControlService: MediaControlService

    // MARK: - Stores

    public let playbackStore: PlaybackStore
    public let configStore: ConfigStore
    public let libraryStore: LibraryStore
    /// In-memory download (pin) queue mirror — per-release state and summary.
    public let downloadStore: DownloadStore
    /// Which device playback is on, and what the picker found while it was open.
    public let castStore: CastStore
    /// Cloud outbox processing mirror — queue depth, per-item state, summary
    /// (carries the sync-pause flag).
    public let outboxStore: OutboxStore

    /// Query-backed projections refreshed by scoped invalidation events.
    public let projectionRegistry = ProjectionRegistry()
    private var projectionRegistrations: [ProjectionRegistration] = []

    // MARK: - Domain services
    //
    // Narrow, per-domain projections of `AppHandle` that views read via
    // `@Environment(...)` instead of reaching for `appService.appHandle`.

    public nonisolated let library: Library
    public nonisolated let playback: Playback
    public nonisolated let queue: Queue
    public nonisolated let mediaPaths: MediaPaths
    /// The app's image pipeline: fetch, decode-at-size, bounded decoded cache.
    public nonisolated let imageStore: ImageStore
    public nonisolated let sync: Sync
    public nonisolated let downloads: Downloads
    /// Cast transport: browse for devices, and move playback to or from one.
    public nonisolated let cast: Cast

    /// Build the shared half of an `AppService` around an already-open,
    /// already-unlocked `AppHandle` and a config snapshot the caller read off
    /// it. Pure field assignment; every side effect lives in the subclass
    /// `wireUp()` after construction.
    public init(
        appHandle: AppHandle,
        mediaControlService: MediaControlService,
        diagnostics: BridgeDiagnostics,
        config: BridgeConfig,
        initialOutbox: BridgeOutboxSnapshot
    ) {
        self.appHandle = appHandle
        self.mediaControlService = mediaControlService
        self.diagnostics = diagnostics
        playbackStore = PlaybackStore()
        configStore = ConfigStore(
            config: Config(bridge: config),
            syncReady: appHandle.isSyncReady()
        )
        libraryStore = LibraryStore()
        // Both reads are infallible — `getDownloadSnapshot` never throws, and
        // the outbox snapshot is read (and its failure handled) by the caller.
        downloadStore = DownloadStore(snapshot: appHandle.getDownloadSnapshot())
        outboxStore = OutboxStore(snapshot: initialOutbox)
        castStore = CastStore()
        // Uniform across platforms: each `init(handle:)` wires exactly the
        // bridge methods its platform exports (the iOS flavors omit the
        // desktop-only reads).
        library = Library(handle: appHandle)
        playback = Playback(handle: appHandle)
        queue = Queue(handle: appHandle)
        mediaPaths = MediaPaths(handle: appHandle)
        imageStore = ImageStore(handle: appHandle)
        sync = Sync(handle: appHandle)
        downloads = Downloads(handle: appHandle)
        cast = Cast(handle: appHandle)
    }

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

    /// Route a display error to the platform's error surface. iOS uses the
    /// shared `ConfigStore` banner; macOS overrides this to route through its
    /// global alert (`UiStore`).
    open func showError(_ error: DisplayError) {
        #if os(iOS)
            configStore.showError(error)
        #else
            preconditionFailure(
                "macOS AppService must override showError to route through UiStore"
            )
        #endif
    }

    /// Mirror a queue snapshot into the playback store and refresh the
    /// media-control next/previous availability.
    public func applyQueueSnapshot(_ snapshot: BridgeQueueSnapshot) {
        playbackStore.applyQueueSnapshot(snapshot)
        mediaControlService.updateCommandAvailability(
            hasNext: snapshot.hasNext,
            hasPrevious: snapshot.hasPrevious
        )
    }

    /// Register the projections both platforms share. Called first by each
    /// subclass's `wireUp()`; platform-only projections are added after via
    /// `registerProjection`.
    public func registerCommonProjections() {
        precondition(projectionRegistrations.isEmpty)
        projectionRegistrations = [
            projectionRegistry.register(makeConfigProjection()),
            projectionRegistry.register(makeSyncStatusProjection()),
            projectionRegistry.register(makeOutboxProjection()),
            projectionRegistry.register(makeDownloadProjection()),
            projectionRegistry.register(makeReleaseDetailProjection()),
            projectionRegistry.register(makeCastDevicesProjection()),
            // The bridge's lag-recovery path is `.queue`'s only invalidation
            // producer (normal queue changes land through the `queueUpdated`
            // direct apply), so this projection exists purely for that recovery
            // case. A lag-recovery refetch queried before the next queue event
            // but applied after that event's direct apply can overwrite one
            // newer snapshot with an older one — corrected on the next queue
            // event. Both applies write idempotent full snapshots, and
            // `Projection`'s generation guard covers invalidation-vs-invalidation
            // races, not this invalidation-vs-direct-apply race.
            projectionRegistry.register(makeQueueProjection()),
        ]
    }

    /// Register a platform-only projection and retain its subscription for the
    /// life of the service.
    public func registerProjection<Value: Sendable>(
        _ projection: Projection<Value>
    ) {
        projectionRegistrations.append(projectionRegistry.register(projection))
    }
}

extension AppService {
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
            onError: { [weak self] error in self?.showError(error) }
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
            onError: { [weak self] error in self?.showError(error) }
        )
    }

    private func makeQueueProjection() -> Projection<BridgeQueueSnapshot> {
        Projection(
            domain: .queue,
            query: { [appHandle] _ in
                try await appHandle.getQueueSnapshot()
            },
            apply: { [weak self] snapshot in
                self?.applyQueueSnapshot(snapshot)
            },
            onError: { [weak self] error in self?.showError(error) }
        )
    }

    private func makeOutboxProjection() -> Projection<BridgeOutboxSnapshot> {
        Projection(
            domain: .outbox,
            query: { [appHandle] _ in
                try await appHandle.getOutboxSnapshot()
            },
            apply: { [outboxStore] snapshot in
                outboxStore.applySnapshot(snapshot)
            },
            onError: { [weak self] error in self?.showError(error) }
        )
    }

    private func makeDownloadProjection() -> Projection<BridgeDownloadSnapshot>
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
            onError: { [weak self] error in self?.showError(error) }
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
                let release = try await appHandle.findReleaseDetail(
                    releaseId: releaseId
                )
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
            onError: { [weak self] error in self?.showError(error) }
        )
    }

    /// The picker's device list. Only ever invalidated while a picker is open —
    /// discovery is what produces the invalidation, and it runs with the picker.
    private func makeCastDevicesProjection() -> Projection<[BridgeCastDevice]> {
        Projection(
            domain: .castDevices,
            query: { [appHandle] _ in
                try await DetachedWork.run { appHandle.getCastDevices() }
            },
            apply: { [castStore] devices in castStore.devices = devices },
            onError: { [weak self] error in self?.showError(error) }
        )
    }
}
