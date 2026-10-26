import Foundation

private final class ConfigValueSink: ConfigCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeConfig) -> Void

    init(apply: @escaping @MainActor @Sendable (BridgeConfig) -> Void) {
        self.apply = apply
    }

    func onValue(config: BridgeConfig) {
        Task { @MainActor in apply(config) }
    }
}
private final class SyncStatusValueSink: SyncStatusCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeSyncStatusSnapshot) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeSyncStatusSnapshot) -> Void
    ) {
        self.apply = apply
    }

    func onValue(value: BridgeSyncStatusSnapshot) {
        Task { @MainActor in apply(value) }
    }
}

private final class SyncActivityValueSink: SyncActivityCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeSyncActivity) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeSyncActivity) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onError(error: BridgeError) { Task { @MainActor in fail(error) } }

    func onValue(value: BridgeSyncActivity) {
        Task { @MainActor in apply(value) }
    }
}

private final class PlaybackValuesSink: PlaybackValuesCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgePlaybackValues) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgePlaybackValues) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgePlaybackValues) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

private final class PlaybackPositionsSink: PlaybackPositionsCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgePlaybackPositions) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgePlaybackPositions) -> Void
    ) {
        self.apply = apply
    }

    func onValue(value: BridgePlaybackPositions) {
        Task { @MainActor in apply(value) }
    }
}

private final class QueueValueSink: QueueCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeQueueSnapshot) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeQueueSnapshot) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeQueueSnapshot) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

private final class OutboxValueSink: OutboxCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeOutboxSnapshot) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgeOutboxSnapshot) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeOutboxSnapshot) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

private final class DownloadValueSink: DownloadCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeDownloadSnapshot) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeDownloadSnapshot) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(value: BridgeDownloadSnapshot) {
        Task { @MainActor in apply(value) }
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

private final class CastDevicesValueSink: CastDevicesCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable ([BridgeCastDevice]) -> Void

    init(
        apply: @escaping @MainActor @Sendable ([BridgeCastDevice]) -> Void
    ) {
        self.apply = apply
    }

    func onValue(devices: [BridgeCastDevice]) {
        Task { @MainActor in apply(devices) }
    }
}

@MainActor
final class CommonSubscriptions {
    private let appHandle: AppHandle
    private let configStore: ConfigStore
    private let syncStatusStore: SyncStatusStore
    private let syncActivityStore: SyncActivityStore
    private let outboxStore: OutboxStore
    private let downloadStore: DownloadStore
    private let castStore: CastStore
    private var subscriptions: [LiveSubscription] = []

    init(
        appHandle: AppHandle,
        configStore: ConfigStore,
        syncStatusStore: SyncStatusStore,
        syncActivityStore: SyncActivityStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        castStore: CastStore
    ) {
        self.appHandle = appHandle
        self.configStore = configStore
        self.syncStatusStore = syncStatusStore
        self.syncActivityStore = syncActivityStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.castStore = castStore
    }

    func start(
        applyPlayback:
            @escaping @MainActor @Sendable (BridgePlaybackValues) -> Void,
        applyPositions:
            @escaping @MainActor @Sendable (BridgePlaybackPositions) -> Void,
        applyQueue:
            @escaping @MainActor @Sendable (BridgeQueueSnapshot) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        precondition(subscriptions.isEmpty)
        subscriptions =
            libraryStatusSubscriptions(onError: onError) + [
                appHandle.subscribePlaybackValues(
                    callback: PlaybackValuesSink(
                        apply: applyPlayback,
                        fail: onError
                    )
                ),
                appHandle.subscribePlaybackPositions(
                    callback: PlaybackPositionsSink(apply: applyPositions)
                ),
                appHandle.subscribeQueue(
                    callback: QueueValueSink(
                        apply: applyQueue,
                        fail: onError
                    )
                ),
                appHandle.subscribeOutbox(
                    callback: OutboxValueSink(
                        apply: { [outboxStore] value in
                            outboxStore.applySnapshot(value)
                        },
                        fail: onError
                    )
                ),
                appHandle.subscribeDownloads(
                    callback: DownloadValueSink(
                        apply: { [downloadStore] value in
                            downloadStore.applySnapshot(value)
                        },
                        fail: onError
                    )
                ),
                appHandle.subscribeCastDevices(
                    callback: CastDevicesValueSink { [castStore] devices in
                        castStore.devices = devices
                    }
                ),
            ]
    }
    private func libraryStatusSubscriptions(
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> [LiveSubscription] {
        [
            appHandle.subscribeLibraryName(
                callback: LibraryNameSink(
                    apply: { [configStore] name in
                        configStore.applyLibraryName(name)
                    },
                    fail: onError
                )
            ),
            appHandle.subscribeConfig(
                callback: ConfigValueSink { [configStore] config in
                    configStore.applyConfigSnapshot(config)
                }
            ),
            appHandle.subscribeSyncStatus(
                callback: SyncStatusValueSink { [syncStatusStore] value in
                    syncStatusStore.apply(value)
                }
            ),
            appHandle.subscribeSyncActivity(
                callback: SyncActivityValueSink(
                    apply: { [syncActivityStore] value in
                        syncActivityStore.apply(value)
                    },
                    fail: { [syncActivityStore] error in
                        syncActivityStore.fail(error)
                    }
                )
            ),
        ]
    }

}

private final class LibraryNameSink: LibraryNameCallback, Sendable {
    private let apply: @MainActor @Sendable (String) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void
    init(
        apply: @escaping @MainActor @Sendable (String) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }
    func onValue(value: String) { Task { @MainActor in apply(value) } }
    func onError(error: BridgeError) { Task { @MainActor in fail(error) } }
}
