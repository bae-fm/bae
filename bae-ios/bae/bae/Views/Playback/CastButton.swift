import BaeKit
import SwiftUI

/// The now-playing bar's Cast control: a speaker glyph that opens the device
/// picker. Browsing runs only while the picker is up. Active (accent) while
/// casting. Absent entirely when casting is turned off — core browses nothing
/// then and refuses a session, so there is nothing for the control to do.
struct CastButton: View {
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(RendererBrowser.self)
    private var renderers

    @State
    private var showPicker = false
    @State
    private var castTask: Task<Void, Never>?

    var body: some View {
        if configStore.config.castEnabled {
            let castingName = castStore.castingDeviceName
            Button {
                showPicker = true
            } label: {
                Image(
                    systemName: castingName == nil
                        ? "hifispeaker" : "hifispeaker.fill"
                )
                .foregroundStyle(
                    castingName == nil ? Color.primary : Theme.accent
                )
            }
            .accessibilityLabel("Cast")
            .sheet(isPresented: $showPicker) {
                CastPickerView(
                    devices: castStore.devices,
                    castingDeviceName: castingName,
                    onCast: castTo,
                    onDisconnect: {
                        cast.stopCasting()
                        showPicker = false
                    }
                )
                .presentationDetents([.medium, .large])
            }
            // Browsing is not always-on: it runs with the picker. Core clears
            // the list as it starts, so the sheet opens on what this browse
            // finds, not what the last one did.
            .onChange(of: showPicker) { _, isOpen in
                if isOpen {
                    cast.startDiscovery()
                    renderers.start()
                }
                else {
                    renderers.stop()
                    cast.stopDiscovery()
                }
            }
            .onDisappear { castTask?.cancel() }
        }
    }

    private func castTo(_ deviceId: String) {
        castTask?.cancel()
        let cast = cast
        castTask = Task {
            do {
                try await cast.castTo(deviceId)
                showPicker = false
            } catch is CancellationError {
                return
            } catch {
                configStore.showError(error)
            }
        }
    }
}

/// The device picker: the active-casting row when casting, then the discovered
/// devices, or an empty-state line while none have answered.
private struct CastPickerView: View {
    let devices: [BridgeCastDevice]
    let castingDeviceName: String?
    let onCast: (String) -> Void
    let onDisconnect: () -> Void

    @Environment(\.dismiss)
    private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if let castingDeviceName {
                    Section {
                        castingRow(castingDeviceName)
                    }
                }
                Section {
                    if devices.isEmpty {
                        Text("No Cast devices found")
                            .foregroundStyle(.secondary)
                    }
                    else {
                        ForEach(devices, id: \.id) { device in
                            deviceRow(device)
                        }
                    }
                }
            }
            .navigationTitle("Cast")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }

    private func castingRow(_ name: String) -> some View {
        HStack {
            Image(systemName: "hifispeaker.fill")
                .foregroundStyle(Theme.accent)
            Text("Casting to \(name)")
                .lineLimit(1)
            Spacer(minLength: 8)
            Button("Disconnect", action: onDisconnect)
                .buttonStyle(.borderless)
        }
    }

    private func deviceRow(_ device: BridgeCastDevice) -> some View {
        Button {
            onCast(device.id)
        } label: {
            HStack {
                Image(systemName: Self.deviceIcon(device.kind))
                    .foregroundStyle(.secondary)
                Text(device.name)
                    .lineLimit(1)
                Spacer(minLength: 8)
                Image(systemName: "checkmark")
                    .foregroundStyle(Theme.accent)
                    .opacity(device.name == castingDeviceName ? 1 : 0)
            }
            .contentShape(Rectangle())
        }
        .foregroundStyle(.primary)
    }

    /// A flavor hint for a row: a speaker for Cast, the AirPlay glyph for an
    /// AirPlay receiver, a TV for a UPnP renderer (commonly a TV or AV
    /// receiver). The list itself isn't segregated by protocol — a speaker is a
    /// speaker. UPnP is found over SSDP, which iOS does not let bae send, so
    /// that row only appears on a platform that browses for itself.
    private static func deviceIcon(_ kind: BridgeRendererKind) -> String {
        switch kind {
        case .cast: "hifispeaker"
        case .dlna: "tv"
        case .airPlay: "airplayaudio"
        }
    }
}
