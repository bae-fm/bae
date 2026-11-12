import BaeKit
import SwiftUI

/// The "Casting" settings tab: one toggle that core enforces, with a warning
/// before turning it off would end a session, and, while casting is on, the
/// receivers bae finds and what a track a device can't play directly is
/// converted to.
struct CastingSettingsTab: View {
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore
    @Environment(CastStart.self)
    private var start

    /// The device an unconfirmed "turn casting off" would disconnect from.
    @State
    private var pendingDisconnect: String?

    var body: some View {
        SettingsPane(.casting) {
            Form {
                Section {
                    Toggle("Enable casting", isOn: enabledBinding)
                } footer: {
                    Text(
                        "Scan for Cast, AirPlay, and UPnP receivers on your network."
                    )
                }
                if configStore.config.castEnabled {
                    CastReceiversSection(
                        receivers: castStore.receivers,
                        connectingName: start.connecting?.name,
                        onCast: { receiver in
                            start.cast(to: receiver)
                        },
                        onDisconnect: cast.stopCasting
                    )
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
                            "For tracks a receiver cannot play as they are. WAV keeps full quality but can cut out on weak Wi‑Fi; MP3 holds up better."
                        )
                    }
                }
            }
            .formStyle(.settings)
        }
        // The page asks for as long as it is open: core browses for it only
        // while casting is on, and follows the setting from there.
        .onAppear { cast.startDiscovery(.settingsPage) }
        .onDisappear { cast.stopDiscovery(.settingsPage) }
        .macAlert(item: $pendingDisconnect) { device in
            (
                MacAlertWords(
                    Text("Turn off casting?"),
                    message: Text("This will stop casting to \(device).")
                ),
                [
                    MacAlertButton(Text("Turn off")) { setEnabled(false) },
                    MacAlertButton(Text("Cancel"), role: .cancel),
                ]
            )
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

/// The receivers bae has found, one row each, the one cast to first, or a
/// spinner until the first one answers. Browsing has no end while the page is
/// open, so an empty list is still looking.
private struct CastReceiversSection: View {
    let receivers: BridgeCastReceiverList
    /// The receiver a cast is connecting to, while it does.
    let connectingName: String?
    let onCast: (BridgeCastReceiver) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        Section {
            if let connectingName {
                CastConnectingLine(name: connectingName)
            }
            if receivers.isEmpty {
                MacSpinner("Looking for receivers…")
            }
            else {
                CastReceiverRows(
                    place: .page,
                    receivers: receivers,
                    onCast: onCast,
                    onDisconnect: onDisconnect
                )
            }
        } header: {
            Text("Receivers on your network")
        }
    }
}

#if DEBUG
    #Preview("Casting Settings") {
        CastingSettingsTab()
            .environment(Cast.stub())
            .environment(CastStart.stub())
            .environment(PreviewData.castStore())
            .environment(
                PreviewData.makeConfigStore(
                    libraryFullWidth: false,
                    castEnabled: true
                )
            )
            .environment(UiStore())
            .frame(width: 500, height: 300)
            .windowBackground()
    }
#endif
