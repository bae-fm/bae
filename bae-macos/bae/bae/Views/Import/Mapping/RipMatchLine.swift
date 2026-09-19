import BaeKit
import SwiftUI

/// What the rip databases said about the release's audio: how many other
/// people's copies of the same disc carry the same bits.
///
/// Core folds the tracks into that one number — the weakest track's best
/// database — so this draws what it is given. A release whose every track no
/// database confirmed has no number and no line.
struct RipMatchLine: View {
    let verification: BridgeVerification

    var body: some View {
        if let matchedCopies = verification.matchedCopies {
            HStack(alignment: .center, spacing: 7) {
                Image(systemName: "checkmark")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color.green)
                    .accessibilityLabel(coreString("core.identity.verified"))
                Text(
                    coreString(
                        "core.verification.matches_other_rips",
                        Int(matchedCopies)
                    )
                )
                .font(.system(size: 11.5))
                .foregroundStyle(.primary)
                EvidenceChip(
                    label: coreString(bridgeSignalOriginKey(origin: .discToc)),
                    selection: .verification
                )
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("rip-match")
        }
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Matched") {
        RipMatchLine(verification: PreviewData.releaseVerification)
            .padding()
            .frame(width: 420)
            .background(Theme.surfaceElevated)
            .releaseEvidencePreviewEnvironment(
                subject: .candidate(key: PreviewData.mappingCandidate.key)
            )
    }

    #Preview("One other rip") {
        RipMatchLine(
            verification: BridgeVerification(
                source: .log,
                matchedCopies: 1,
                tracks: []
            )
        )
        .padding()
        .frame(width: 420)
        .background(Theme.surfaceElevated)
        .releaseEvidencePreviewEnvironment(
            subject: .candidate(key: PreviewData.mappingCandidate.key)
        )
    }

    #Preview("A track nothing confirmed") {
        RipMatchLine(
            verification: BridgeVerification(
                source: .log,
                matchedCopies: nil,
                tracks: []
            )
        )
        .padding()
        .frame(width: 420)
        .background(Theme.surfaceElevated)
        .releaseEvidencePreviewEnvironment(
            subject: .candidate(key: PreviewData.mappingCandidate.key)
        )
    }
#endif
