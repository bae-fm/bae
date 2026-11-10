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
    private var castReceivers: ScreenSubscription<CastReceiversSubscription>?
    private var config: ScreenSubscription<ConfigSubscription>?
    private var queue: ScreenSubscription<QueueSubscription>?
    private var uploadsPaused: ScreenSubscription<UploadsPausedSubscription>?
    private var downloadCounts: ScreenSubscription<DownloadCountsSubscription>?
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
        castReceivers = ScreenSubscription(
            appHandle.subscribeCastReceivers(),
            onValue: { [castStore] receivers in
                castStore.receivers = receivers
            },
            onError: onError
        )
        config = ScreenSubscription(
            appHandle.subscribeConfig(),
            onValue: { [configStore] config in
                configStore.applyConfigSnapshot(config)
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
        downloadCounts = ScreenSubscription(
            appHandle.subscribeDownloadCounts(),
            onValue: { [downloadStore] counts in
                downloadStore.applyDownloadsPaused(counts.wait == .paused)
            },
            onError: onError
        )
        feeds =
            outboxStore.feedTransfers(from: appHandle, onError: onError) + [
                downloadStore.releaseProgress.feed(from: appHandle),
                playbackStore.trackPositions.feed(from: appHandle),
                playbackStore.trackBuffered.feed(from: appHandle),
                playbackStore.trackStorageWaits.feed(from: appHandle),
            ]
        startSync()
    }

    /// What sync is doing and what it waits on, for the status line and the
    /// activity indicator.
    private func startSync() {
        syncStatus = ScreenSubscription(
            appHandle.subscribeSyncStatus(),
            onValue: { [weak self] value in self?.applySyncStatus(value) },
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

    /// Apply a sync status. A storage setup records the library's location
    /// as it ends, and one can end with no setup screen to hear it: one sync
    /// finishes after a relaunch, or one whose screen closed past its point
    /// of no return. So the location is read again once a setup stops
    /// running, and at the first status of a library opened with none,
    /// whose setup may have ended between that read and this status.
    private func applySyncStatus(_ value: BridgeSyncStatusSnapshot) {
        let wasRunning =
            syncStatusStore.snapshot.map(\.setupRunning)
            ?? (configStore.storage == nil)
        let setupEnded = wasRunning && !value.setupRunning
        syncStatusStore.apply(value)
        guard setupEnded else { return }
        Task { [appHandle, configStore, syncStatusStore] in
            do {
                configStore.applyStorage(try await appHandle.storageLocation())
            }
            catch { syncStatusStore.fail(error) }
        }
    }
}
