import BaeKit
import SwiftUI

/// The mark that says a signal read off this file is what identified the
/// release: the image a barcode was OCR'd from, the log or sheet a disc ID was
/// computed from. It states a fact and does nothing.
///
/// The hover carries the whole sentence, value included — the mark itself is
/// only as wide as its surface allows, and a 96-point thumbnail allows a
/// glyph.
enum ImportEvidence {
    /// The evidence this file is the source of, if it is any.
    static func of(
        _ fileId: String,
        in evidence: [BridgeFileEvidence]
    ) -> BridgeFileEvidence? {
        evidence.first { $0.fileId == fileId }
    }

    /// What hovering the file says, in the user's language.
    static func hoverText(_ evidence: BridgeFileEvidence) -> String {
        coreString(bridgeFileEvidenceKey(evidence: evidence), evidence.value)
    }

    /// The same glyph and wording the signals toolbar uses for the signal.
    static func kind(_ signal: BridgeEvidenceSignal) -> BridgeSignalKind {
        switch signal {
        case .barcode: .barcode
        case .discId: .discId
        }
    }
}

/// The chip, where the row has room for words.
struct ImportEvidenceChip: View {
    let signal: BridgeEvidenceSignal

    var body: some View {
        let kind = ImportEvidence.kind(signal)
        HStack(spacing: 3) {
            Image(systemName: SignalBadgeStyle.icon(for: kind))
            Text(SignalBadgeStyle.label(for: kind))
        }
        .font(.caption2.weight(.medium))
        .padding(.horizontal, 6)
        .padding(.vertical, 2)
        .background(Color.accentColor.opacity(0.15), in: Capsule())
        .foregroundStyle(Color.accentColor)
    }
}

/// The same statement in a thumbnail's corner, where words would not fit.
struct ImportEvidenceMark: View {
    let signal: BridgeEvidenceSignal

    var body: some View {
        let icon = SignalBadgeStyle.icon(for: ImportEvidence.kind(signal))
        Image(systemName: icon)
            .font(.caption2.weight(.medium))
            .foregroundStyle(.white)
            .padding(3)
            .background(
                Color.accentColor,
                in: RoundedRectangle(cornerRadius: 3)
            )
            .padding(2)
    }
}
