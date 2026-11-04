import BaeKit
import Testing

@Suite("SyncStatusStore")
struct SyncStatusStoreTests {
    @MainActor
    @Test("config values cannot overwrite the status line")
    func configValuesDoNotPublishTheLine() {
        let config = ConfigStore(
            config: Config(bridge: bridgeConfig()),
            storage: nil
        )
        let sync = SyncStatusStore()

        sync.apply(status(indicator: .synced))
        config.applyConfigSnapshot(bridgeConfig())
        #expect(sync.line == .read(.synced))

        sync.apply(status(indicator: nil))
        #expect(sync.line == .noStorage)
    }

    /// With no storage set up on this device there is no sync to describe:
    /// no line, where Settings offers signing in to the recorded storage.
    @MainActor
    @Test("no storage on this device has no line")
    func noStorageHasNoLine() {
        let sync = SyncStatusStore()
        sync.apply(status(indicator: nil))
        #expect(sync.line == .noStorage)
        #expect(sync.statusText == nil)
    }

    /// The hour-losing case: a cycle fails, core records the fault, and every
    /// surface renders the category line — "Something went wrong." — which names
    /// no fault at all. The store hands the concrete text on with it, so a
    /// surface has both to show.
    @MainActor
    @Test("a failed cycle surfaces its fault, not just its category line")
    func failedCycleSurfacesItsFault() throws {
        let fault =
            "sync cycle: pull Store commits: database: retained Merge replay has an unresolved foreign-key dependency"
        let sync = SyncStatusStore()

        sync.apply(
            BridgeSyncStatusSnapshot(
                error: .Diagnostic(category: .internal, detail: fault),
                pending: [],
                indicator: nil,
                reconnectOffered: false
            )
        )

        let error = try #require(sync.error)
        #expect(error.line == BridgeErrorCategory.internal.localizedLine)
        #expect(error.detail == fault, "the copy button keeps the whole chain")
        #expect(
            error.detailSummary == fault,
            "and a surface can render it inline"
        )
    }

    /// A connected library with something coven waits on is not "Synced" —
    /// core decides that and says so on the snapshot, so no surface re-derives
    /// it — and the record reaches the list that offers its action.
    @MainActor
    @Test("a pending record makes the line wait and reaches the list")
    func pendingRecordMakesTheLineWait() throws {
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
            actionKey: "core.pending.action.retry"
        )
        let sync = SyncStatusStore()

        sync.apply(
            BridgeSyncStatusSnapshot(
                error: nil,
                pending: [record],
                indicator: .waiting(items: 1),
                reconnectOffered: false
            )
        )

        #expect(sync.line == .read(.waiting(items: 1)))
        let first = try #require(sync.pending?.first)
        #expect(first.action == .retryOperation(id: 7))
    }

    /// Before the status is first read the line says it is checking, never
    /// that the library is synced, and the list waits.
    @MainActor
    @Test("the line checks until the status is read")
    func theLineChecksUntilTheStatusIsRead() {
        let sync = SyncStatusStore()
        #expect(sync.line == .checking)
        #expect(sync.pending == nil)
        sync.apply(status(indicator: .synced))
        #expect(sync.line == .read(.synced))
        #expect(sync.pending == [])
    }

    @MainActor
    @Test("a healthy sync has no error to render")
    func healthySyncHasNoError() {
        let sync = SyncStatusStore()
        sync.apply(status(indicator: .synced))
        #expect(sync.error == nil)
    }

    /// A start that failed with storage set up offers Reconnect beside why,
    /// as core says; a read that fails withdraws it with the snapshot.
    @MainActor
    @Test("a failed start offers Reconnect while its status stands")
    func failedStartOffersReconnect() {
        let sync = SyncStatusStore()
        sync.apply(status(indicator: .synced))
        #expect(!sync.offersReconnect)

        sync.apply(
            BridgeSyncStatusSnapshot(
                error: .Diagnostic(
                    category: .membership,
                    detail: "the store key is missing"
                ),
                pending: [],
                indicator: .failed,
                reconnectOffered: true
            )
        )
        #expect(sync.offersReconnect)

        sync.fail(
            BridgeError.Diagnostic(
                category: .database,
                detail: "the read failed"
            )
        )
        #expect(!sync.offersReconnect)
    }

    @MainActor
    @Test("a required app update renders its line")
    func requiredUpdateRendersItsLine() throws {
        let sync = SyncStatusStore()
        sync.apply(
            BridgeSyncStatusSnapshot(
                error: .Diagnostic(
                    category: .syncUpdateRequired,
                    detail: "schema 17 required"
                ),
                pending: [],
                indicator: .synced,
                reconnectOffered: false
            )
        )
        #expect(
            try #require(sync.error).line
                == BridgeErrorCategory.syncUpdateRequired.localizedLine
        )
    }

    private func status(
        indicator: BridgeSyncIndicator?
    ) -> BridgeSyncStatusSnapshot {
        BridgeSyncStatusSnapshot(
            error: nil,
            pending: [],
            indicator: indicator,
            reconnectOffered: false
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
            hasSourceToSearch: true,
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            castEnabled: false,
            castTranscodeFormat: .mp3,
            mcp: BridgeMcpConfig(enabled: false, port: 47_777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4_533,
                username: "",
                bindAddress: "127.0.0.1"
            ),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false
        )
    }
}
