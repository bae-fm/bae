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
@Suite("UiEventDispatcher transport arms", .serialized)
struct UiEventDispatcherTransportTests {
    @Test("playbackPlaying sets the playing track and pushes rate 1.0")
    func playbackPlayingSetsPlayingTrack() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )

        sink(playingEvent(trackId: "t1"))

        guard case .playing(let track) = appService.playbackStore.nowPlaying
        else {
            Issue.record("expected .playing")
            return
        }
        #expect(track.trackId == "t1")
        let info = infoCenter.nowPlayingInfo
        #expect(info?[MPMediaItemPropertyTitle] as? String == "Track Title")
        #expect(info?[MPNowPlayingInfoPropertyPlaybackRate] as? Double == 1.0)
    }

    @Test("playbackPaused sets the paused track and pushes rate 0.0")
    func playbackPausedSetsPausedTrack() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )

        sink(pausedEvent(trackId: "t1"))

        guard
            case .paused(let track, let reason) = appService.playbackStore
                .nowPlaying
        else {
            Issue.record("expected .paused")
            return
        }
        #expect(track.trackId == "t1")
        #expect(reason == .manual)
        let info = infoCenter.nowPlayingInfo
        #expect(info?[MPNowPlayingInfoPropertyPlaybackRate] as? Double == 0.0)
    }

    @Test("a bare playbackLoading retains the prior track and info")
    func bareLoadingRetainsPriorTrackAndInfo() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )
        sink(playingEvent(trackId: "t1"))
        let infoBeforeLoading = infoCenter.nowPlayingInfo

        sink(.playbackLoading(trackId: "t2", track: nil))

        #expect(appService.playbackStore.nowPlaying.track?.trackId == "t1")
        #expect(appService.playbackStore.nowPlaying.loadingTrackId == "t2")
        let title =
            infoCenter.nowPlayingInfo?[MPMediaItemPropertyTitle] as? String
        let priorTitle = infoBeforeLoading?[MPMediaItemPropertyTitle] as? String
        #expect(title == priorTitle)
    }

    @Test("a resolved playbackLoading swaps the target and pushes rate 0.0")
    func resolvedLoadingSwapsTargetAndInfo() {
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer { infoCenter.nowPlayingInfo = nil }
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )
        sink(playingEvent(trackId: "t1"))
        sink(.playbackLoading(trackId: "t2", track: nil))

        sink(
            .playbackLoading(
                trackId: "t2",
                track: BridgeLoadingTrackInfo(
                    trackTitle: "Target Title",
                    artistNames: "Artist Name",
                    albumId: "album-1",
                    albumTitle: "Album Title",
                    coverImage: nil,
                    durationMs: 180_000
                )
            )
        )

        #expect(appService.playbackStore.nowPlaying.track?.trackId == "t2")
        let info = infoCenter.nowPlayingInfo
        #expect(info?[MPMediaItemPropertyTitle] as? String == "Target Title")
        #expect(info?[MPNowPlayingInfoPropertyPlaybackRate] as? Double == 0.0)
    }

    @Test(
        "playbackStopped resets the store, clears info, and disables transport"
    )
    func playbackStoppedClearsEverything() {
        let center = MPRemoteCommandCenter.shared()
        let infoCenter = MPNowPlayingInfoCenter.default()
        infoCenter.nowPlayingInfo = nil
        defer {
            infoCenter.nowPlayingInfo = nil
            center.nextTrackCommand.isEnabled = false
            center.previousTrackCommand.isEnabled = false
        }
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )
        sink(playingEvent(trackId: "t1"))
        appService.mediaControlService.updateCommandAvailability(
            hasNext: true,
            hasPrevious: true
        )

        sink(.playbackStopped)

        guard case .stopped = appService.playbackStore.nowPlaying else {
            Issue.record("expected .stopped")
            return
        }
        guard
            case .reset = appService.playbackStore.playbackPositionSubject
                .value
        else {
            Issue.record("expected playback position to reset")
            return
        }
        #expect(infoCenter.nowPlayingInfo == nil)
        #expect(!center.nextTrackCommand.isEnabled)
        #expect(!center.previousTrackCommand.isEnabled)
    }
}

