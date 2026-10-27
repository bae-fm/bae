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

private let playingState = BridgePlaybackValueState.playing(
    track: BridgeNowPlayingTrack(
        track: BridgePlayingTrack(trackId: "track-1", durationMs: 120_000),
        display: BridgeTrackDisplay(
            title: "Track Title",
            artistNames: "Artist Name",
            albumId: "album-1",
            releaseId: "release-1",
            albumTitle: "Album Title",
            coverImage: nil
        )
    )
)

@MainActor
@Suite("AppService media control", .serialized)
struct AppServiceMediaControlTests {
    @Test("idle import preview does not clear library Now Playing")
    func idlePreviewKeepsLibraryNowPlaying() async throws {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        appService.startCommonSubscriptions()

        let state = playingState
        let position = BridgePlaybackPosition(
            trackId: "track-1",
            positionMs: 30_000,
            durationMs: 120_000,
            progress: 0.25
        )
        let mediaPosition = BridgeMediaControlPosition(
            trackId: "track-1",
            positionMs: 30_000,
            durationMs: 120_000,
            progress: 0.25
        )
        handle.deliverPlayback(playbackValues(state: state))
        handle.deliverPositions(
            BridgePlaybackPositions(
                library: position,
                seekRevision: 0,
                preview: nil,
                mediaControl: mediaPosition
            )
        )

        try await Wait.until {
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == "Track Title"
        }
    }

    @Test(
        "a position that comes before its track's value shows once that track does"
    )
    func positionBeforeItsTrackShowsWithTheTrack() async throws {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        appService.startCommonSubscriptions()

        handle.deliverPlayback(
            playbackValues(state: playing(trackId: "track-1"))
        )
        try await Wait.until {
            appService.stateForTesting.nowPlaying.track?.track.trackId
                == "track-1"
        }
        handle.deliverPositions(
            BridgePlaybackPositions(
                library: BridgePlaybackPosition(
                    trackId: "track-2",
                    positionMs: 30_000,
                    durationMs: 120_000,
                    progress: 0.25
                ),
                seekRevision: 0,
                preview: nil,
                mediaControl: BridgeMediaControlPosition(
                    trackId: "track-2",
                    positionMs: 30_000,
                    durationMs: 120_000,
                    progress: 0.25
                )
            )
        )
        handle.deliverPlayback(
            playbackValues(state: playing(trackId: "track-2"))
        )

        try await Wait.until {
            guard
                case .position(_, let positionMs, _) = appService
                    .stateForTesting.playbackPosition
            else {
                return false
            }
            return positionMs == 30_000
        }
    }
}

private func playing(trackId: String) -> BridgePlaybackValueState {
    .playing(
        track: BridgeNowPlayingTrack(
            track: BridgePlayingTrack(trackId: trackId, durationMs: 120_000),
            display: BridgeTrackDisplay(
                title: "Track Title",
                artistNames: "Artist Name",
                albumId: "album-1",
                releaseId: "release-1",
                albumTitle: "Album Title",
                coverImage: nil
            )
        )
    )
}

private func playbackValues(
    state: BridgePlaybackValueState
) -> BridgePlaybackValues {
    BridgePlaybackValues(
        state: state,
        volume: 1,
        isMuted: false,
        repeatMode: .off,
        remote: .local,
        preview: .idle,
        mediaControl: .library
    )
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
    .importIdentificationProgress(identified: 0, total: 1),
    .importsInFlight(count: 1),
]

private let handledEvents: [BridgeUiEvent] = [
    .playbackError(reason: .syncDisconnected),
    .queueItemsAdded(count: 1),
]

// MARK: - Test doubles

@MainActor
private func makeAppService(handle: FakeAppHandle = FakeAppHandle())
    -> bae.AppService
{
    bae.AppService(
        appHandle: handle,
        mediaControlService: MediaControlService(),
        diagnostics: configureDiagnostics(
            config: .disabled,
            bugReporter: CoreBugReporter()
        ),
        uiStore: UiStore(),
        config: BridgeConfig(
            libraryId: "lib-test",
            libraryPath: "/tmp/test",
            pauseBetweenSides: false,
            sidePauseCountdown: .off,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            identifyAutomatically: true,
            importWhenIdentified: false,
            importStorage: BridgeImportStorage(
                cloud: true,
                pinned: true
            ),
            lookupCatalogs: [
                BridgeLookupCatalogSetting(
                    catalog: .musicBrainz,
                    availability: .on,
                    canChange: true
                ),
                BridgeLookupCatalogSetting(
                    catalog: .discogs,
                    availability: .on,
                    canChange: true
                ),
            ],
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            castEnabled: false,
            castTranscodeFormat: .mp3,
            mcp: BridgeMcpConfig(enabled: false, port: 47777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4533,
                username: "",
                bindAddress: "127.0.0.1"
            ),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false
        ),
        seed: emptySeed,
        deviceName: "Test Mac"
    )
}

/// An empty upload queue, as a freshly opened library's core reports it.
@MainActor
private var emptySeed: LibrarySessionSeed {
    LibrarySessionSeed(
        name: "Test Library",
        storage: nil,
        uploadsPaused: false
    )
}

/// A handle-less `AppHandle` that answers the reads the macOS `AppService`
/// constructor and its subscriptions make so a dispatcher test can build the
/// real service without a live core.
private final class FakeAppHandle: AppHandle, @unchecked Sendable {
    private var queueCallback: (any QueueCallback)?
    private var playbackCallback: (any PlaybackValuesCallback)?
    private var positionsCallback: (any PlaybackPositionsCallback)?

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func isSyncReady() -> Bool {
        false
    }

    // The service reports the network path as soon as it starts; nothing
    // here syncs.
    override func reportNetworkChange() {}

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

    override func subscribeDownloadTransfers(
        callback _: any DownloadTransfersCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeSyncActivity(
        callback _: any SyncActivityCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribePlaybackValues(
        callback: any PlaybackValuesCallback
    ) -> LiveSubscription {
        playbackCallback = callback
        return NoopLiveSubscription()
    }

    override func subscribePlaybackPositions(
        callback: any PlaybackPositionsCallback
    ) -> LiveSubscription {
        positionsCallback = callback
        return NoopLiveSubscription()
    }

    override func subscribeUploadsPaused(
        callback _: any UploadsPausedCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeOutboxTransfers(
        callback _: any OutboxTransfersCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeCastDevices(
        callback _: any CastDevicesCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeLibraryName(
        callback _: any LibraryNameCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeMembers(
        callback _: any MembershipCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeProfiles(
        callback _: any ProfilesCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
    }

    override func subscribeInvites(
        callback _: any InvitesCallback
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

    func deliverPositions(_ value: BridgePlaybackPositions) {
        positionsCallback?.onValue(value: value)
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
