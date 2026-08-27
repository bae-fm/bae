import BaeKit
import SwiftUI

/// The mark that says an identifying signal was extracted from this file: the
/// image a barcode was read from, or the log or sheet a disc ID was computed
/// from. It states a fact and does nothing.
///
/// The hover carries the whole sentence, value included — the mark itself is
/// only as wide as its surface allows, and a 96-point thumbnail allows a
/// glyph.
enum ImportEvidence {
    struct Badge: Identifiable {
        let signal: BridgeEvidenceSignal
        var evidence: [BridgeFileEvidence]

        var id: String {
            switch signal {
            case .barcode: "barcode"
            case .discId: "disc-id"
            }
        }
    }

    /// Every extracted value whose source is this file.
    static func of(
        _ fileId: String,
        in evidence: [BridgeFileEvidence]
    ) -> [BridgeFileEvidence] {
        evidence.filter { $0.fileId == fileId }
    }

    /// One badge per signal kind, preserving every extracted value for its
    /// help text. Grouping is presentation-only: core keeps one provenance
    /// record per extracted value.
    static func badges(_ evidence: [BridgeFileEvidence]) -> [Badge] {
        evidence.reduce(into: []) { badges, entry in
            if let index = badges.firstIndex(where: {
                $0.signal == entry.signal
            }) {
                badges[index].evidence.append(entry)
            }
            else {
                badges.append(Badge(signal: entry.signal, evidence: [entry]))
            }
        }
    }

    /// What hovering one signal badge says, in the user's language.
    static func hoverText(_ evidence: BridgeFileEvidence) -> String {
        coreString(bridgeFileEvidenceKey(evidence: evidence), evidence.value)
    }

    /// What hovering a file tile says when it carries several signal kinds.
    static func hoverText(_ evidence: [BridgeFileEvidence]) -> String {
        evidence.map(hoverText).joined(separator: "\n")
    }

    /// The same glyph and wording the signals toolbar uses for the signal.
    static func kind(_ signal: BridgeEvidenceSignal) -> BridgeSignalKind {
        switch signal {
        case .barcode: .barcode
        case .discId: .discId
        }
    }
}

/// The chip itself: the signal's own glyph and name.
///
/// `onImage` is for a thumbnail's corner, where the chip sits on a photograph
/// rather than on the pane — it fills instead of tinting, so it reads against
/// whatever is behind it, and gives up its label before it outgrows the tile.
struct ImportEvidenceChip: View {
    let signal: BridgeEvidenceSignal
    var onImage: Bool = false

    private var fill: AnyShapeStyle {
        onImage
            ? AnyShapeStyle(Color.accentColor)
            : AnyShapeStyle(Color.accentColor.opacity(0.15))
    }

    var body: some View {
        let kind = ImportEvidence.kind(signal)
        HStack(spacing: 3) {
            Image(systemName: SignalBadgeStyle.icon(for: kind))
            Text(SignalBadgeStyle.label(for: kind))
                .lineLimit(1)
                .truncationMode(.tail)
        }
        .font(.caption2.weight(.medium))
        .padding(.horizontal, 5)
        .padding(.vertical, 2)
        .background(fill, in: Capsule())
        .foregroundStyle(onImage ? Color.white : Color.accentColor)
    }
}
