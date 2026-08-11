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
        let uiStore = UiStore()
        PlaybackSettingsTab()
            .environment(Playback.stub())
            .environment(PreviewData.configStore())
            .environment(uiStore)
            .frame(width: 500, height: 300)
    }
#endif
