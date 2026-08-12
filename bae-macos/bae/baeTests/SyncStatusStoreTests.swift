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

    private func status(syncReady: Bool) -> BridgeSyncStatusSnapshot {
        BridgeSyncStatusSnapshot(
            error: nil,
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
            encryptionKeyStored: false,
            encryptionKeyFingerprint: nil,
            pauseBetweenSides: false,
            maxConcurrentUploads: 1,
            maxConcurrentDownloads: 1,
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
