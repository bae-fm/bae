import BaeKit
import Foundation
import Testing

@testable import bae

@Suite("SyncStatusStore")
struct SyncStatusStoreTests {
    @MainActor
    @Test("config values cannot overwrite the sync state")
    func configValuesDoNotPublishTheState() {
        let config = ConfigStore(
            config: Config(bridge: bridgeConfig()),
            storage: nil
        )
        let sync = SyncStatusStore()

        sync.apply(status(.previewSynced))
        config.applyConfigSnapshot(bridgeConfig())
        #expect(sync.shown?.status == .synced)

        sync.apply(status(.previewNotSynced))
        #expect(sync.shown?.status == .notSynced)
    }

    /// Before the status is first read the page shows nothing in its block,
    /// never that the library is synced, and the list waits.
    @MainActor
    @Test("the state is absent until the status is read")
    func theStateIsAbsentUntilTheStatusIsRead() {
        let sync = SyncStatusStore()
        #expect(sync.shown == nil)
        #expect(!sync.unreadable)
        #expect(sync.pending == nil)
        sync.apply(status(.previewSynced))
        #expect(sync.shown?.status == .synced)
        #expect(sync.pending == [])
    }

    /// A read that fails withdraws the state with the snapshot it came from,
    /// and says it couldn't be read until a read succeeds.
    @MainActor
    @Test("a failed read withdraws the state until a read succeeds")
    func aFailedReadWithdrawsTheState() {
        let sync = SyncStatusStore()
        sync.apply(status(.previewSynced))

        sync.fail(
            BridgeError.Diagnostic(
                category: .database,
                detail: "the read failed"
            )
        )
        #expect(sync.shown == nil)
        #expect(sync.unreadable)

        sync.apply(status(.previewSynced))
        #expect(sync.shown?.status == .synced)
        #expect(!sync.unreadable)
    }

    /// A connected library with something coven waits on reaches the list
    /// that offers its action, whatever state the page names.
    @MainActor
    @Test("a pending record reaches the list")
    func pendingRecordReachesTheList() throws {
        let record = BridgePendingRecord(
            subject: BridgeMessage(
                key: "core.pending.operation.invite",
                args: []
            ),
            reason: BridgePendingReason(
                key: "core.pending.reason.failed",
                detail: nil,
                causes: []
            ),
            action: .retryOperation(id: 7),
            actionKey: "core.pending.action.retry",
            sendsFiles: false
        )
        let sync = SyncStatusStore()

        sync.apply(
            BridgeSyncStatusSnapshot(
                shown: .previewSynced,
                pending: [record],
                setupRunning: false,
                undoneSetup: nil
            )
        )

        let first = try #require(sync.pending?.first)
        #expect(first.action == .retryOperation(id: 7))
    }

    /// The state's words, cause and last sync are core's, in the person's
    /// language: a provider's wait names it and when sync resumes.
    @Test("the causes that name the device are worded for the Mac")
    func deviceCausesAreWordedForTheMac() {
        #expect(
            BridgeSyncCause.deviceDiskFull.macLine
                == "There is no space left on this Mac. Free some space and bae will try again."
        )
        #expect(
            BridgeSyncCause.deviceKeysUnreadable.macLine
                == "The keychain is locked. Unlock it to keep syncing."
        )
        #expect(
            BridgeSyncCause.libraryUnreadable.macLine
                == "bae could not read the library on this Mac."
        )
    }

    @MainActor
    @Test("a state is worded from core's keys and arguments")
    func aStateIsWordedFromCoresKeys() throws {
        let resumes = Date(timeIntervalSince1970: 1_000_000)
        let shown = BridgeSyncShown(
            status: .waitingToSync,
            statusKey: "core.sync.status.waiting_to_sync",
            tone: .warning,
            cause: .message(
                message: BridgeMessage(
                    key: "core.sync.cause.provider_wait",
                    args: [BridgeMessageArg(name: "provider", value: "Dropbox")]
                ),
                resumesAt: resumes
            ),
            fix: nil,
            lastSynced: Date(timeIntervalSinceNow: -7_200)
        )
        #expect(shown.statusLine == "Waiting to sync")
        let cause = try #require(shown.causeLine)
        #expect(
            cause.hasPrefix("Dropbox asked bae to wait. Syncing resumes at ")
        )
        #expect(
            cause.hasSuffix(
                resumes.formatted(date: .omitted, time: .shortened) + "."
            )
        )
        #expect(shown.lastSyncedLine == "2 hours ago")
        #expect(shown.statusTone.color == StatusTone.warning.color)
    }

    private func status(_ shown: BridgeSyncShown) -> BridgeSyncStatusSnapshot {
        BridgeSyncStatusSnapshot(
            shown: shown,
            pending: [],
            setupRunning: false,
            undoneSetup: nil
        )
    }

    private func bridgeConfig() -> BridgeConfig {
        BridgeConfig(
            libraryId: "test-library",
            libraryPath: "/test",
            pauseBetweenSides: false,
            sidePauseCountdown: .off,
            maxConcurrentUploads: 1,
            maxConcurrentDownloads: 1,
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
            saveExtras: BridgeSaveExtras(
                embedCover: true,
                coverFile: true,
                loudnessTags: true
            ),
            lastSavePreset: "flac",
            castEnabled: false,
            castTranscodeFormat: .mp3,
            mcp: BridgeMcpConfig(enabled: false, port: 47_777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4_533,
                username: "",
                network: .thisMachine
            ),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false
        )
    }
}
