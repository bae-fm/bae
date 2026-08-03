import BaeKit
import SwiftUI

struct PlaybackSettingsTab: View {
    @Environment(Playback.self)
    private var playback
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore

    @AppStorage("persistPlayback")
    private var persistPlayback = false

    var body: some View {
        Form {
            Section {
                PauseBetweenSidesToggle(
                    configStore: configStore,
                    setEnabled: playback.setPauseBetweenSides,
                    showError: { @MainActor error in
                        uiStore.showError(error)
                    }
                )
                Toggle("Restore on launch", isOn: $persistPlayback)
            } footer: {
                Text(
                    "Restores the last session's track, position, queue, and volume when the app opens."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
    }
}

#if DEBUG
    #Preview("Playback Settings") {
        let appService = PlaybackSettingsPreviewAppService.make()

        PlaybackSettingsTab()
            .environment(appService.playback)
            .environment(appService.configStore)
            .environment(appService.uiStore)
            .frame(width: 500, height: 300)
    }

    @MainActor
    private enum PlaybackSettingsPreviewAppService {
        static func make() -> AppService {
            AppService(
                appHandle: PlaybackSettingsPreviewAppHandle(),
                mediaControlService: MediaControlService(),
                diagnostics: configureDiagnostics(config: .disabled),
                uiStore: UiStore(),
                config: BridgeConfig(
                    libraryId: "lib-preview",
                    libraryName: "Preview Library",
                    libraryPath: "/preview",
                    encryptionKeyStored: false,
                    encryptionKeyFingerprint: nil,
                    pauseBetweenSides: false,
                    maxConcurrentUploads: 3,
                    maxConcurrentDownloads: 3,
                    showRemainingTime: false,
                    libraryFullWidth: false,
                    savePresets: PreviewData.savePresets,
                    defaultTrackSavePreset: "flac",
                    defaultReleaseSavePreset: "flac",
                    castEnabled: false,
                    mcp: BridgeMcpConfig(enabled: false, port: 47777),
                    subsonic: BridgeSubsonicConfig(
                        enabled: false,
                        port: 4533,
                        username: "",
                        bindAddress: "127.0.0.1"
                    ),
                    discogsTokenStatus: .notConfigured,
                    discogsUsable: false,
                    sync: nil
                ),
                initialOutbox: OutboxStore.emptySnapshot
            )
        }
    }

    private final class PlaybackSettingsPreviewAppHandle: AppHandle,
        @unchecked Sendable
    {
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

        override func setPauseBetweenSides(enabled: Bool) throws {}
    }
#endif
