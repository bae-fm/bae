import Foundation

@MainActor
final class CommonProjections {
    private struct ConfigValue: Sendable {
        let config: BridgeConfig
        let syncReady: Bool
    }

    private struct ReleaseDetailValue: Sendable {
        let releaseId: String
        let release: BridgeRelease?
    }

    private let registry: ProjectionRegistry
    private let appHandle: AppHandle
    private let configStore: ConfigStore
    private let outboxStore: OutboxStore
    private let downloadStore: DownloadStore
    private let libraryStore: LibraryStore
    private let castStore: CastStore
    private var registrations: [ProjectionRegistration] = []

    init(
        registry: ProjectionRegistry,
        appHandle: AppHandle,
        configStore: ConfigStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        libraryStore: LibraryStore,
        castStore: CastStore
    ) {
        self.registry = registry
        self.appHandle = appHandle
        self.configStore = configStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.libraryStore = libraryStore
        self.castStore = castStore
    }

    func start(
        applyQueue: @escaping @MainActor (BridgeQueueSnapshot) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) {
        precondition(registrations.isEmpty)
        registrations = [
            registry.register(makeConfigProjection(onError: onError)),
            registry.register(makeSyncStatusProjection(onError: onError)),
            registry.register(makeOutboxProjection(onError: onError)),
            registry.register(makeDownloadProjection(onError: onError)),
            registry.register(makeReleaseDetailProjection(onError: onError)),
            registry.register(makeCastDevicesProjection(onError: onError)),
            // The bridge's lag-recovery path is `.queue`'s only invalidation
            // producer. A recovery read can overlap a direct queue event, so
            // both paths apply revisioned snapshots and PlaybackStore rejects
            // an older recovery result.
            registry.register(
                makeQueueProjection(
                    applyQueue: applyQueue,
                    onError: onError
                )
            ),
        ]
    }

    private func makeConfigProjection(
        onError: @escaping @MainActor (any Error) -> Void
    ) -> Projection<ConfigValue> {
        Projection(
            domain: .config,
            query: { [appHandle] _ in
                try await DetachedWork.run {
                    ConfigValue(
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
            onError: onError
        )
    }

    private func makeSyncStatusProjection(
        onError: @escaping @MainActor (any Error) -> Void
    ) -> Projection<BridgeSyncStatusSnapshot> {
        Projection(
            domain: .syncStatus,
            query: { [appHandle] _ in
                try await DetachedWork.run { appHandle.getSyncStatus() }
            },
            apply: { [configStore] snapshot in
                configStore.applySyncStatusSnapshot(snapshot)
            },
            onError: onError
        )
    }

    private func makeQueueProjection(
        applyQueue: @escaping @MainActor (BridgeQueueSnapshot) -> Void,
        onError: @escaping @MainActor (any Error) -> Void
    ) -> Projection<BridgeQueueSnapshot> {
        Projection(
            domain: .queue,
            query: { [appHandle] _ in
                try await appHandle.getQueueSnapshot()
            },
            apply: applyQueue,
            onError: onError
        )
    }

    private func makeOutboxProjection(
        onError: @escaping @MainActor (any Error) -> Void
    ) -> Projection<BridgeOutboxSnapshot> {
        Projection(
            domain: .outbox,
            query: { [appHandle] _ in
                try await appHandle.getOutboxSnapshot()
            },
            apply: { [outboxStore] snapshot in
                outboxStore.applySnapshot(snapshot)
            },
            onError: onError
        )
    }

    private func makeDownloadProjection(
        onError: @escaping @MainActor (any Error) -> Void
    ) -> Projection<BridgeDownloadSnapshot> {
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
            onError: onError
        )
    }

    private func makeReleaseDetailProjection(
        onError: @escaping @MainActor (any Error) -> Void
    ) -> Projection<ReleaseDetailValue> {
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
                return ReleaseDetailValue(
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
            onError: onError
        )
    }

    private func makeCastDevicesProjection(
        onError: @escaping @MainActor (any Error) -> Void
    ) -> Projection<[BridgeCastDevice]> {
        Projection(
            domain: .castDevices,
            query: { [appHandle] _ in
                try await DetachedWork.run { appHandle.getCastDevices() }
            },
            apply: { [castStore] devices in castStore.devices = devices },
            onError: onError
        )
    }
}
