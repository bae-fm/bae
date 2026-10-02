import BaeKit
import SwiftUI

/// The "Casting" settings tab: one toggle that core enforces, with a warning
/// before turning it off would end a session, and, while casting is on, what
/// a track a device can't play directly is converted to.
struct CastingSettingsTab: View {
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore

    /// The device an unconfirmed "turn casting off" would disconnect from.
    @State
    private var pendingDisconnect: String?

    var body: some View {
        Form {
            Section {
                Toggle("Enable casting", isOn: enabledBinding)
            } footer: {
                Text(
                    "Plays to Cast, AirPlay, and UPnP receivers on your network. While off, bae does not look for devices."
                )
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            if CastTranscodeFormatPicker.isShown(for: configStore.config) {
                Section {
                    CastTranscodeFormatPicker(
                        configStore: configStore,
                        setFormat: cast.setTranscodeFormat,
                        showError: { @MainActor error in
                            uiStore.showError(error)
                        }
                    )
                } footer: {
                    Text(
                        "Tracks a Cast or UPnP device can't play directly, such as tracks from a single file + CUE, are converted while they play. WAV is lossless but takes several times the data of MP3. UPnP devices always get MP3, since they aren't required to play WAV."
                    )
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .formStyle(.grouped)
        .alert(
            "Turn off casting?",
            isPresented: confirmingDisconnect,
            presenting: pendingDisconnect
        ) { _ in
            Button("Turn Off", role: .destructive) { setEnabled(false) }
            Button("Cancel", role: .cancel) {}
        } message: { device in
            Text("This will stop casting to \(device).")
        }
    }

    /// Reads the persisted setting; the config subscription moves the switch,
    /// so a refused write leaves it where it was.
    private var enabledBinding: Binding<Bool> {
        Binding(
            get: { configStore.config.castEnabled },
            set: { enabled in
                switch Cast.toggleAction(
                    enabled: enabled,
                    castingDeviceName: castStore.castingDeviceName
                ) {
                case .apply(let enabled): setEnabled(enabled)
                case .confirmDisconnect(let device): pendingDisconnect = device
                }
            }
        )
    }

    private var confirmingDisconnect: Binding<Bool> {
        Binding(
            get: { pendingDisconnect != nil },
            set: { presented in
                if !presented { pendingDisconnect = nil }
            }
        )
    }

    private func setEnabled(_ enabled: Bool) {
        Task {
            do {
                try await cast.setEnabled(enabled)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}

#if DEBUG
    #Preview("Casting Settings") {
        CastingSettingsTab()
            .environment(Cast.stub())
            .environment(PreviewData.castStore())
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    castEnabled: true
                )
            )
            .environment(UiStore())
            .frame(width: 500, height: 300)
    }
#endif
