import BaeKit
import SwiftUI

/// The playback bar's Cast control: a speaker glyph that opens a device picker
/// popover. Discovery runs only while the popover is open. Active (accent) while
/// casting, with a "Casting to …" row and a disconnect action.
struct CastButton: View {
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(UiStore.self)
    private var uiStore

    @State
    private var showPicker = false
    @State
    private var castTask: Task<Void, Never>?

    var body: some View {
        let castingName = castStore.castingDeviceName
        let active = castingName != nil
        return Button(action: { showPicker.toggle() }) {
            Image(systemName: active ? "hifispeaker.fill" : "hifispeaker")
                .font(.system(size: 15, weight: .medium))
                .frame(width: 30, height: 30)
                .foregroundStyle(active ? Theme.accent : Color.primary)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle())
        .help(active ? castingHelp(castingName) : LocalizedStringKey("Cast"))
        .accessibilityLabel("Cast")
        .popover(isPresented: $showPicker, arrowEdge: .top) {
            CastPickerPopover(
                devices: castStore.devices,
                castingDeviceName: castingName,
                onCast: castTo,
                onDisconnect: {
                    cast.stopCasting()
                    showPicker = false
                }
            )
            // The device list grows while the popover is open (discovery runs
            // only then), and NSPopover's animated resize spins a nested run
            // loop that can fire a torn-down observer and crash. The popover
            // sits above the bar, so its visual anchor is its bottom edge.
            .popoverEntrance(anchor: .bottom)
            .background { PopoverBehavior() }
        }
        // Discovery is not always-on: browse only while the picker is open.
        .onChange(of: showPicker) { _, isOpen in
            if isOpen {
                cast.startDiscovery()
            }
            else {
                cast.stopDiscovery()
            }
        }
        .onDisappear { castTask?.cancel() }
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
                uiStore.showError(error)
            }
        }
    }

    private func castingHelp(_ name: String?) -> LocalizedStringKey {
        "Casting to \(name ?? "")"
    }
}

/// The device-picker popover content: the active-casting row (when casting), then
/// the discovered devices, or an empty-state line while none are found.
private struct CastPickerPopover: View {
    let devices: [BridgeCastDevice]
    let castingDeviceName: String?
    let onCast: (String) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let castingDeviceName {
                castingRow(castingDeviceName)
                Divider()
            }
            if devices.isEmpty {
                Text("No Cast devices found")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .padding(.vertical, 6)
                    .padding(.horizontal, 4)
            }
            else {
                ForEach(devices, id: \.id) { device in
                    deviceRow(device)
                }
            }
        }
        .padding(10)
        .frame(width: 260)
    }

    private func castingRow(_ name: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "hifispeaker.fill")
                .foregroundStyle(Theme.accent)
            VStack(alignment: .leading, spacing: 1) {
                Text("Casting to \(name)")
                    .font(.system(size: 13, weight: .semibold))
                    .lineLimit(1)
            }
            Spacer()
            Button("Disconnect", action: onDisconnect)
                .buttonStyle(.borderless)
                .font(.system(size: 12, weight: .medium))
        }
        .padding(.vertical, 2)
    }

    private func deviceRow(_ device: BridgeCastDevice) -> some View {
        let isActive = device.name == castingDeviceName
        return Button(action: { onCast(device.id) }) {
            HStack(spacing: 8) {
                Image(systemName: deviceIcon(device.kind))
                    .foregroundStyle(.secondary)
                Text(device.name)
                    .font(.system(size: 13))
                    .lineLimit(1)
                Spacer()
                if isActive {
                    Image(systemName: "checkmark")
                        .foregroundStyle(Theme.accent)
                }
            }
            .contentShape(Rectangle())
            .padding(.vertical, 4)
            .padding(.horizontal, 4)
        }
        .buttonStyle(.plain)
    }

    /// A flavor hint for a picker row: a plain speaker for Cast, a TV glyph for a
    /// UPnP renderer (commonly a TV or AV receiver), and the AirPlay glyph for an
    /// AirPlay receiver. The list itself isn't segregated by protocol — a speaker
    /// is a speaker.
    private func deviceIcon(_ kind: BridgeRendererKind) -> String {
        switch kind {
        case .cast: "hifispeaker"
        case .dlna: "tv"
        case .airPlay: "airplayaudio"
        }
    }
}
