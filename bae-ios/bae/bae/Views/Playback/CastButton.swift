import BaeKit
import SwiftUI

/// The now-playing bar's Cast control, which opens the device picker and is
/// absent while casting is turned off.
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
                    castingDevice: castStore.castingDevice,
                    onCast: castTo,
                    onDisconnect: {
                        cast.stopCasting()
                        showPicker = false
                    }
                )
                .presentationDetents([.medium, .large])
            }
            // Browsing runs only while the picker is up; core clears the list as
            // it starts, so the sheet shows only this browse's devices.
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
            }
            catch is CancellationError {
                return
            }
            catch {
                configStore.showError(error)
            }
        }
    }
}

/// The device picker: the active-casting row when casting, then the discovered
/// devices, or an empty-state line while none have answered.
private struct CastPickerView: View {
    let devices: [BridgeCastDevice]
    let castingDevice: BridgeRemoteDevice?
    let onCast: (String) -> Void
    let onDisconnect: () -> Void

    @Environment(\.dismiss)
    private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if let castingDevice {
                    Section {
                        castingRow(castingDevice.name)
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
            Spacer(minLength: ThemeSpace.related)
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
                Spacer(minLength: ThemeSpace.related)
                Image(systemName: "checkmark")
                    .foregroundStyle(Theme.accent)
                    .opacity(device.id == castingDevice?.id ? 1 : 0)
            }
            .contentShape(Rectangle())
        }
        .foregroundStyle(.primary)
    }

    /// The row's glyph for the device's protocol; iOS cannot send SSDP, so UPnP
    /// rows appear only on platforms that browse for themselves.
    private static func deviceIcon(_ kind: BridgeRendererKind) -> String {
        switch kind {
        case .cast: "hifispeaker"
        case .dlna: "tv"
        case .airPlay: "airplayaudio"
        }
    }
}
