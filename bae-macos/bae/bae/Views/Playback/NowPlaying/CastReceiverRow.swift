import BaeKit
import SwiftUI

/// The rows of a receiver list, wherever bae lists them: the one playback is
/// cast to first, then the rest.
struct CastReceiverRows: View {
    let receivers: BridgeCastReceiverList
    let onCast: (BridgeCastReceiver) -> Void
    let onDisconnect: () -> Void

    var body: some View {
        if let casting = receivers.casting {
            CastReceiverRow(
                receiver: casting,
                role: .casting(onDisconnect: onDisconnect)
            )
        }
        ForEach(receivers.found, id: \.id) { receiver in
            CastReceiverRow(
                receiver: receiver,
                role: .found(onCast: { onCast(receiver) })
            )
        }
    }
}

/// A receiver, wherever bae lists them: the cast picker and Settings → Casting.
/// Its kind's glyph, its name, then its kind under it; the receiver playback is
/// cast to says "Casting" in the accent instead and ends in Disconnect. Any
/// other row casts to its receiver when clicked.
struct CastReceiverRow: View {
    /// Where the row's receiver stands in the list, and what the row offers.
    enum Role {
        /// Playback is cast to it; the row offers Disconnect.
        case casting(onDisconnect: () -> Void)
        /// Discovery found it; clicking the row casts to it.
        case found(onCast: () -> Void)
    }

    let receiver: BridgeCastReceiver
    let role: Role

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
                .foregroundStyle(casting ? Theme.accent : .secondary)
        } content: {
            VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
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
            if case .casting(let onDisconnect) = role {
                Button("Disconnect", action: onDisconnect)
                    .buttonStyle(.borderless)
                    .controlSize(.small)
            }
        }
        .padding(.vertical, ThemeSpace.related + ThemeSpace.line)
    }

    /// The glyph for the receiver's protocol.
    private var icon: String {
        switch receiver.kind {
        case .cast: "hifispeaker"
        case .dlna: "tv"
        case .airPlay: "airplayaudio"
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
