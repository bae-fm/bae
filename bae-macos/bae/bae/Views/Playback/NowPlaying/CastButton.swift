import BaeKit
import SwiftUI

/// The playback bar's Cast control: a speaker glyph, accent while casting, that
/// opens the device picker.
struct CastButton: View {
    @Environment(Cast.self)
    private var cast
    @Environment(CastStore.self)
    private var castStore
    @Environment(CastStart.self)
    private var start

    @State
    private var showPicker = false

    var body: some View {
        let castingName = castStore.castingDeviceName
        let active = castingName != nil
        let connectingName = start.connecting?.name
        return Button(action: { showPicker.toggle() }) {
            Image(systemName: active ? "hifispeaker.fill" : "hifispeaker")
                .themeIcon(.large)
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .buttonStyle(IconHoverButtonStyle(isOn: active).accentFocus())
        .help(help(casting: castingName, connecting: connectingName))
        .accessibilityLabel("Cast")
        .macPopover(isPresented: $showPicker, arrowEdge: .top) {
            CastPickerPopover(
                receivers: castStore.receivers,
                endedFailure: castStore.endedFailure,
                connectingName: connectingName,
                onDisconnect: {
                    cast.stopCasting()
                    showPicker = false
                }
            )
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
        // picker with it and leaving `showPicker` as it was. A cast still
        // connecting is the app's, not the picker's: core calls it off as
        // casting turns off.
        .onDisappear {
            cast.stopDiscovery(.picker)
        }
    }

    /// The device a cast is connecting to, else the one cast to, else what
    /// the button does.
    private func help(casting: String?, connecting: String?)
        -> LocalizedStringKey
    {
        if let connecting {
            return "Connecting to \(connecting)…"
        }
        if let casting {
            return "Casting to \(casting)"
        }
        return "Cast"
    }
}

/// The device picker: why the last device stopped when its link was lost, the
/// receiver a cast is connecting to, then each receiver once, the one cast to
/// first with Disconnect.
struct CastPickerPopover: View {
    let receivers: BridgeCastReceiverList
    let endedFailure: DisplayError?
    /// The receiver a cast is connecting to, while it does.
    let connectingName: String?
    let onDisconnect: () -> Void

    @Environment(CastStart.self)
    private var start
    @Environment(\.dismiss)
    private var dismiss
    /// The start this popover began. It closes when that one connects, not
    /// when one an earlier popover began does.
    @State
    private var asked: CastStart.Attempt?

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
                CastReceiverRows(
                    place: .picker,
                    receivers: receivers,
                    onCast: { receiver in asked = start.cast(to: receiver) },
                    onDisconnect: onDisconnect
                )
            }
        }
        .padding(ThemeSpace.group)
        .frame(width: 260)
        .onChange(of: start.started) {
            if start.didStart(asked) {
                dismiss()
            }
        }
    }
}
