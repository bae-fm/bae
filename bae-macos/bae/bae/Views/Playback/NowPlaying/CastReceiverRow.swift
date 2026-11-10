import BaeKit
import SwiftUI

/// A receiver, wherever bae lists them: the cast picker and Settings → Casting.
/// Its kind's glyph, its name, then its kind under it; the receiver playback is
/// cast to says "Casting" in the accent instead and ends in Disconnect. Any
/// other row casts to its receiver when clicked.
struct CastReceiverRow: View {
    let receiver: BridgeCastReceiver
    let onCast: () -> Void
    let onDisconnect: () -> Void

    var body: some View {
        if receiver.casting {
            row
        }
        else {
            Button(action: onCast) {
                row.contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    private var row: some View {
        RowItem {
            Image(systemName: icon)
                .foregroundStyle(receiver.casting ? Theme.accent : .secondary)
        } content: {
            VStack(alignment: .leading, spacing: ThemeSpace.hairline) {
                Text(verbatim: receiver.name)
                    .themeText(.rowTitle)
                    .lineLimit(2)
                    .truncationMode(.tail)
                if receiver.casting {
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
            if receiver.casting {
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
