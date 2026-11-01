import Foundation

private final class SyncStatusValueSink: SyncStatusCallback,
    @unchecked Sendable
{
    private let apply: @MainActor @Sendable (BridgeSyncStatusSnapshot) -> Void
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(
        apply:
            @escaping @MainActor @Sendable (BridgeSyncStatusSnapshot) -> Void,
        fail: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        self.apply = apply
        self.fail = fail
    }

    func onError(error: BridgeError) { Task { @MainActor in fail(error) } }

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
    private var playbackValues: ScreenSubscription<PlaybackValuesSubscription>?
    private var playbackTimeline:
        ScreenSubscription<PlaybackTimelineSubscription>?
    private var queueAdditions: ScreenSubscription<QueueAdditionsSubscription>?
    private var castDevices: ScreenSubscription<CastDevicesSubscription>?
    private var config: ScreenSubscription<ConfigSubscription>?
    private var libraryName: ScreenSubscription<LibraryNameSubscription>?
    private var queue: ScreenSubscription<QueueSubscription>?
    private var uploadsPaused: ScreenSubscription<UploadsPausedSubscription>?
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
        applyQueueAddition:
            @escaping @MainActor @Sendable (BridgeQueueAddition) -> Void,
        applyQueue:
            @escaping @MainActor @Sendable (BridgeQueueSnapshot) -> Void,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) {
        precondition(subscriptions.isEmpty)
        playbackValues = ScreenSubscription(
            appHandle.subscribePlaybackValues(),
            onValue: applyPlayback,
            onError: onError
        )
        playbackTimeline = ScreenSubscription(
            appHandle.subscribePlaybackTimeline(),
            onValue: applyTimeline,
            onError: onError
        )
        queueAdditions = ScreenSubscription(
            appHandle.subscribeQueueAdditions(),
            onValue: applyQueueAddition,
            onError: onError
        )
        castDevices = ScreenSubscription(
            appHandle.subscribeCastDevices(),
            onValue: { [castStore] devices in castStore.devices = devices },
            onError: onError
        )
        config = ScreenSubscription(
            appHandle.subscribeConfig(),
            onValue: { [configStore] config in
                configStore.applyConfigSnapshot(config)
            },
            onError: onError
        )
        libraryName = ScreenSubscription(
            appHandle.subscribeLibraryName(),
            onValue: { [configStore] name in configStore.applyLibraryName(name)
            },
            onError: onError
        )
        queue = ScreenSubscription(
            appHandle.subscribeQueue(),
            onValue: applyQueue,
            onError: onError
        )
        uploadsPaused = ScreenSubscription(
            appHandle.subscribeUploadsPaused(),
            onValue: { [outboxStore] paused in
                outboxStore.applyUploadsPaused(paused)
            },
            onError: onError
        )
        feeds = [
            outboxStore.releaseProgress.feed(from: appHandle, onError: onError),
            outboxStore.filesBeingSent.feed(from: appHandle, onError: onError),
            downloadStore.releaseProgress.feed(from: appHandle),
            playbackStore.trackPositions.feed(from: appHandle),
        ]
        subscriptions = syncSubscriptions()
    }

    private func syncSubscriptions() -> [LiveSubscription] {
        [
            appHandle.subscribeSyncStatus(
                callback: SyncStatusValueSink(
                    apply: { [syncStatusStore] value in
                        syncStatusStore.apply(value)
                    },
                    fail: { [syncStatusStore] error in
                        syncStatusStore.fail(error)
                    }
                )
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
