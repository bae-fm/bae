import SwiftUI

struct PlaybackSettingsTab: View {
    @Environment(AppService.self)
    private var appService
    @Environment(ConfigStore.self)
    private var configStore

    @AppStorage("persistPlayback")
    private var persistPlayback = false

    var body: some View {
        Form {
            Section {
                PauseBetweenSidesToggle(
                    configStore: configStore,
                    appHandle: appService.appHandle,
                    showError: { @MainActor error in
                        appService.uiStore.showError(error)
                    }
                )
                Toggle("Restore on launch", isOn: $persistPlayback)
            } footer: {
                Text(
                    "Saves the current track, position, queue, and volume on quit and restores them on next launch."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
    }
}

#Preview("Playback Settings") {
    let appService = PlaybackSettingsPreviewAppService.make()

    PlaybackSettingsTab()
        .environment(appService)
        .environment(appService.configStore)
        .frame(width: 500, height: 300)
}

@MainActor
private enum PlaybackSettingsPreviewAppService {
    static func make() -> AppService {
        AppService(
            appHandle: PlaybackSettingsPreviewAppHandle(),
            uiStore: UiStore(),
            config: BridgeConfig(
                libraryId: "lib-preview",
                libraryName: "Preview Library",
                libraryPath: "/preview",
                encryptionKeyStored: false,
                encryptionKeyFingerprint: nil,
                pauseBetweenSides: false,
                discogsTokenStatus: .notConfigured,
                discogsUsable: false,
                sync: nil
            )
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

    override func getOutboxSnapshot() throws -> BridgeOutboxSnapshot {
        OutboxStore.emptySnapshot
    }

    override func getDownloadSnapshot() -> BridgeDownloadSnapshot {
        BridgeDownloadSnapshot(
            downloads: [],
            total: BridgeDownloadProgress(queued: 0, active: 0, failed: 0),
            paused: false
        )
    }

    override func setPauseBetweenSides(enabled: Bool) throws {}
}
