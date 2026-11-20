import BaeKit
import SwiftUI

/// The Playback pane: pausing between sides and restoring on launch, then the
/// Play on group. The group holds the switch Play on other devices and, while
/// it is on, the receivers bae finds and what a track a receiver can't play
/// directly is converted to.
struct PlaybackSettingsTab: View {
    @Environment(Playback.self)
    private var playback
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(UiStore.self)
    private var uiStore

    @AppStorage("persistPlayback")
    private var persistPlayback = false

    /// The receiver an unconfirmed "turn Play on other devices off" would
    /// stop playing on.
    @State
    private var pendingDisconnect: String?

    var body: some View {
        SettingsPane(.playback) {
            Form {
                MacSection {
                    PauseBetweenSidesToggle(
                        configStore: configStore,
                        setEnabled: playback.setPauseBetweenSides,
                        showError: { @MainActor error in
                            uiStore.showError(error)
                        }
                    )
                    // Resume automatically belongs to the pause above it.
                    MacNested {
                        SidePauseCountdownPicker(
                            configStore: configStore,
                            setCountdown: playback.setSidePauseCountdown,
                            showError: { @MainActor error in
                                uiStore.showError(error)
                            }
                        )
                    }
                    MacRow("Restore on launch") {
                        MacSwitch("Restore on launch", isOn: $persistPlayback)
                    }
                } footer: {
                    Text(
                        "Restores the last session’s track, position, queue and volume when the app opens."
                    )
                }
                MacSection {
                    MacRow("Play on other devices") {
                        MacSwitch(
                            "Play on other devices",
                            isOn: playOnOtherDevicesBinding
                        )
                    }
                } footer: {
                    Text(
                        "Scan for Cast, AirPlay, and UPnP receivers on your network."
                    )
                }
                if configStore.config.castEnabled {
                    CastReceiversSection()
                }
                if CastTranscodeFormatPicker.isShown(for: configStore.config) {
                    MacSection {
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
        // The pane asks for as long as it is open: core browses for it only
        // while Play on other devices is on, and follows the setting from
        // there.
        .onAppear { cast.startDiscovery(.settingsPage) }
        .onDisappear { cast.stopDiscovery(.settingsPage) }
        .macAlert(item: $pendingDisconnect) { device in
            (
                MacAlertWords(Text("Stop playing on \(device)?")),
                [
                    MacAlertButton(Text("Turn off")) {
                        setPlayOnOtherDevices(false)
                    },
                    MacAlertButton(Text("Cancel"), role: .cancel),
                ]
            )
        }
    }

    /// Reads the persisted setting; the config subscription moves the switch,
    /// so a refused write leaves it where it was.
    private var playOnOtherDevicesBinding: Binding<Bool> {
        Binding(
            get: { configStore.config.castEnabled },
            set: { enabled in
                switch Cast.toggleAction(
                    enabled: enabled,
                    castingDeviceName: castStore.castingDeviceName
                ) {
                case .apply(let enabled): setPlayOnOtherDevices(enabled)
                case .confirmDisconnect(let device): pendingDisconnect = device
                }
            }
        )
    }

    private func setPlayOnOtherDevices(_ enabled: Bool) {
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

/// The receivers bae has found, one row each, the one playing first, or a
/// spinner until the first one answers. Browsing has no end while the pane is
/// open, so an empty list is still looking.
/// It reads the receivers and the connection itself, so a receiver found
/// redraws this section and not the pane around it.
private struct CastReceiversSection: View {
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(CastStart.self)
    private var start

    var body: some View {
        MacSection {
            if castStore.receivers.isEmpty {
                MacSpinner("Looking for receivers…")
            }
            else {
                MacCastRows(
                    place: .page,
                    receivers: castStore.receivers,
                    connecting: start.connecting,
                    onCast: { receiver in start.cast(to: receiver) },
                    onDisconnect: cast.stopCasting
                )
            }
        } header: {
            Text("Receivers on your network")
        }
    }
}
