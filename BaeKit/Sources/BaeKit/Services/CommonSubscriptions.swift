import Foundation

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
    private var playbackValues: ScreenSubscription<PlaybackValuesSubscription>?
    private var playbackTimeline:
        ScreenSubscription<PlaybackTimelineSubscription>?
    private var queueAdditions: ScreenSubscription<QueueAdditionsSubscription>?
    private var castDevices: ScreenSubscription<CastDevicesSubscription>?
    private var config: ScreenSubscription<ConfigSubscription>?
    private var libraryName: ScreenSubscription<LibraryNameSubscription>?
    private var queue: ScreenSubscription<QueueSubscription>?
    private var uploadsPaused: ScreenSubscription<UploadsPausedSubscription>?
    private var syncStatus: ScreenSubscription<SyncStatusSubscription>?
    private var syncActivity: ScreenSubscription<SyncActivitySubscription>?
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
        precondition(playbackValues == nil, "the subscriptions start once")
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
        startSync()
    }

    /// What sync is doing and what it waits on, for the status line and the
    /// activity indicator.
    private func startSync() {
        syncStatus = ScreenSubscription(
            appHandle.subscribeSyncStatus(),
            onValue: { [syncStatusStore] value in syncStatusStore.apply(value)
            },
            onError: { [syncStatusStore] error in syncStatusStore.fail(error) }
        )
        syncActivity = ScreenSubscription(
            appHandle.subscribeSyncActivity(),
            onValue: { [syncActivityStore] value in
                syncActivityStore.apply(value)
            },
            onError: { [syncActivityStore] error in
                syncActivityStore.fail(error)
            }
        )
    }
}
