import BaeKit
import SwiftUI

/// The rows of a receiver list, wherever bae lists them: the one playback is
/// cast to first, then the rest, the receiver of the cast `connecting` saying
/// so on its row.
struct MacCastRows: View {
    let place: MacCastRow.Place
    let receivers: BridgeCastReceiverList
    let connecting: CastStart.Connecting?
    let onCast: (BridgeCastReceiver) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        if let casting = receivers.casting {
            MacCastRow(
                receiver: casting,
                role: .casting(onDisconnect: onDisconnect),
                place: place,
                wait: wait(on: casting)
            )
        }
        ForEach(receivers.found, id: \.id) { receiver in
            MacCastRow(
                receiver: receiver,
                role: .found(onCast: { onCast(receiver) }),
                place: place,
                wait: wait(on: receiver)
            )
        }
    }

    /// What the row of `receiver` waits on, when the cast connecting is to
    /// it.
    private func wait(on receiver: BridgeCastReceiver) -> MacCastRow.Wait? {
        guard let connecting, connecting.receiver.id == receiver.id else {
            return nil
        }
        return connecting.ownerDeciding ? .ownerDeciding : .connecting
    }
}

/// A receiver, wherever bae lists them: the Play on popover and Settings →
/// Playback. Its kind's glyph, its name, then its kind under it; the receiver
/// playing says "Playing" in the accent instead. In the popover it ends in
/// Disconnect; in the Playback settings every row ends in a ⋯ menu holding
/// Details and, on the receiver playing, Disconnect. Any other row plays on
/// its receiver when clicked. While bae connects to the receiver, the row
/// says what it waits on in italic in place of its kind, with a spinner at
/// its end.
struct MacCastRow: View {
    /// Where receivers are listed, which decides how a row ends.
    enum Place {
        /// The Play on popover, opened to pick a receiver or stop.
        case picker
        /// Settings → Playback.
        case page
    }

    /// Where the row's receiver stands in the list, and what the row offers.
    enum Role {
        /// Playback is on it; the row offers Disconnect.
        case casting(onDisconnect: () -> Void)
        /// Discovery found it; clicking the row plays on it.
        case found(onCast: () -> Void)
    }

    /// What bae waits on while it connects to the row's receiver.
    enum Wait {
        /// The receiver to answer.
        case connecting
        /// The receiver's owner to accept bae on it: an AirPlay 2 receiver
        /// has held bae's request for five seconds.
        case ownerDeciding
    }

    let receiver: BridgeCastReceiver
    let role: Role
    let place: Place
    let wait: Wait?

    @State
    private var showsDetails = false

