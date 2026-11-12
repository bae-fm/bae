import BaeKit
import SwiftUI

/// The rows of a receiver list, wherever bae lists them: the one playback is
/// cast to first, then the rest.
struct CastReceiverRows: View {
    let place: CastReceiverRow.Place
    let receivers: BridgeCastReceiverList
    let onCast: (BridgeCastReceiver) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        if let casting = receivers.casting {
            CastReceiverRow(
                receiver: casting,
                role: .casting(onDisconnect: onDisconnect),
                place: place
            )
        }
        ForEach(receivers.found, id: \.id) { receiver in
            CastReceiverRow(
                receiver: receiver,
                role: .found(onCast: { onCast(receiver) }),
                place: place
            )
        }
    }
}

/// A receiver, wherever bae lists them: the cast picker and Settings → Casting.
/// Its kind's glyph, its name, then its kind under it; the receiver playback is
/// cast to says "Casting" in the accent instead. In the picker it ends in
/// Disconnect; on the Casting page every row ends in a ⋯ menu holding Details
/// and, on the receiver cast to, Disconnect. Any other row casts to its
/// receiver when clicked.
struct CastReceiverRow: View {
    /// Where receivers are listed, which decides how a row ends.
    enum Place {
        /// The cast picker, opened to pick a receiver or stop.
        case picker
        /// Settings → Casting.
        case page
    }

    /// Where the row's receiver stands in the list, and what the row offers.
    enum Role {
        /// Playback is cast to it; the row offers Disconnect.
        case casting(onDisconnect: () -> Void)
        /// Discovery found it; clicking the row casts to it.
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
            Button(action: onCast) {
                row.contentShape(Rectangle())
            }
            .buttonStyle(.plain)
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

    private var row: some View {
        RowItem {
            Image(systemName: icon)
                .font(.system(size: 15))
                .foregroundStyle(casting ? Theme.accent : .secondary)
        } content: {
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: receiver.name)
                    .themeText(.rowTitle)
                    .lineLimit(2)
                    .truncationMode(.tail)
                if casting {
                    Text(
                        LocalizedStringResource(
                            "cast.receiver.casting",
                            defaultValue: "Casting",
                            comment:
                                "Under the name of the receiver playback is cast to, in the cast picker and Settings → Casting."
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
        } trail: {
            switch place {
            case .picker:
                if case .casting(let onDisconnect) = role {
                    Button("Disconnect", action: onDisconnect)
                        .buttonStyle(.borderless)
                        .controlSize(.small)
                        .foregroundStyle(.primary)
                        .tint(.primary)
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

/// The line over the receivers while a cast connects, naming the receiver.
struct CastConnectingLine: View {
    let name: String

    var body: some View {
        ProgressView {
            Text(
                "Connecting to \(name)…",
                comment:
                    "Cast picker and Settings → Casting, over the receivers, and the Cast button's tooltip, while bae connects to the device the person picked; the argument is its name."
            )
        }
        .controlSize(.small)
        .themeText(.body)
        .padding(.vertical, ThemeSpace.line)
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
