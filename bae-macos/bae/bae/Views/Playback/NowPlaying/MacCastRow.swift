import BaeKit
import SwiftUI

/// The rows of a receiver list, wherever bae lists them: the one playback is
/// cast to first, then the rest.
struct MacCastRows: View {
    let place: MacCastRow.Place
    let receivers: BridgeCastReceiverList
    let onCast: (BridgeCastReceiver) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        if let casting = receivers.casting {
            MacCastRow(
                receiver: casting,
                role: .casting(onDisconnect: onDisconnect),
                place: place
            )
        }
        ForEach(receivers.found, id: \.id) { receiver in
            MacCastRow(
                receiver: receiver,
                role: .found(onCast: { onCast(receiver) }),
                place: place
            )
        }
    }
}

/// A receiver, wherever bae lists them: the Play on popover and Settings →
/// Playback. Its kind's glyph, its name, then its kind under it; the receiver
/// playing says "Playing" in the accent instead. In the popover it ends in
/// Disconnect; in the Playback settings every row ends in a ⋯ menu holding
/// Details and, on the receiver playing, Disconnect. Any other row plays on
/// its receiver when clicked.
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

    let receiver: BridgeCastReceiver
    let role: Role
    let place: Place

    @State
    private var showsDetails = false

    var body: some View {
        switch role {
        case .casting:
            row
        case .found(let onCast):
            switch place {
            case .picker:
                Button(action: onCast) {
                    row.contentShape(Rectangle())
                }
                .buttonStyle(.accentPlain)
            case .page:
                // The row holds the ⋯ menu, which a button around the row
                // would take in as part of its own label.
                row
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onCast)
                    .accessibilityAction(named: Text("Play on"), onCast)
            }
        }
    }

    private var casting: Bool {
        if case .casting = role {
            true
        }
        else {
            false
        }
    }

    @ViewBuilder
    private var row: some View {
        switch place {
        case .picker:
            item
        case .page:
            // The ⋯ sits centred on the row's two lines, and VoiceOver
            // orders a window's elements by where they are, so it would read
            // the ⋯ between them; the row keeps its own order instead.
            item.accessibilityElement(children: .contain)
        }
    }

    private var item: some View {
        MacItem {
            Image(systemName: icon)
                .font(.system(size: 15))
                .foregroundStyle(casting ? Theme.accent : .secondary)
                .accessibilityHidden(true)
        } content: {
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: receiver.name)
                    .themeText(.rowTitle)
                    .lineLimit(2)
                    .truncationMode(.tail)
                if casting {
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
                        .foregroundStyle(.secondary)
                }
            }
            // Read before the ⋯ in a row that orders its own elements.
            .accessibilitySortPriority(place == .page ? 1 : 0)
        } trail: {
            switch place {
            case .picker:
                if case .casting(let onDisconnect) = role {
                    MacButton(
                        "Disconnect",
                        kind: .borderless,
                        action: onDisconnect
                    )
                    .neutral()
                    .small()
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
