import BaeKit
import Combine
import MediaPlayer
import Testing

@testable import bae

/// Drives the real macOS `AppService` with a faked `AppHandle` (no live
/// core) through the subscriptions it starts. These read and write
/// process-global singletons (Now Playing info + remote command center), so
/// every suite here is serialized and resets state on exit.
@MainActor
@Suite("Playback failure display", .serialized)
struct PlaybackFailureDisplayTests {
    @Test("a playback failure reaches uiStore once, however often it is read")
    func playbackFailureReachesUiStoreOnce() async throws {
        let handle = FakeAppHandle()
        let uiStore = UiStore()
        let appService = makeAppService(handle: handle, uiStore: uiStore)
        appService.startCommonSubscriptions()
        let failed = BridgePlaybackFailure(number: 1, reason: .syncDisconnected)

        handle.deliverPlayback(playbackValues(state: .stopped, failure: failed))
        try await Wait.until { appService.hasDisplayedErrorForTesting }
        uiStore.lastError = nil

        // The same failure read again with a volume change shows nothing;
        // the next failure, alike in all but its number, shows.
        var louder = playbackValues(state: .stopped, failure: failed)
        louder.volume = 0.5
        handle.deliverPlayback(louder)
        try await Wait.until {
            appService.stateForTesting.settings?.volume == 0.5
        }
        #expect(!appService.hasDisplayedErrorForTesting)

        handle.deliverPlayback(
            playbackValues(
                state: .stopped,
                failure: BridgePlaybackFailure(
                    number: 2,
                    reason: .syncDisconnected
                )
            )
        )
        try await Wait.until { appService.hasDisplayedErrorForTesting }
    }
}

@MainActor
@Suite("Queue additions", .serialized)
struct QueueAdditionTests {
    @Test("each pulled queue addition reaches the badge with its track count")
    func queueAdditionReachesBadge() async throws {
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        var addedCounts: [Int] = []
        let subscription = appService.queueItemsAddedPublisherForTesting
            .sink {
                addedCounts.append($0)
                Wait.happened()
            }
        defer { subscription.cancel() }
        appService.startCommonSubscriptions()

        // Two additions alike in their count are two badges.
        handle.deliverQueueAddition(BridgeQueueAddition(number: 1, tracks: 3))
        handle.deliverQueueAddition(BridgeQueueAddition(number: 2, tracks: 3))

        try await Wait.until { addedCounts == [3, 3] }
    }
}

@MainActor
@Suite("Sync status", .serialized)
struct SyncStatusSubscriptionTests {
    @Test(
        "each pulled sync status reaches its store, and a failed read shows as unreadable until the next value"
    )
    func pulledSyncStatusReachesTheStore() async throws {
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        let store = appService.syncStatusStoreForTesting
        appService.startCommonSubscriptions()
        #expect(store.shown == nil && !store.unreadable)

        let synced = BridgeSyncStatusSnapshot(
            shown: .fixtureSynced,
            pending: [],
            setupRunning: false,
            undoneSetup: nil
        )
        handle.deliverSyncStatus(synced)
        try await Wait.until { store.shown?.status == .synced }

        handle.failSyncStatusRead(
            .Diagnostic(category: .database, detail: "the read failed")
        )
        try await Wait.until { store.unreadable }

        handle.deliverSyncStatus(synced)
        try await Wait.until { store.shown?.status == .synced }
    }
}

@MainActor
@Suite("Storage setup", .serialized)
struct StorageSetupSubscriptionTests {
    @Test(
        "a storage setup that stops running with no screen to hear it shows the location it recorded"
    )
    func aSetupThatStopsRunningShowsItsLocation() async throws {
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        appService.startCommonSubscriptions()
        let location = BridgeStorageLocation.s3(
            bucket: "music-library",
            region: "us-east-1",
            endpoint: nil,
            profile: nil
        )
        let status = { (running: Bool) in
            BridgeSyncStatusSnapshot(
                shown: running
                    ? BridgeSyncShown(
                        status: .settingUpStorage,
                        statusKey: "core.sync.setting_up_storage",
                        tone: .neutral,
                        cause: nil,
                        fix: nil,
                        lastSynced: nil
                    ) : .fixtureSynced,
                pending: [],
                setupRunning: running,
                undoneSetup: nil
            )
        }

        handle.deliverSyncStatus(status(true))
        try await Wait.until {
            appService.syncStatusStoreForTesting.shown?.status
                == .settingUpStorage
        }
        handle.location = location
        handle.deliverSyncStatus(status(false))

        try await Wait.until { appService.storageForTesting == location }
    }
}