    var body: some View {
        switch role {
        case .casting:
            ownOrder(item)
        case .found(let onCast):
            switch place {
            case .picker:
                Button(action: onCast) {
                    item.contentShape(Rectangle())
                }
                .buttonStyle(.accentPlain)
                // Named as macOS names a row button: its name and its second
                // line, without the spinner of a wait.
                .accessibilityLabel(buttonName)
            case .page:
                // The row holds the ⋯ menu, which a button around the row
                // would take in as part of its own label.
                ownOrder(item)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onCast)
                    .accessibilityAction(named: Text("Play on"), onCast)
            }
        }
    }

    /// Whether the row's elements are read on their own, and not as a
    /// button's label.
    private var readsOwnElements: Bool {
        if case .found = role, place == .picker {
            false
        }
        else {
            true
        }
    }

    /// The row read in its own order: name, kind, then what ends it. The
    /// end sits centred on the two lines, and VoiceOver orders a window's
    /// elements by where they are, so it would read the end between them.
    private func ownOrder(_ row: some View) -> some View {
        row.accessibilityElement(children: .contain)
    }

    private var casting: Bool {
        if case .casting = role {
            true
        }
        else {
            false
        }
    }

    private var item: some View {
        MacItem(centered: true, wide: true) {
            Image(systemName: icon)
                .themeIcon(.medium)
                .foregroundStyle(
                    casting ? Theme.accent : Theme.textSecondary
                )
                .accessibilityHidden(true)
        } content: {
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: receiver.name)
                    .themeText(.rowTitle)
                    .lineLimit(2)
                    .truncationMode(.tail)
                if let wait {
                    Text(verbatim: waitWords(wait))
                        .themeText(.detail)
                        .italic()
                        .foregroundStyle(Theme.textSecondary)
                }
                else if casting {
                    Text(
                        LocalizedStringResource(
                            "cast.receiver.playing",
                            defaultValue: "Playing",
                            comment:
                                "Under the name of the receiver playback is on, in the Play on popover and the Playback settings."
                        )
                    )
                    .themeText(.detail)
                    .foregroundStyle(Theme.accent)
                }
                else {
                    Text(verbatim: receiver.kind.productName)
                        .themeText(.detail)
                        .foregroundStyle(Theme.textSecondary)
                }
            }
            // Read before the row's end in a row read in its own order.
            .accessibilitySortPriority(readsOwnElements ? 1 : 0)
        } trail: {
            if wait != nil {
                MacSpinner()
            }
            switch place {
            case .picker:
                if case .casting(let onDisconnect) = role {
                    MacButton(
                        "Disconnect",
                        kind: .borderless,
                        action: onDisconnect
                    )
                    .small()
                    .accent()
                }
            case .page:
                MacMoreMenu {
                    MacMenuItem("Details…") { showsDetails = true }
                    if case .casting(let onDisconnect) = role {
                        MacMenuItem("Disconnect", action: onDisconnect)
                    }
                }
                .macPopover(isPresented: $showsDetails, arrowEdge: .bottom) {
                    CastReceiverDetails(receiver: receiver)
                }
            }
        }
        .padding(.vertical, rowPadding)
        .padding(.horizontal, rowInset)
    }

    /// The name VoiceOver reads for a row the person clicks to play on: the
    /// receiver's name and its second line, what bae waits on or its kind.
    private var buttonName: Text {
        let line = wait.map(waitWords) ?? receiver.kind.productName
        return Text(
            LocalizedStringResource(
                "cast.receiver.button",
                defaultValue: "\(receiver.name), \(line)",
                comment:
                    "VoiceOver's name for a receiver's row in the Play on popover: the receiver's name, then the line under it (its kind, or what bae waits on while it connects)."
            )
        )
    }

    /// The words of `wait`, under the receiver's name.
    private func waitWords(_ wait: Wait) -> String {
        switch wait {
        case .connecting:
            String(
                localized: LocalizedStringResource(
                    "cast.receiver.connecting",
                    defaultValue: "Connecting…",
                    comment:
                        "Under a receiver's name in the Play on popover and the Playback settings, in place of its kind, while bae connects to it."
                )
            )
        case .ownerDeciding:
            String(
                localized: "Accept on \(receiver.name)…",
                comment:
                    "Under a receiver's name in the Play on popover and the Playback settings, and the Play on button's tooltip, once the receiver has held bae's request for five seconds: its owner may have to accept bae on it. The argument is the receiver's name."
            )
        }
    }

    /// The room beside a row's glyph and Disconnect: the picker's rows sit in
    /// from the popover's padding; a settings box insets its own.
    private var rowInset: CGFloat {
        switch place {
        case .picker: ThemeSpace.group
        case .page: 0
        }
    }

    /// The space over and under a row's two lines: the picker's list gives
    /// none of its own; a settings box's rows already have theirs.
    private var rowPadding: CGFloat {
        switch place {
        case .picker: ThemeSpace.related + ThemeSpace.line
        case .page: 0
        }
    }

    /// The glyph for the receiver's protocol.
    private var icon: String {
        switch receiver.kind {
        case .cast: "hifispeaker"
        case .dlna: "tv"
        case .airPlay: "airplayvideo"
        }
    }
}

extension BridgeRendererKind {
    /// The kind by its product's name, the same in every language.
    var productName: String {
        switch self {
        case .cast: "Chromecast"
        case .airPlay: "AirPlay"
        case .dlna: "UPnP"
        }
    }
}
