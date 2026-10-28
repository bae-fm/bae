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

private final class PlaybackTimelineSink: PlaybackTimelineCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgePlaybackTimeline) -> Void

    init(
        apply: @escaping @MainActor @Sendable (BridgePlaybackTimeline) -> Void
    ) {
        self.apply = apply
    }

    func onValue(timeline: BridgePlaybackTimeline) {
        Task { @MainActor in apply(timeline) }
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

private final class UploadsPausedSink: UploadsPausedCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (Bool) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply: @escaping @MainActor @Sendable (Bool) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onValue(paused: Bool) {
        Task { @MainActor in apply(paused) }
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
    private let playbackStore: PlaybackStore
    private let castStore: CastStore
    private var subscriptions: [LiveSubscription] = []
    /// The fast streams' feeds, which stop with these subscriptions.
    private var feeds: [LatestFeed] = []

    init(
        appHandle: AppHandle,
        configStore: ConfigStore,
        syncStatusStore: SyncStatusStore,
        syncActivityStore: SyncActivityStore,
        outboxStore: OutboxStore,
        downloadStore: DownloadStore,
        playbackStore: PlaybackStore,
        castStore: CastStore
    ) {
        self.appHandle = appHandle
        self.configStore = configStore
        self.syncStatusStore = syncStatusStore
        self.syncActivityStore = syncActivityStore
        self.outboxStore = outboxStore
        self.downloadStore = downloadStore
        self.playbackStore = playbackStore
        self.castStore = castStore
    }

    func start(
        applyPlayback:
            @escaping @MainActor @Sendable (BridgePlaybackValues) -> Void,
        applyTimeline:
            @escaping @MainActor @Sendable (BridgePlaybackTimeline) -> Void,
        applyQueue:
            @escaping @MainActor @Sendable (BridgeQueueSnapshot) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        precondition(subscriptions.isEmpty)
        feeds = [
            outboxStore.releaseProgress.feed(from: appHandle, onError: onError),
            outboxStore.filesBeingSent.feed(from: appHandle, onError: onError),
            downloadStore.releaseProgress.feed(from: appHandle),
            playbackStore.trackPositions.feed(from: appHandle),
        ]
        subscriptions =
            libraryStatusSubscriptions(onError: onError) + [
                appHandle.subscribePlaybackValues(
                    callback: PlaybackValuesSink(
                        apply: applyPlayback,
                        fail: onError
                    )
                ),
                appHandle.subscribePlaybackTimeline(
                    callback: PlaybackTimelineSink(apply: applyTimeline)
                ),
                appHandle.subscribeQueue(
                    callback: QueueValueSink(
                        apply: applyQueue,
                        fail: onError
                    )
                ),
                appHandle.subscribeUploadsPaused(
                    callback: UploadsPausedSink(
                        apply: { [outboxStore] paused in
                            outboxStore.applyUploadsPaused(paused)
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
