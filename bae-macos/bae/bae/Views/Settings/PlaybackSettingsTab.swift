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
        SettingsPane(.playback) {
            Form {
                Section {
                    PauseBetweenSidesToggle(
                        configStore: configStore,
                        setEnabled: playback.setPauseBetweenSides,
                        showError: { @MainActor error in
                            uiStore.showError(error)
                        }
                    )
                    // Resume automatically belongs to the pause above it.
                    SidePauseCountdownPicker(
                        configStore: configStore,
                        setCountdown: playback.setSidePauseCountdown,
                        showError: { @MainActor error in
                            uiStore.showError(error)
                        }
                    )
                    .joiningRowAbove()
                    Toggle("Restore on launch", isOn: $persistPlayback)
                } footer: {
                    Text(
                        "Restores the last session’s track, position, queue and volume when the app opens."
                    )
                }
            }
            .formStyle(.settings)
        }
    }
}

#if DEBUG
    #Preview("Playback Settings") {
        let uiStore = UiStore()
        PlaybackSettingsTab()
            .environment(Playback.stub())
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    pauseBetweenSides: true
                )
            )
            .environment(uiStore)
            .frame(width: 500, height: 300)
            .windowBackground()
    }
#endif
