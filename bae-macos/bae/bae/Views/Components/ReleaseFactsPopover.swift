import BaeKit
import SwiftUI

/// What a release says about itself: what the rip databases said about its
/// audio, then every catalog that describes it. Either half is absent when
/// there is nothing of it to state, and the hairline between them goes with
/// whichever is missing.
struct ReleaseFactsPopover: View {
    nonisolated static let width: CGFloat = 320

    let verification: BridgeVerification?
    let records: [BridgeReleaseRecord]

    private var statesNames: Bool {
        verification?.matchedCopies != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseFactsScale.card.lineSpacing)
        {
            if let verification, statesNames {
                RipMatchLine(verification: verification)
            }
            if statesNames, !records.isEmpty {
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(height: 1)
            }
            if !records.isEmpty {
                ReleaseRecordsRow(records: records, scale: .card)
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 12)
        .frame(width: Self.width, alignment: .leading)
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Everything the release states") {
        ReleaseFactsPopover(
            verification: PreviewData.releaseVerification,
            records: PreviewData.releaseRecordsPair
        )
        .importPreviewEnvironment()
    }

    #Preview("Nothing read off the folder") {
        ReleaseFactsPopover(
            verification: nil,
            records: PreviewData.releaseRecordsPair
        )
        .importPreviewEnvironment()
    }
#endif
