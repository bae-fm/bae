import BaeKit
import Combine
import MediaPlayer
import Testing

@testable import bae

/// Drives the real macOS sink (`UiEventDispatcher.makeSink`) with a faked
/// `AppHandle` (no live core) against every shared arm the dispatcher handles,
/// plus the `.handled`/`.unhandled` split that decides whether an event falls
/// to the platform's own tail. These read and write process-global singletons
/// (Now Playing info + remote command center), so every suite here is
/// serialized and resets state on exit.
@MainActor
@Suite("UiEventDispatcher error seams", .serialized)
struct UiEventDispatcherErrorTests {
    @Test("playbackError reaches uiStore through the showError override")
    func playbackErrorReachesUiStore() {
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )

        sink(.playbackError(reason: .syncDisconnected))

        #expect(appService.hasDisplayedErrorForTesting)
    }

    @Test("error reaches uiStore through the showError override")
    func errorReachesUiStore() {
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )

        sink(
            .error(
                error: .Diagnostic(category: .internal, detail: "boom")
            )
        )

        #expect(appService.hasDisplayedErrorForTesting)
    }
}

@MainActor
@Suite("UiEventDispatcher control arms", .serialized)
struct UiEventDispatcherControlTests {
    @Test("queueItemsAdded reaches the transient playback signal")
    func queueItemsAddedReachesPlaybackSignal() {
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )
        var addedCounts: [Int] = []
        let subscription = appService.queueItemsAddedPublisherForTesting
            .sink { addedCounts.append($0) }
        defer { subscription.cancel() }

        sink(.queueItemsAdded(count: 3))

        #expect(addedCounts == [3])
    }
}

@MainActor
@Suite("AppService media control", .serialized)
struct AppServiceMediaControlTests {
    @Test("idle import preview does not clear library Now Playing")
    func idlePreviewKeepsLibraryNowPlaying() async {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        appService.startCommonSubscriptions()

        let state = BridgePlaybackValueState.playing(
            trackId: "track-1",
            trackTitle: "Track Title",
            artistNames: "Artist Name",
            artistId: "artist-1",
            albumId: "album-1",
            albumTitle: "Album Title",
            coverImage: nil,
            durationMs: 120_000
        )
        let position = BridgePlaybackPosition(
            trackId: "track-1",
            positionMs: 30_000,
            durationMs: 120_000,
            progress: 0.25
        )
        handle.deliverPlayback(
            BridgePlaybackValues(
                state: state,
                position: position,
                seekRevision: 0,
                volume: 1,
                isMuted: false,
                repeatMode: .off,
                remoteDeviceName: nil,
                preview: BridgePreviewValues(
                    state: .idle,
                    positionMs: 0,
                    progress: 0
                ),
                mediaControl: BridgeMediaControlValues(
                    playback: .library(
                        state: state,
                        position: position,
                        seekRevision: 0
                    ),
                    volume: 1,
                    isMuted: false
                )
            )
        )

        await waitUntil {
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == "Track Title"
        }
    }
}

@MainActor
@Suite("UiEventDispatcher outcome policy", .serialized)
struct UiEventDispatcherOutcomeTests {
    @Test("the desktop import events are unhandled")
    func desktopImportEventsAreUnhandled() {
        let appService = makeAppService()
        for event in unhandledEvents {
            #expect(
                UiEventDispatcher.dispatch(event, appService: appService)
                    == .unhandled
            )
        }
    }

    @Test("every other event is handled")
    func everyOtherEventIsHandled() {
        let appService = makeAppService()
        for event in handledEvents {
            #expect(
                UiEventDispatcher.dispatch(event, appService: appService)
                    == .handled
            )
        }
    }
}

// MARK: - Shared fixtures

private let unhandledEvents: [BridgeUiEvent] = [
    .importQueueIdentifyProgress(identified: 0, total: 1)
]

private let handledEvents: [BridgeUiEvent] = [
    .playbackError(reason: .syncDisconnected),
    .queueItemsAdded(count: 1),
    .error(error: .Diagnostic(category: .internal, detail: "boom")),
]

// MARK: - Test doubles

@MainActor
private func waitUntil(_ predicate: @MainActor () -> Bool) async {
    for _ in 0..<100 {
        if predicate() {
            return
        }
        await Task.yield()
    }
    #expect(predicate())
}

@MainActor
private func makeAppService(handle: FakeAppHandle = FakeAppHandle())
    -> bae.AppService
{
    bae.AppService(
        appHandle: handle,
        mediaControlService: MediaControlService(),
        diagnostics: configureDiagnostics(config: .disabled),
        uiStore: UiStore(),
        config: BridgeConfig(
            libraryId: "lib-test",
            libraryName: "Test Library",
            libraryPath: "/tmp/test",
            pauseBetweenSides: false,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            identifyAutomatically: true,
            defaultImportMetadataSource: .findOnline,
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            castEnabled: false,
            mcp: BridgeMcpConfig(enabled: false, port: 47777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4533,
                username: "",
                bindAddress: "127.0.0.1"
            ),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false,
            sync: nil
        ),
        initialOutbox: OutboxStore.emptySnapshot
    )
}

/// A handle-less `AppHandle` that answers the reads the macOS `AppService`
/// constructor makes so a dispatcher test can build the real service without a
/// live core.
private final class FakeAppHandle: AppHandle, @unchecked Sendable {
    private var queueCallback: (any QueueCallback)?
    private var playbackCallback: (any PlaybackValuesCallback)?

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func isSyncReady() -> Bool {
        false
    }

    override func getDownloadSnapshot() -> BridgeDownloadSnapshot {
        BridgeDownloadSnapshot(
            downloads: [],
            total: BridgeDownloadProgress(queued: 0, active: 0, failed: 0),
            summaryParts: [],
            paused: false
        )
    }

    override func getOutputSnapshot() -> BridgeOutputSnapshot {
        BridgeOutputSnapshot(
            outputs: [],
            total: BridgeOutputProgress(queued: 0, active: 0, failed: 0),
            summaryParts: [],
            paused: false
        )
    }

    override func subscribeConfig(
        callback _: any ConfigCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeSyncStatus(
        callback _: any SyncStatusCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeDownloads(
        callback _: any DownloadCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeEagerCacheFillStatus(
        callback _: any EagerCacheFillStatusCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribePlaybackValues(
        callback: any PlaybackValuesCallback
    ) -> LiveSubscription {
        playbackCallback = callback
        return NoopLiveSubscription()
    }

    override func subscribeOutbox(
        callback _: any OutboxCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeCastDevices(
        callback _: any CastDevicesCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeQueue(
        callback: any QueueCallback
    ) -> LiveSubscription {
        queueCallback = callback
        return NoopLiveSubscription()
    }

    func deliverQueue(_ value: BridgeQueueSnapshot) {
        queueCallback?.onValue(value: value)
    }

    func deliverPlayback(_ value: BridgePlaybackValues) {
        playbackCallback?.onValue(value: value)
    }

}

private final class NoopLiveSubscription: LiveSubscription,
    @unchecked Sendable
{
    init() {
        super.init(noHandle: LiveSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func cancel() {}
}
