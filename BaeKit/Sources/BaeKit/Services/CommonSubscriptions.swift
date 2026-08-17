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

private final class ArtworkLoadingValueSink: EagerCacheFillStatusCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeEagerCacheFillStatus) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeEagerCacheFillStatus) -> Void
    ) {
        self.apply = apply
    }

    func onValue(value: BridgeEagerCacheFillStatus) {
        Task { @MainActor in apply(value) }
    }
}

private final class PlaybackValuesSink: PlaybackValuesCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgePlaybackValues) -> Void

    init(apply: @escaping @MainActor @Sendable (BridgePlaybackValues) -> Void) {
        self.apply = apply
    }

    func onValue(value: BridgePlaybackValues) {
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

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeDownloadSnapshot) -> Void
    ) {
        self.apply = apply
    }

    func onValue(value: BridgeDownloadSnapshot) {
        Task { @MainActor in apply(value) }
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
    private let artworkLoadingStore: ArtworkLoadingStore
    private let outboxStore: OutboxStore
    private let downloadStore: DownloadStore
    private let castStore: CastStore
    private var subscriptions: [LiveSubscription] = []

    init(
        appHandle: AppHandle,
        configStore: ConfigStore,
        syncStatusStore: SyncStatusStore,
        artworkLoadingStore: ArtworkLoadingStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        castStore: CastStore
    ) {
        self.appHandle = appHandle
        self.configStore = configStore
        self.syncStatusStore = syncStatusStore
        self.artworkLoadingStore = artworkLoadingStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.castStore = castStore
    }

    func start(
        applyPlayback:
            @escaping @MainActor @Sendable (BridgePlaybackValues) -> Void,
        applyQueue:
            @escaping @MainActor @Sendable (BridgeQueueSnapshot) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        precondition(subscriptions.isEmpty)
        subscriptions = [
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
            appHandle.subscribeEagerCacheFillStatus(
                callback: ArtworkLoadingValueSink {
                    [artworkLoadingStore] value in
                    artworkLoadingStore.apply(value)
                }
            ),
            appHandle.subscribePlaybackValues(
                callback: PlaybackValuesSink(apply: applyPlayback)
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
                callback: DownloadValueSink { [downloadStore] value in
                    downloadStore.applySnapshot(value)
                }
            ),
            appHandle.subscribeCastDevices(
                callback: CastDevicesValueSink { [castStore] devices in
                    castStore.devices = devices
                }
            ),
        ]
    }
}
