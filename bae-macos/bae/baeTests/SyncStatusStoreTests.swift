import BaeKit
import Testing

@Suite("SyncStatusStore")
struct SyncStatusStoreTests {
    @MainActor
    @Test("config values cannot overwrite readiness transitions")
    func configValuesDoNotPublishReadiness() {
        let config = ConfigStore(config: Config(bridge: bridgeConfig()))
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
                canReconnect: true,
                blocked: [],
                lastSyncTime: nil,
                syncing: false,
                syncReady: false
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

    /// A cycle that completed but left an operation waiting on a person is not
    /// "Synced" — core decides that, so no surface re-derives it — and the
    /// operation reaches the settings row that offers to retry it.
    @MainActor
    @Test("a blocked operation reads as the error state and reaches the UI")
    func blockedOperationReadsAsError() throws {
        let sync = SyncStatusStore()

        sync.apply(
            BridgeSyncStatusSnapshot(
                error: nil,
                canReconnect: false,
                blocked: [
                    BridgeBlockedSyncOperation(
                        id: "write:write-1",
                        kind: .write,
                        description: "releases/release-3",
                        error: "blob release_files/file-7 is missing"
                    )
                ],
                lastSyncTime: 1_700_000_000_000,
                syncing: false,
                syncReady: true
            )
        )

        #expect(sync.indicator == .error)
        let operation = try #require(sync.blocked.first)
        #expect(operation.id == "write:write-1")
        #expect(operation.error == "blob release_files/file-7 is missing")
    }

    @MainActor
    @Test("a healthy sync has no error to render")
    func healthySyncHasNoError() {
        let sync = SyncStatusStore()
        sync.apply(status(syncReady: true))
        #expect(sync.error == nil)
    }

    @MainActor
    @Test("a required app update is visible without offering reconnection")
    func requiredUpdateHasNoReconnect() throws {
        let sync = SyncStatusStore()
        sync.apply(
            BridgeSyncStatusSnapshot(
                error: .Diagnostic(
                    category: .syncUpdateRequired,
                    detail: "schema 17 required"
                ),
                canReconnect: false,
                blocked: [],
                lastSyncTime: nil,
                syncing: false,
                syncReady: true
            )
        )
        #expect(
            try #require(sync.error).line
                == BridgeErrorCategory.syncUpdateRequired.localizedLine
        )
        #expect(sync.indicator == .error)
        #expect(!sync.canReconnect)
    }

    private func status(syncReady: Bool) -> BridgeSyncStatusSnapshot {
        BridgeSyncStatusSnapshot(
            error: nil,
            canReconnect: false,
            blocked: [],
            lastSyncTime: nil,
            syncing: false,
            syncReady: syncReady
        )
    }

    private func bridgeConfig() -> BridgeConfig {
        BridgeConfig(
            libraryId: "test-library",
            libraryName: "Test Library",
            libraryPath: "/test",
            pauseBetweenSides: false,
            maxConcurrentUploads: 1,
            maxConcurrentDownloads: 1,
            identifyAutomatically: true,
            defaultImportMetadataSource: .findOnline,
            showRemainingTime: false,
            libraryFullWidth: false,
            savePresets: [],
            defaultTrackSavePreset: "flac",
            defaultReleaseSavePreset: "flac",
            castEnabled: false,
            mcp: BridgeMcpConfig(enabled: false, port: 47_777),
            subsonic: BridgeSubsonicConfig(
                enabled: false,
                port: 4_533,
                username: "",
                bindAddress: "127.0.0.1"
            ),
            discogsTokenStatus: .notConfigured,
            discogsUsable: false,
            sync: nil
        )
    }
}
