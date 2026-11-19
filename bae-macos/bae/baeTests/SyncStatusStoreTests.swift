import BaeKit
import Foundation
import Testing

@testable import bae

@Suite("SyncStatusStore")
struct SyncStatusStoreTests {
    /// A folder renamed at the provider reaches the screen with the next
    /// config value, with no read of its own.
    @MainActor
    @Test("each config value brings the storage location it carries")
    func configValuesBringTheStorageLocation() {
        let config = ConfigStore(config: Config(bridge: bridgeConfig()))
        #expect(config.storage == nil)
        #expect(!config.syncs)

        let location = BridgeStorageLocation.dropbox(
            location: .appFolder,
            accountLabel: "name@example.com",
            folderLabel: nil
        )
        config.applyConfigSnapshot(bridgeConfig(storage: location))
        #expect(config.storage == location)
        #expect(config.syncs)

        let renamed = BridgeStorageLocation.dropbox(
            location: .appFolder,
            accountLabel: "name@example.com",
            folderLabel: BridgeFolderLabel(
                name: "Renamed Music",
                webUrl: "https://www.dropbox.com/home/Renamed%20Music"
            )
        )
        config.applyConfigSnapshot(bridgeConfig(storage: renamed))
        #expect(config.storage == renamed)
    }

    @MainActor
    @Test("config values cannot overwrite the sync state")
    func configValuesDoNotPublishTheState() {
        let config = ConfigStore(
            config: Config(bridge: bridgeConfig())
        )
        let sync = SyncStatusStore()

        sync.apply(status(.fixtureSynced))
        config.applyConfigSnapshot(bridgeConfig())
        #expect(sync.shown?.status == .synced)

        sync.apply(status(.fixtureNotSynced))
        #expect(sync.shown?.status == .notSynced)
    }

    /// Before the status is first read the page shows nothing in its block,
    /// never that the library is synced.
    @MainActor
    @Test("the state is absent until the status is read")
    func theStateIsAbsentUntilTheStatusIsRead() {
        let sync = SyncStatusStore()
        #expect(sync.shown == nil)
        #expect(!sync.unreadable)
        sync.apply(status(.fixtureSynced))
        #expect(sync.shown?.status == .synced)
    }

    /// A read that fails withdraws the state with the snapshot it came from,
    /// and says it couldn't be read until a read succeeds.
    @MainActor
    @Test("a failed read withdraws the state until a read succeeds")
    func aFailedReadWithdrawsTheState() {
        let sync = SyncStatusStore()
        sync.apply(status(.fixtureSynced))

        sync.fail(
            BridgeError.Diagnostic(
                category: .database,
                detail: "the read failed"
            )
        )
        #expect(sync.shown == nil)
        #expect(sync.unreadable)

        sync.apply(status(.fixtureSynced))
        #expect(sync.shown?.status == .synced)
        #expect(!sync.unreadable)
    }

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
        #expect(
            BridgeSyncCause.enterNewRestoreCode.macLine
                == "Enter the new restore code to restore the library on this Mac. The restore discards any changes this Mac has not sent."
        )
    }

    /// The state's words, cause and last sync are core's, in the person's
    /// language: a provider's wait names it, and the line under it counts
    /// down to when sync resumes.
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
                countdown: BridgeCountdown(
                    key: "core.sync.cause.provider_wait.countdown",
                    until: resumes
                )
            ),
            fix: nil,
            lastSynced: Date(timeIntervalSinceNow: -7_200)
        )
        #expect(shown.statusLine == "Waiting to sync")
        #expect(shown.causeLine == "Dropbox asked bae to slow down.")
        let countdown = try #require(shown.causeCountdown)
        #expect(
            countdown.line(at: resumes.addingTimeInterval(-42))
                == "Resuming in 42 seconds."
        )
        #expect(
            countdown.line(at: resumes.addingTimeInterval(-0.5))
                == "Resuming in 1 second."
        )
        #expect(shown.lastSyncedLine == "2 hours ago")
        #expect(shown.statusTone.color == StatusTone.warning.color)
    }

    /// A cause that names no moment has no countdown under it.
    @Test("a cause with no moment has no countdown")
    func aCauseWithNoMomentHasNoCountdown() {
        let shown = BridgeSyncShown(
            status: .syncFailed,
            statusKey: "core.sync.failed",
            tone: .danger,
            cause: .deviceDiskFull,
            fix: .retryNow,
            lastSynced: nil
        )
        #expect(shown.causeCountdown == nil)
    }

    private func status(_ shown: BridgeSyncShown) -> BridgeSyncStatusSnapshot {
        BridgeSyncStatusSnapshot(
            shown: shown,
            setupRunning: false,
            undoneSetup: nil
        )
    }

    private func bridgeConfig(
        storage: BridgeStorageLocation? = nil
    ) -> BridgeConfig {
        BridgeConfig(
            libraryId: "test-library",
            libraryPath: "/test",
            storage: storage,
            storageAccess: nil,
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
            lastSavePreset: BridgeSavePreset(
                id: "flac",
                name: "FLAC",
                codec: .flac(bitDepth: .sameAsSource)
            ),
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
