import BaeKit
import SwiftUI

/// What an import records, stated as a sentence: the pressing you claim to
/// physically hold, with a muted note saying what identified it.
struct ImportClaimLine: View {
    let claim: BridgeClaimLine

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text(claimSentence)
                    .font(.system(size: 12.5, weight: .medium))
                Spacer(minLength: 8)
                Text(evidenceNote)
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(Theme.field, in: RoundedRectangle(cornerRadius: 6))
    }

    /// "You have this pressing — CD · 2004 · UK · CAT-1234".
    private var claimSentence: String {
        guard let release = claim.release else {
            return coreString("ui.import.claim.pressing_undescribed")
        }
        return coreString("ui.import.claim.pressing", release)
    }

    private var evidenceNote: String {
        switch claim.evidence {
        case .discIdAlone:
            coreString("ui.import.claim.evidence.disc_id")
        case .discIdShared(let matchCount):
            coreString(
                "ui.import.claim.evidence.disc_id_shared",
                Int(matchCount)
            )
        case .barcode:
            coreString("ui.import.claim.evidence.barcode")
        case .search:
            coreString("ui.import.claim.evidence.search")
        }
    }
}

/// The claim sentences live in the shared `ui.*` catalog because both desktop
/// surfaces render the same words from the same `BridgeClaimLine`.
#if DEBUG
    // MARK: - Previews

    #Preview("Claim line") {
        VStack(alignment: .leading, spacing: 12) {
            ImportClaimLine(
                claim: BridgeClaimLine(
                    choice: .release(
                        releaseId: "rel-1",
                        source: .musicBrainz
                    ),
                    evidence: .discIdAlone,
                    release: "CD · 2004 · UK · CAT-1234",
                    trackCount: 11
                )
            )
            ImportClaimLine(
                claim: BridgeClaimLine(
                    choice: .release(
                        releaseId: "rel-1",
                        source: .musicBrainz
                    ),
                    evidence: .discIdShared(matchCount: 2),
                    release: "CD · 2004 · UK",
                    trackCount: 11
                )
            )
            ImportClaimLine(
                claim: BridgeClaimLine(
                    choice: .release(
                        releaseId: "rel-2",
                        source: .musicBrainz
                    ),
                    evidence: .search,
                    release: "CD · 2015 · US",
                    trackCount: 14
                )
            )
        }
        .padding()
        .frame(width: 520)
        .windowBackground()
    }
#endif
