import BaeKit
import MediaPlayer
import Testing

@testable import bae

/// Drives the real macOS `UiEventReducer` sink with a faked `AppHandle` (no live
/// core) and checks that a `queueUpdated` event lands its carried snapshot. Core
/// resolves the queue projection before emitting, so the event is the queue's
/// update path on macOS the same as on iOS/Windows/Android; the media-control
/// next/previous availability rides the same snapshot. The suite is serialized
/// and resets the process-global command center because it reads back the
/// transport-command enabled flags.
@MainActor
@Suite("UiEventReducer queue", .serialized)
struct UiEventReducerQueueTests {
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
        let sink = UiEventReducer.makeSink(appService: appService)

        sink(
            .queueUpdated(
                snapshot: BridgeQueueSnapshot(
                    manual: [
                        makeEntry(entryId: "manual-1", trackId: "track-1")
                    ],
                    context: BridgePlaybackContext(
                        kind: .release,
                        shuffled: false,
                        upcoming: [
                            makeEntry(entryId: "upcoming-1", trackId: "track-2")
                        ]
                    ),
                    hasNext: true,
                    hasPrevious: false
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

    private func makeEntry(entryId: String, trackId: String) -> BridgeQueueEntry
    {
        BridgeQueueEntry(
            entryId: entryId,
            trackId: trackId,
            title: "Track Title",
            artistNames: "Artist Name",
            durationMs: 180_000,
            albumTitle: "Album Title",
            coverImageId: nil
        )
    }

    private func makeAppService() -> bae.AppService {
        bae.AppService(
            appHandle: FakeAppHandle(),
            mediaControlService: MediaControlService(),
            uiStore: UiStore(),
            config: BridgeConfig(
                libraryId: "lib-test",
                libraryName: "Test Library",
                libraryPath: "/tmp/test",
                encryptionKeyStored: false,
                encryptionKeyFingerprint: nil,
                pauseBetweenSides: false,
                exportLocation: .askEachTime,
                exportFilenameTemplate: "",
                exportPresets: [],
                defaultTrackExportSelection: .original,
                defaultReleaseExportSelection: .original,
                mcp: BridgeMcpConfig(enabled: false, port: 47777),
                discogsTokenStatus: .notConfigured,
                discogsUsable: false,
                sync: nil
            ),
            initialOutbox: OutboxStore.emptySnapshot
        )
    }
}

/// A handle-less `AppHandle` that answers the reads the macOS `AppService`
/// constructor makes so a reducer test can build the real service without a live
/// core. The reducer's queue path touches no handle method, so only the
/// construction-time reads are overridden; any other call would hit the base
/// FFI against the null handle and trap.
private final class FakeAppHandle: AppHandle, @unchecked Sendable {
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
            paused: false
        )
    }

    override func getExportSnapshot() -> BridgeExportSnapshot {
        BridgeExportSnapshot(
            exports: [],
            total: BridgeExportProgress(queued: 0, active: 0, failed: 0),
            paused: false
        )
    }

    override func getImportCandidates() -> BridgeImportCandidatesSnapshot {
        BridgeImportCandidatesSnapshot(
            watchedFolders: [],
            folderCandidates: [],
            invalidCandidates: []
        )
    }
}
