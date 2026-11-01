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
        try await Wait.until { appService.stateForTesting.volume == 0.5 }
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
            .sink { addedCounts.append($0) }
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
        #expect(store.indicator == .loading)

        let synced = BridgeSyncStatusSnapshot(
            error: nil,
            connected: true,
            pending: .ready(records: []),
            indicator: .synced
        )
        handle.deliverSyncStatus(synced)
        try await Wait.until { store.indicator == .synced }
        #expect(store.syncReady)

        handle.failSyncStatusRead(
            .Diagnostic(category: .database, detail: "the read failed")
        )
        try await Wait.until { store.indicator == .unreadable }

        handle.deliverSyncStatus(synced)
        try await Wait.until { store.indicator == .synced }
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
        handle.deliverPlayback(playbackValues(state: state))
        handle.deliverTimeline(line(trackId: "track-1", positionMs: 30_000))

        try await Wait.until {
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

        try await Wait.until {
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
/// constructor and its subscriptions make, so a test can build the real
/// service without a live core.
private final class FakeAppHandle: AppHandle, @unchecked Sendable {
    private let queue = FakeQueue()
    private let playbackValues = FakePlaybackValues()
    private let queueAdditions = FakeQueueAdditions()
    private let timeline = FakePlaybackTimeline()
    private let syncStatus = FakeSyncStatus()

    func deliverSyncStatus(_ status: BridgeSyncStatusSnapshot) {
        syncStatus.values.deliver(status)
    }

    func failSyncStatusRead(_ error: BridgeError) {
        syncStatus.values.fail(error)
    }

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    // The service reports the network path as soon as it starts; nothing
    // here syncs.
    override func reportNetworkChange() {}

    override func subscribeConfig() -> ConfigSubscription {
        SilentConfig()
    }

    override func subscribeSyncStatus() -> SyncStatusSubscription {
        syncStatus
    }

    override func feedDownloadTransfers(
        activity _: any KeyActivityCallback
    ) -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func subscribeSyncActivity(
        callback _: any SyncActivityCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
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

    override func feedPlaybackPositions(
        activity _: any KeyActivityCallback
    ) -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func subscribeUploadsPaused() -> UploadsPausedSubscription {
        SilentUploadsPaused()
    }

    override func feedReleaseUploadTransfers(
        activity _: any KeyActivityCallback,
        failure _: any StreamFailureCallback
    ) -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func feedUploadTotalTransfer(
        activity _: any KeyActivityCallback,
        failure _: any StreamFailureCallback
    ) -> KeyedValueStream {
        NoopKeyedValueStream()
    }

    override func subscribeCastDevices() -> CastDevicesSubscription {
        NoCastDevices()
    }

    override func subscribeLibraryName() -> LibraryNameSubscription {
        SilentLibraryName()
    }

    override func subscribeMembers() -> MembersSubscription {
        SilentMembers()
    }

    override func subscribeProfiles() -> ProfilesSubscription {
        SilentProfiles()
    }

    override func subscribeInvites(
        callback _: any InvitesCallback
    ) -> LiveSubscription {
        NoopLiveSubscription()
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

/// A cast device list that lists nothing: its one pull answers that it was
/// cancelled, which ends the screen's pulls.
private final class NoCastDevices: CastDevicesSubscription, @unchecked Sendable
{
    init() {
        super.init(noHandle: CastDevicesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> [BridgeCastDevice] {
        throw BridgeError.Cancelled
    }

    override func cancel() async throws {}
}

/// Values a test hands the app one at a time, each answering the app's next
/// pull, and failed reads in their place; cancelling answers a waiting pull
/// that it was cancelled.
private final class PullQueue<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var waiting: [Result<Value, any Error>] = []
    private var puller: CheckedContinuation<Value, any Error>?

    func deliver(_ value: Value) { answer(.success(value)) }

    /// The next pull answers that its read failed with `error`.
    func fail(_ error: any Error) { answer(.failure(error)) }

    private func answer(_ result: Result<Value, any Error>) {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            if puller == nil { waiting.append(result) }
            return puller
        }
        puller?.resume(with: result)
    }

    func next() async throws -> Value {
        try await withCheckedThrowingContinuation { continuation in
            let result = lock.withLock {
                if waiting.isEmpty {
                    puller = continuation
                    return Result<Value, any Error>?.none
                }
                return waiting.removeFirst()
            }
            if let result { continuation.resume(with: result) }
        }
    }

    func cancel() {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            return puller
        }
        puller?.resume(throwing: BridgeError.Cancelled)
    }
}

/// The sync status a test hands the app one value, or one failed read, at a
/// time.
private final class FakeSyncStatus: SyncStatusSubscription, @unchecked Sendable
{
    let values = PullQueue<BridgeSyncStatusSnapshot>()

    init() {
        super.init(noHandle: SyncStatusSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeSyncStatusSnapshot {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Preferences that never change after the ones the service was built with:
/// a pull waits until it is cancelled.
private final class SilentConfig: ConfigSubscription, @unchecked Sendable {
    private let values = PullQueue<BridgeConfig>()

    init() {
        super.init(noHandle: ConfigSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeConfig {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Playback values a test hands the app one at a time.
private final class FakePlaybackValues: PlaybackValuesSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<BridgePlaybackValues>()

    init() {
        super.init(noHandle: PlaybackValuesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ value: BridgePlaybackValues) { values.deliver(value) }

    override func next() async throws -> BridgePlaybackValues {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Timelines a test hands the app one at a time.
private final class FakePlaybackTimeline: PlaybackTimelineSubscription,
    @unchecked Sendable
{
    private let timelines = PullQueue<BridgePlaybackTimeline>()

    init() {
        super.init(noHandle: PlaybackTimelineSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ timeline: BridgePlaybackTimeline) {
        timelines.deliver(timeline)
    }

    override func next() async throws -> BridgePlaybackTimeline {
        try await timelines.next()
    }

    override func cancel() async throws { timelines.cancel() }
}

/// Queue values a test hands the app one at a time.
private final class FakeQueue: QueueSubscription, @unchecked Sendable {
    private let values = PullQueue<BridgeQueueSnapshot>()

    init() {
        super.init(noHandle: QueueSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ value: BridgeQueueSnapshot) { values.deliver(value) }

    override func next() async throws -> BridgeQueueSnapshot {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Whether uploads are paused, which never changes: a pull waits until it
/// is cancelled.
private final class SilentUploadsPaused: UploadsPausedSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<Bool>()

    init() {
        super.init(noHandle: UploadsPausedSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> Bool { try await values.next() }

    override func cancel() async throws { values.cancel() }
}

/// The library's name, which never changes: a pull waits until it is
/// cancelled.
private final class SilentLibraryName: LibraryNameSubscription,
    @unchecked Sendable
{
    private let values = PullQueue<String>()

    init() {
        super.init(noHandle: LibraryNameSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> String { try await values.next() }

    override func cancel() async throws { values.cancel() }
}

/// The members, who never change: a pull waits until it is cancelled.
private final class SilentMembers: MembersSubscription, @unchecked Sendable {
    private let values = PullQueue<BridgeMembership>()

    init() {
        super.init(noHandle: MembersSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> BridgeMembership {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// The profiles, which never change: a pull waits until it is cancelled.
private final class SilentProfiles: ProfilesSubscription, @unchecked Sendable {
    private let values = PullQueue<[BridgePersonProfile]>()

    init() {
        super.init(noHandle: ProfilesSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func next() async throws -> [BridgePersonProfile] {
        try await values.next()
    }

    override func cancel() async throws { values.cancel() }
}

/// Queue additions a test hands the app one at a time.
private final class FakeQueueAdditions: QueueAdditionsSubscription,
    @unchecked Sendable
{
    private let additions = PullQueue<BridgeQueueAddition>()

    init() {
        super.init(noHandle: QueueAdditionsSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    func deliver(_ addition: BridgeQueueAddition) {
        additions.deliver(addition)
    }

    override func next() async throws -> BridgeQueueAddition {
        try await additions.next()
    }

    override func cancel() async throws { additions.cancel() }
}

/// A stream nothing feeds, which holds nothing.
private final class NoopKeyedValueStream: KeyedValueStream,
    @unchecked Sendable
{
    init() {
        super.init(noHandle: KeyedValueStream.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func latest(key _: Data) -> BridgeFastValue? {
        nil
    }

    override func cancel() {}
}
