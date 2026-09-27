import BaeKit
import SwiftUI

/// The playback bar's Cast control: a speaker glyph, accent while casting, that
/// opens the device picker.
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
                .themeIcon(.large)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
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
            // Discovery grows the list while open, and NSPopover's animated
            // resize can crash; the popover grows up from the bar.
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

/// The device picker: the casting row while casting, then the discovered
/// devices.
private struct CastPickerPopover: View {
    let devices: [BridgeCastDevice]
    let castingDeviceName: String?
    let onCast: (String) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            if let castingDeviceName {
                castingRow(castingDeviceName)
                Divider()
            }
            if devices.isEmpty {
                Text("No Cast devices found")
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .padding(.vertical, ThemeSpace.compact)
                    .padding(.horizontal, ThemeSpace.inline)
            }
            else {
                ForEach(devices, id: \.id) { device in
                    deviceRow(device)
                }
            }
        }
        .padding(ThemeSpace.group)
        .frame(width: 260)
    }

    private func castingRow(_ name: String) -> some View {
        HStack(spacing: ThemeSpace.inline) {
            Image(systemName: "hifispeaker.fill")
                .foregroundStyle(Theme.accent)
            VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                Text("Casting to \(name)")
                    .themeText(.strong)
                    .lineLimit(1)
            }
            Spacer()
            Button("Disconnect", action: onDisconnect)
                .buttonStyle(.borderless)
                .themeText(.body)
        }
        .padding(.vertical, ThemeSpace.line)
    }

    private func deviceRow(_ device: BridgeCastDevice) -> some View {
        let isActive = device.name == castingDeviceName
        return Button(action: { onCast(device.id) }) {
            HStack(spacing: ThemeSpace.inline) {
                Image(systemName: deviceIcon(device.kind))
                    .foregroundStyle(.secondary)
                Text(device.name)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Spacer()
                if isActive {
                    Image(systemName: "checkmark")
                        .foregroundStyle(Theme.accent)
                }
            }
            .contentShape(Rectangle())
            .padding(.vertical, ThemeSpace.inline)
            .padding(.horizontal, ThemeSpace.inline)
        }
        .buttonStyle(.plain)
    }

    /// The picker row's glyph for the device's protocol.
    private func deviceIcon(_ kind: BridgeRendererKind) -> String {
        switch kind {
        case .cast: "hifispeaker"
        case .dlna: "tv"
        case .airPlay: "airplayaudio"
        }
    }
}
