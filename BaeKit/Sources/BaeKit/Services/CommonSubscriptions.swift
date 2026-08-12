import Foundation

private final class ConfigValueSink: ConfigCallback, @unchecked Sendable {
    private let apply: @MainActor @Sendable (BridgeConfig, Bool) -> Void

    init(apply: @escaping @MainActor @Sendable (BridgeConfig, Bool) -> Void) {
        self.apply = apply
    }

    func onValue(config: BridgeConfig, syncReady: Bool) {
        Task { @MainActor in apply(config, syncReady) }
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
    private let outboxStore: OutboxStore
    private let downloadStore: DownloadStore
    private let castStore: CastStore
    private var subscriptions: [LiveSubscription] = []

    init(
        appHandle: AppHandle,
        configStore: ConfigStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        castStore: CastStore
    ) {
        self.appHandle = appHandle
        self.configStore = configStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.castStore = castStore
    }

    func start(
        applyQueue:
            @escaping @MainActor @Sendable (BridgeQueueSnapshot) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        precondition(subscriptions.isEmpty)
        subscriptions = [
            appHandle.subscribeConfig(
                callback: ConfigValueSink { [configStore] config, syncReady in
                    configStore.applyConfigSnapshot(
                        config,
                        syncReady: syncReady
                    )
                }
            ),
            appHandle.subscribeSyncStatus(
                callback: SyncStatusValueSink { [configStore] value in
                    configStore.applySyncStatusSnapshot(value)
                }
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