@MainActor
@Suite("Sync activity", .serialized)
struct SyncActivitySubscriptionTests {
    @Test("each pulled sync activity reaches its store")
    func pulledSyncActivityReachesTheStore() async throws {
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        let store = appService.syncActivityStoreForTesting
        appService.startCommonSubscriptions()
        #expect(store.activity == nil)

        handle.deliverSyncActivity(activity(sending: 2))
        try await Wait.until { store.activity?.moving == true }

        handle.deliverSyncActivity(activity(sending: nil))
        try await Wait.until { store.activity?.moving == false }
    }

    private func activity(sending: UInt32?) -> BridgeSyncActivity {
        BridgeSyncActivity(
            moving: sending != nil,
            held: false,
            needsAttention: false,
            failure: nil,
            attention: [],
            syncing: false,
            uploads: sending.map {
                BridgeUploadQueueActivity(
                    batch: BridgeTransferBatch(done: 0, total: $0),
                    wait: nil,
                    paused: false
                )
            },
            pins: nil,
            movingToThisDevice: nil
        )
    }
}

@MainActor
@Suite("Open invites", .serialized)
struct InvitesSubscriptionTests {
    @Test("each pulled list of open invites reaches the membership store")
    func pulledInvitesReachTheStore() async throws {
        let handle = FakeAppHandle()
        let store = MembershipStore(handle: handle)
        store.start()
        store.show()
        #expect(store.invites.isEmpty)

        handle.deliverInvites([invite(id: "11")])
        try await Wait.until { store.invites.map(\.id) == ["11"] }

        handle.deliverInvites([])
        try await Wait.until { store.invites.isEmpty }
    }

    @Test(
        "an invite a sharing refusal ended reaches the store beside the open ones"
    )
    func refusedInvitesReachTheStore() async throws {
        let handle = FakeAppHandle()
        let store = MembershipStore(handle: handle)
        store.start()
        store.show()

        let refused = BridgeRefusedInvite(
            operation: 7,
            address: "dan@example.com",
            refusal: .invalidAddress,
            reasonKey: "core.invite.refused.invalid_address"
        )
        handle.deliverInvites([invite(id: "11")], refused: [refused])
        try await Wait.until { store.inviteEntries.count == 2 }

        #expect(store.invites.map(\.id) == ["11"])
        #expect(store.inviteEntries.contains { $0.refusedInvite == refused })
    }

    @Test(
        "the people and invites are read while a screen shows them; the admin read runs throughout"
    )
    func shownReadsFollowTheScreen() async throws {
        let handle = FakeAppHandle()
        let store = MembershipStore(handle: handle)

        store.start()
        #expect(handle.subscribed == ["members"], "the menu bar's admin read")

        store.show()
        #expect(
            Set(handle.subscribed.dropFirst())
                == ["profiles", "requestedProfile", "adminChanges", "invites"]
        )
        handle.deliverInvites([invite(id: "11")])
        try await Wait.until { store.invites.map(\.id) == ["11"] }

        store.hide()
        try await Wait.until { handle.invitesCancels == 1 }
        #expect(store.invites.isEmpty, "what the screen read goes with it")

        store.show()
        #expect(handle.subscribed.filter { $0 == "invites" }.count == 2)
        #expect(handle.subscribed.filter { $0 == "members" }.count == 1)
    }

    private func invite(id: String) -> BridgeInvite {
        BridgeInvite(
            id: id,
            member: "member-1",
            label: "dan@example.com",
            access: .providerAccount(email: "dan@example.com"),
            shown: .code(code: "invite-code", progress: .waiting),
            held: nil
        )
    }
}

@MainActor
@Suite("Library name", .serialized)
struct LibraryNameStoreTests {
    @Test("the name is read while a screen follows it, and goes with it")
    func nameIsReadWhileFollowed() async throws {
        let handle = FakeAppHandle()
        let store = LibraryNameStore(handle: handle, showError: { _ in })
        #expect(handle.subscribed.isEmpty, "nothing reads the name unasked")

        let following = Task { await store.follow() }
        try await Wait.until { handle.subscribed == ["libraryName"] }
        following.cancel()
        await following.value

        #expect(store.name == nil)
        #expect(handle.subscribed == ["libraryName"])
    }

    @Test("opening the common subscriptions reads no library name")
    func commonSubscriptionsReadNoName() {
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        appService.startCommonSubscriptions()

        #expect(!handle.subscribed.contains("libraryName"))
        #expect(handle.subscribed == ["members"])
    }
}

