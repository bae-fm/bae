import BaeKit
import SwiftUI

/// A source label and the request that opens its evidence.
struct EvidenceChip: View {
    let label: String
    let selection: BridgeEvidenceSelection

    @Environment(\.openReleaseEvidence)
    private var openEvidence
    @Environment(\.releaseEvidenceSubject)
    private var subject

    var body: some View {
        Button {
            guard let openEvidence, let subject else {
                preconditionFailure(
                    "Evidence chips require a reader and release context"
                )
            }
            openEvidence(subject, selection)
        } label: {
            Text(label)
                .font(.system(size: 9.5))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 5)
                .padding(.vertical, 1)
                .background(
                    Color.primary.opacity(0.07),
                    in: RoundedRectangle(cornerRadius: 4)
                )
                .fixedSize()
        }
        .buttonStyle(.plain)
    }
}
