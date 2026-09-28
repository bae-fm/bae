import BaeKit
import SwiftUI

/// Badges and hover text marking a file as the source of an identifying
/// signal: the image a barcode was read from, or the log or sheet a disc ID
/// was computed from.
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

    /// One badge per signal kind, keeping every value for its hover text.
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

    /// The same glyph and wording the Find online ledger uses for the signal.
    static func kind(_ signal: BridgeEvidenceSignal) -> BridgeSignalKind {
        switch signal {
        case .barcode: .barcode
        case .discId: .discId
        }
    }
}

/// The chip: the signal's glyph and name. `onImage` fills it solid with the
/// accent so it reads on a thumbnail.
struct ImportEvidenceChip: View {
    let signal: BridgeEvidenceSignal
    var onImage: Bool = false

    var body: some View {
        let kind = ImportEvidence.kind(signal)
        let label = SignalBadgeStyle.label(for: kind)
        let symbol = SignalBadgeStyle.icon(for: kind)
        if onImage {
            HStack(spacing: ThemeSpace.inline) {
                Image(systemName: symbol)
                    .themeIcon(.badge)
                Text(label)
            }
            .themeText(.chip)
            .lineLimit(1)
            .foregroundStyle(Theme.onFill)
            .padding(.horizontal, ThemeSpace.compact)
            .padding(.vertical, ThemeSpace.line)
            .background(
                StatusTone.accent.color,
                in: RoundedRectangle(cornerRadius: ThemeRadius.chip)
            )
        }
        else {
            StatusChip(verbatim: label, tone: .accent, symbol: symbol)
        }
    }
}
