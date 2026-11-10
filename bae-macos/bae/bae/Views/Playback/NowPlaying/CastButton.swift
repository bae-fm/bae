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
    /// The receiver a cast is connecting to, while it does.
    @State
    private var connecting: BridgeCastReceiver?

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
                receivers: castStore.receivers,
                castingDevice: castStore.castingDevice,
                endedFailure: castStore.endedFailure,
                connectingName: connecting?.name,
                onCast: castTo,
                onDisconnect: {
                    cast.stopCasting()
                    showPicker = false
                }
            )
            // Discovery grows the list while open, and NSPopover's animated
            // resize can crash, so `PopoverAnimation` keeps it from
            // animating; the popover grows up from the bar.
            .popoverEntrance(anchor: .bottom)
        }
        // Discovery is not always-on: browse only while the picker is open.
        .onChange(of: showPicker) { _, isOpen in
            if isOpen {
                cast.startDiscovery(.picker)
            }
            else {
                cast.stopDiscovery(.picker)
            }
        }
        // The bar drops this control when casting turns off, taking an open
        // picker with it and leaving `showPicker` as it was.
        .onDisappear {
            castTask?.cancel()
            cast.stopDiscovery(.picker)
        }
    }

    private func castTo(_ receiver: BridgeCastReceiver) {
        castTask?.cancel()
        let cast = cast
        connecting = receiver
        castTask = Task {
            do {
                try await cast.castTo(receiver.id)
                connecting = nil
                showPicker = false
            }
            catch is CancellationError {
                return
            }
            catch {
                connecting = nil
                uiStore.showError(error)
            }
        }
    }

    private func castingHelp(_ name: String?) -> LocalizedStringKey {
        "Casting to \(name ?? "")"
    }
}

/// The device picker: the casting row while casting, or why the last device
/// stopped when its link was lost, the device a cast is connecting to, then
/// the receivers, the one cast to first and checked.
struct CastPickerPopover: View {
    let receivers: [BridgeCastReceiver]
    let castingDevice: BridgeRemoteDevice?
    let endedFailure: DisplayError?
    /// The device a cast is connecting to, while it does.
    let connectingName: String?
    let onCast: (BridgeCastReceiver) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            if let connectingName {
                ProgressView {
                    Text(
                        "Connecting to \(connectingName)…",
                        comment:
                            "Cast picker, while bae connects to the device the person picked; the argument is its name."
                    )
                }
                .controlSize(.small)
                .themeText(.body)
                .padding(.vertical, ThemeSpace.line)
                Divider()
            }
            if let castingDevice {
                castingRow(castingDevice.name)
                Divider()
            }
            else if let endedFailure {
                Notice(error: endedFailure, tone: .warning)
                    .padding(.vertical, ThemeSpace.line)
                Divider()
            }
            if receivers.isEmpty {
                Text("No Cast devices found")
                    .themeText(.body)
                    .foregroundStyle(.secondary)
                    .padding(.vertical, ThemeSpace.compact)
                    .padding(.horizontal, ThemeSpace.inline)
            }
            else {
                ForEach(receivers, id: \.id) { receiver in
                    receiverRow(receiver)
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

    private func receiverRow(_ receiver: BridgeCastReceiver) -> some View {
        Button(action: { onCast(receiver) }) {
            HStack(spacing: ThemeSpace.inline) {
                Image(systemName: deviceIcon(receiver.kind))
                    .foregroundStyle(.secondary)
                Text(receiver.name)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Spacer()
                if receiver.casting {
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