@MainActor
@Suite("UiEventDispatcher queue", .serialized)
struct UiEventDispatcherQueueTests {
    @Test(
        "queueUpdated applies its snapshot to the queue and transport commands"
    )
    func queueUpdatedAppliesSnapshot() {
        let center = MPRemoteCommandCenter.shared()
        center.nextTrackCommand.isEnabled = false
        center.previousTrackCommand.isEnabled = false
        defer {
            center.nextTrackCommand.isEnabled = false
            center.previousTrackCommand.isEnabled = false
        }

        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )

        sink(
            .queueUpdated(
                snapshot: BridgeQueueSnapshot(
                    manual: [
                        makeEntry(entryId: "manual-1", trackId: "track-1")
                    ],
                    context: BridgePlaybackContext(
                        kind: .release,
                        sourceTitle: nil,
                        shuffled: false,
                        upcoming: [
                            makeEntry(entryId: "upcoming-1", trackId: "track-2")
                        ],
                        upcomingTotal: 1
                    ),
                    hasNext: true,
                    hasPrevious: false,
                    revision: 1
                )
            )
        )

        #expect(
            appService.playbackStore.manualQueue.map(\.entryId) == ["manual-1"]
        )
        #expect(
            appService.playbackStore.queueContext?.upcoming.map(\.entryId)
                == ["upcoming-1"]
        )
        #expect(center.nextTrackCommand.isEnabled)
        #expect(!center.previousTrackCommand.isEnabled)
    }

    /// The bridge's lag-recovery path is the only producer of a `.queue`
    /// invalidation; the base `AppService`'s queue projection is its consumer
    /// on both platforms. Invalidating `.queue` refetches through
    /// `getQueueSnapshot` and lands the same `applyQueueSnapshot` the direct
    /// `queueUpdated` arm uses.
    @Test("a queue invalidation refetches and applies the snapshot")
    func queueInvalidationRefetchesAndApplies() async {
        let center = MPRemoteCommandCenter.shared()
        center.nextTrackCommand.isEnabled = false
        center.previousTrackCommand.isEnabled = false
        defer {
            center.nextTrackCommand.isEnabled = false
            center.previousTrackCommand.isEnabled = false
        }

        let handle = FakeAppHandle()
        handle.queueSnapshot = BridgeQueueSnapshot(
            manual: [makeEntry(entryId: "manual-2", trackId: "track-3")],
            context: nil,
            hasNext: false,
            hasPrevious: true,
            revision: 1
        )
        let appService = makeAppService(handle: handle)
        appService.registerCommonProjections()

        appService.projectionRegistry.invalidate(.queue)
        await waitUntil {
            appService.playbackStore.manualQueue.map(\.entryId)
                == ["manual-2"]
        }

        #expect(
            appService.playbackStore.manualQueue.map(\.entryId)
                == ["manual-2"]
        )
        #expect(!center.nextTrackCommand.isEnabled)
        #expect(center.previousTrackCommand.isEnabled)
    }

    private func makeEntry(entryId: String, trackId: String) -> BridgeQueueEntry
    {
        BridgeQueueEntry(
            entryId: entryId,
            trackId: trackId,
            title: "Track Title",
            artistNames: "Artist Name",
            durationClock: bridgeClock(ms: 180_000),
            albumTitle: "Album Title",
            coverImage: nil
        )
    }
}

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

        #expect(appService.uiStore.lastError != nil)
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

        #expect(appService.uiStore.lastError != nil)
    }
}

@MainActor
@Suite("UiEventDispatcher control arms", .serialized)
struct UiEventDispatcherControlTests {
    @Test("volumeChanged/muteChanged/repeatModeChanged/queueItemsAdded")
    func controlArmsWriteThePlaybackStore() {
        let appService = makeAppService()
        let sink = UiEventDispatcher.makeSink(
            appService: appService,
            onUnhandled: DesktopUiEvents.apply
        )
        var addedCounts: [Int] = []
        let subscription = appService.playbackStore.queueItemsAddedSubject
            .sink { addedCounts.append($0) }
        defer { subscription.cancel() }

        sink(.volumeChanged(volume: 0.4))
        sink(.muteChanged(isMuted: true))
        sink(.repeatModeChanged(mode: .context))
        sink(.queueItemsAdded(count: 3))

        #expect(appService.playbackStore.volume == 0.4)
        #expect(appService.playbackStore.isMuted)
        #expect(appService.playbackStore.repeatMode == .context)
        #expect(addedCounts == [3])
    }
}

