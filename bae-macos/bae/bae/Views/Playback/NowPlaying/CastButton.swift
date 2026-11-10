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
    private var start = CastStart()

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
                endedFailure: castStore.endedFailure,
                connectingName: start.connecting?.name,
                onCast: { receiver in
                    start.cast(
                        to: receiver,
                        with: cast,
                        onStarted: { showPicker = false },
                        onFailure: uiStore.showError
                    )
                },
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
            start.cancel()
            cast.stopDiscovery(.picker)
        }
    }

    private func castingHelp(_ name: String?) -> LocalizedStringKey {
        "Casting to \(name ?? "")"
    }
}

/// The device picker: why the last device stopped when its link was lost, the
/// receiver a cast is connecting to, then each receiver once, the one cast to
/// first with Disconnect.
struct CastPickerPopover: View {
    let receivers: [BridgeCastReceiver]
    let endedFailure: DisplayError?
    /// The receiver a cast is connecting to, while it does.
    let connectingName: String?
    let onCast: (BridgeCastReceiver) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            if let connectingName {
                CastConnectingLine(name: connectingName)
                Divider()
            }
            if let endedFailure {
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
                    CastReceiverRow(
                        receiver: receiver,
                        onCast: { onCast(receiver) },
                        onDisconnect: onDisconnect
                    )
                }
            }
        }
        .padding(ThemeSpace.group)
        .frame(width: 260)
    }
}