private let playingState = BridgePlaybackValueState.playing(
    track: BridgeNowPlayingTrack(
        track: BridgePlayingTrack(trackId: "track-1", durationMs: 120_000),
        display: BridgeTrackDisplay(
            title: "Track Title",
            artistLine: .named("Artist Name"),
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
    /// The system's Now Playing info was written.
    private static let written = [
        Wait.Notice(MediaControlService.didWriteNowPlaying, of: nil)
    ]

    @Test("idle import preview does not clear library Now Playing")
    func idlePreviewKeepsLibraryNowPlaying() async throws {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let handle = FakeAppHandle()
        let appService = makeAppService(handle: handle)
        appService.startCommonSubscriptions()

        let state = playingState
        handle.deliverPlayback(playbackValues(state: state))
        handle.deliverTimeline(line(trackId: "track-1", positionMs: 30_000))

        try await Wait.until(notices: Self.written) {
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
                == "Track Title"
        }
    }

    @Test(
        "a line that comes before its track's value shows once that track does"
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
        handle.deliverTimeline(line(trackId: "track-2", positionMs: 30_000))
        handle.deliverPlayback(
            playbackValues(state: playing(trackId: "track-2"))
        )

        try await Wait.until(notices: Self.written) {
            infoCenter.nowPlayingInfo?[
                MPNowPlayingInfoPropertyElapsedPlaybackTime
            ] as? Double == 30.0
        }
    }
}

/// A library line core's timeline tells.
private func line(trackId: String, positionMs: UInt64)
    -> BridgePlaybackTimeline
{
    BridgePlaybackTimeline(
        library: BridgeTimelinePoint(
            trackId: trackId,
            positionMs: positionMs,
            durationMs: 120_000,
            moving: true
        ),
        seekRevision: 0,
        preview: nil
    )
}

private func playing(trackId: String) -> BridgePlaybackValueState {
    .playing(
        track: BridgeNowPlayingTrack(
            track: BridgePlayingTrack(trackId: trackId, durationMs: 120_000),
            display: BridgeTrackDisplay(
                title: "Track Title",
                artistLine: .named("Artist Name"),
                albumId: "album-1",
                releaseId: "release-1",
                albumTitle: "Album Title",
                coverImage: nil
            )
        )
    )
}

private func playbackValues(
    state: BridgePlaybackValueState,
    failure: BridgePlaybackFailure? = nil
) -> BridgePlaybackValues {
    BridgePlaybackValues(
        state: state,
        volume: 1,
        isMuted: false,
        repeatMode: .off,
        remote: .local,
        preview: .idle,
        mediaControl: .library,
        failure: failure
    )
}

// MARK: - Test doubles

/// The instant the tests' Now Playing measures elapsed time against, which
/// stands still, so an elapsed time reads what was written.
private let fixedInstant = ContinuousClock.now

@MainActor
private func makeAppService(
    handle: FakeAppHandle = FakeAppHandle(),
    uiStore: UiStore = UiStore()
) -> bae.AppService {
    bae.AppService(
        appHandle: handle,
        mediaControlService: MediaControlService(now: { fixedInstant }),
        diagnostics: configureDiagnostics(
            config: .disabled,
            bugReporter: CoreBugReporter()
        ),
        uiStore: uiStore,
        config: testBridgeConfig(),
        seed: emptySeed,
        host: TestHost.make()
    )
}

/// The configuration of a freshly opened test library.
private func testBridgeConfig() -> BridgeConfig {
    BridgeConfig(
        libraryId: "lib-test",
        libraryPath: "/tmp/test",
        pauseBetweenSides: false,
        sidePauseCountdown: .off,
        maxConcurrentUploads: 3,
        maxConcurrentDownloads: 3,
        maxConcurrentTransfers: 8,
        identifyAutomatically: true,
        importWhenIdentified: false,
        importDestination: .cloud,
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
        hasSourceToSearch: true,
        showRemainingTime: false,
        libraryFullWidth: false,
        askBeforeUnpinning: true,
        savePresets: [],
        filenamePieces: [],
        filenamePatternError: .empty,
        folderPieces: [
            .value(value: .artist), .text(text: " - "),
            .value(value: .album),
        ],
        folderPatternError: nil,
        saveExtras: PreviewData.saveExtras,
        lastSavePreset: "flac",
        castEnabled: false,
        castTranscodeFormat: .mp3,
        mcp: BridgeMcpConfig(enabled: false, port: 47777),
        subsonic: BridgeSubsonicConfig(
            enabled: false,
            port: 4533,
            username: "",
            network: .thisMachine
        ),
        discogsTokenStatus: .notConfigured,
        discogsUsable: false
    )
}

/// An empty upload queue, as a freshly opened library's core reports it.
@MainActor
private var emptySeed: LibrarySessionSeed {
    LibrarySessionSeed(
        storage: nil,
        uploadsPaused: false
    )
}

/// A handle-less `AppHandle` that answers the reads the macOS `AppService`
/// constructor and its subscriptions make, so a test can build the real
/// service without a live core.
private final class FakeAppHandle: AppHandle, @unchecked Sendable {
    private let queue = FakeQueue()
    private let playbackValues = FakePlaybackValues()
    private let queueAdditions = FakeQueueAdditions()
    private let timeline = FakePlaybackTimeline()
    private let syncStatus = FakeSyncStatus()
    private let syncActivity = FakeSyncActivity()
    private let invites = FakeInvites()
    private let subscribedLock = NSLock()
    private var subscribedNames: [String] = []

    /// The membership and library-name reads opened so far, in order.
    var subscribed: [String] { subscribedLock.withLock { subscribedNames } }
    /// How many times an invites read has been ended.
    var invitesCancels: Int { invites.cancels }

    private func record(_ name: String) {
        subscribedLock.recording { subscribedNames.append(name) }
    }

    func deliverSyncActivity(_ activity: BridgeSyncActivity) {
        syncActivity.values.deliver(activity)
    }

    func deliverSyncStatus(_ status: BridgeSyncStatusSnapshot) {
        syncStatus.values.deliver(status)
    }

    func failSyncStatusRead(_ error: BridgeError) {
        syncStatus.values.fail(error)
    }

    /// The library's location, as `storageLocation()` reads it.
    var location: BridgeStorageLocation?

    override func storageLocation() async throws -> BridgeStorageLocation? {
        location
    }

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func subscribeConfig() -> ConfigSubscription {
        SilentConfig()
    }

    override func subscribeSyncStatus() -> SyncStatusSubscription {
        syncStatus
    }

    override func feedDownloadTransfers() -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func feedActivityBars() -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func subscribeSyncActivity() -> SyncActivitySubscription {
        syncActivity
    }

    override func subscribePlaybackValues() -> PlaybackValuesSubscription {
        playbackValues
    }

    override func subscribeQueueAdditions() -> QueueAdditionsSubscription {
        queueAdditions
    }

    override func subscribePlaybackTimeline() -> PlaybackTimelineSubscription {
        timeline
    }

    override func feedPlaybackPositions() -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func feedTrackBuffered() -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func feedTrackStorageWaits() -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func subscribeUploadsPaused() -> UploadsPausedSubscription {
        SilentUploadsPaused()
    }

    override func subscribeDownloadCounts() -> DownloadCountsSubscription {
        SilentDownloadCounts()
    }

    override func feedOutboxTransfers() -> BridgeOutboxFeeds {
        .streams(
            releases: NoopKeyedValueStream(),
            total: NoopKeyedValueStream()
        )
    }

    override func subscribeCastReceivers() -> CastReceiversSubscription {
        NoCastReceivers()
    }

    override func subscribeLibraryName() -> LibraryNameSubscription {
        record("libraryName")
        return SilentLibraryName()
    }

    override func subscribeMembers() -> MembersSubscription {
        record("members")
        return SilentMembers()
    }

    override func subscribeProfiles() -> ProfilesSubscription {
        record("profiles")
        return SilentProfiles()
    }

    override func subscribeInvites() -> InvitesSubscription {
        record("invites")
        return invites
    }

    override func subscribeRequestedProfile() -> RequestedProfileSubscription {
        record("requestedProfile")
        return SilentRequestedProfile()
    }

    override func subscribeAdminChanges() -> AdminChangesSubscription {
        record("adminChanges")
        return SilentAdminChanges()
    }

    // A screen showing the people reads them once as it opens.
    override func people() async throws -> [BridgePerson] { [] }

    override func ownPerson() async throws -> BridgePerson? { nil }

    func deliverInvites(
        _ value: [BridgeInvite],
        refused: [BridgeRefusedInvite] = []
    ) {
        invites.values.deliver(
            value.map { .open(invite: $0) }
                + refused.map { .refused(refused: $0) }
        )
    }

    override func subscribeQueue() -> QueueSubscription {
        queue
    }

    func deliverQueue(_ value: BridgeQueueSnapshot) {
        queue.deliver(value)
    }

    func deliverPlayback(_ value: BridgePlaybackValues) {
        playbackValues.deliver(value)
    }

    func deliverQueueAddition(_ addition: BridgeQueueAddition) {
        queueAdditions.deliver(addition)
    }

    func deliverTimeline(_ timeline: BridgePlaybackTimeline) {
        self.timeline.deliver(timeline)
    }
}