@MainActor
@Suite("UiEventDispatcher outcome policy", .serialized)
struct UiEventDispatcherOutcomeTests {
    @Test("preview and import-loudness events are unhandled")
    func previewAndImportLoudnessAreUnhandled() {
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

private func playingEvent(trackId: String) -> BridgeUiEvent {
    .playbackPlaying(
        trackId: trackId,
        trackTitle: "Track Title",
        artistNames: "Artist Name",
        artistId: "artist-1",
        albumId: "album-1",
        albumTitle: "Album Title",
        coverImage: nil,
        durationMs: 200_000
    )
}

private func pausedEvent(trackId: String) -> BridgeUiEvent {
    .playbackPaused(
        trackId: trackId,
        trackTitle: "Track Title",
        artistNames: "Artist Name",
        artistId: "artist-1",
        albumId: "album-1",
        albumTitle: "Album Title",
        coverImage: nil,
        durationMs: 200_000,
        reason: .manual
    )
}

/// The five events the shared dispatcher declines: import preview playback and
/// per-track loudness-measurement progress. Every other `BridgeUiEvent` variant
/// belongs in `handledEvents` below.
private let unhandledEvents: [BridgeUiEvent] = [
    .previewIdle,
    .previewPlaying(path: "/tmp/preview.flac", durationMs: 1_000),
    .previewPaused(path: "/tmp/preview.flac", durationMs: 1_000),
    .previewProgress(positionMs: 0, progress: 0),
    .candidateImportLoudnessProgress(
        key: "k",
        tracksDone: 0,
        tracksTotal: 1,
        fraction: 0
    ),
]

private let handledEvents: [BridgeUiEvent] = [
    .invalidated(invalidation: .config),
    .playbackStopped,
    .playbackError(reason: .syncDisconnected),
    .playbackLoading(trackId: "t1", track: nil),
    playingEvent(trackId: "t1"),
    pausedEvent(trackId: "t1"),
    .playbackProgress(
        trackId: "t1",
        positionMs: 0,
        durationMs: 100_000,
        progress: 0
    ),
    .playbackSeeked(
        trackId: "t1",
        positionMs: 0,
        durationMs: 100_000,
        progress: 0
    ),
    .volumeChanged(volume: 0.5),
    .muteChanged(isMuted: false),
    .repeatModeChanged(mode: .off),
    .queueUpdated(
        snapshot: BridgeQueueSnapshot(
            manual: [],
            context: nil,
            hasNext: false,
            hasPrevious: false,
            revision: 1
        )
    ),
    .queueItemsAdded(count: 1),
    .releaseTransferProgress(releaseId: "r1", action: .pin),
    .releaseTransferEnded(releaseId: "r1"),
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
            encryptionKeyStored: false,
            encryptionKeyFingerprint: nil,
            pauseBetweenSides: false,
            maxConcurrentUploads: 3,
            maxConcurrentDownloads: 3,
            showRemainingTime: false,
                    libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            mcp: BridgeMcpConfig(enabled: false, port: 47777),
            subsonic: BridgeSubsonicConfig(
                enabled: false, port: 4533, username: "", bindAddress: "127.0.0.1"),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false,
            sync: nil
        ),
        initialOutbox: OutboxStore.emptySnapshot
    )
}

/// A handle-less `AppHandle` that answers the reads the macOS `AppService`
/// constructor makes so a dispatcher test can build the real service without a
/// live core. `queueSnapshot`, when set, answers `getQueueSnapshot` for the
/// lag-recovery projection test; any other call would hit the base FFI against
/// the null handle and trap.
private final class FakeAppHandle: AppHandle, @unchecked Sendable {
    var queueSnapshot: BridgeQueueSnapshot?

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

    override func getImportCandidates() -> BridgeImportCandidatesSnapshot {
        BridgeImportCandidatesSnapshot(
            watchedFolders: [],
            folderCandidates: [],
            invalidCandidates: [],
            boundaries: [],
            folderScanStatuses: []
        )
    }

    override func getQueueSnapshot() async throws -> BridgeQueueSnapshot {
        guard let queueSnapshot else {
            preconditionFailure(
                "FakeAppHandle.getQueueSnapshot called without a fixture"
            )
        }
        return queueSnapshot
    }
}
