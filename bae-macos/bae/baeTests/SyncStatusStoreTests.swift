import BaeKit
import Testing

@Suite("SyncStatusStore")
struct SyncStatusStoreTests {
    @MainActor
    @Test("config values cannot overwrite readiness transitions")
    func configValuesDoNotPublishReadiness() {
        let config = ConfigStore(
            config: Config(bridge: bridgeConfig()),
            libraryName: "Test Library",
            storage: nil
        )
        let sync = SyncStatusStore()

        sync.apply(status(syncReady: true))
        config.applyConfigSnapshot(bridgeConfig())
        #expect(sync.syncReady)

        sync.apply(status(syncReady: false))
        #expect(!sync.syncReady)
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
                connected: false,
                pending: .loading,
                indicator: .unconnected
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
                connected: true,
                pending: .ready(records: [record]),
                indicator: .waiting(items: 1)
            )
        )

        #expect(sync.indicator == .waiting(items: 1))
        guard case .ready(let records) = sync.pending else {
            Issue.record("the read records reach the list")
            return
        }
        let first = try #require(records.first)
        #expect(first.action == .retryOperation(id: 7))
    }

    @MainActor
    @Test("a healthy sync has no error to render")
    func healthySyncHasNoError() {
        let sync = SyncStatusStore()
        sync.apply(status(syncReady: true))
        #expect(sync.error == nil)
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
                connected: true,
                pending: .ready(records: []),
                indicator: .synced
            )
        )
        #expect(
            try #require(sync.error).line
                == BridgeErrorCategory.syncUpdateRequired.localizedLine
        )
    }

    private func status(syncReady: Bool) -> BridgeSyncStatusSnapshot {
        BridgeSyncStatusSnapshot(
            error: nil,
            connected: syncReady,
            pending: syncReady ? .ready(records: []) : .loading,
            indicator: syncReady ? .synced : .unconnected
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
