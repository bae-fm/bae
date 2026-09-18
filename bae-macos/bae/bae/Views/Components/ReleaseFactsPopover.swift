import BaeKit
import SwiftUI

/// What a release says about itself: the names read off the object, what the
/// rip databases said about its audio, then every catalog that describes it.
/// Either half is absent when there is nothing of it to state, and the
/// hairline between them goes with whichever is missing.
///
/// One card wherever the question is asked — the candidate row's glyphs and
/// the library expansion's facts line — because it is one answer.
struct ReleaseFactsPopover: View {
    nonisolated static let width: CGFloat = 320

    let marks: [BridgeReleaseMark]
    let verification: BridgeVerification?
    let records: [BridgeReleaseRecord]

    private var statesNames: Bool {
        !marks.isEmpty || verification?.matchedCopies != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ReleaseFactsScale.card.lineSpacing)
        {
            if statesNames {
                MarkLines(marks: marks, scale: .card)
                if let verification {
                    RipMatchLine(verification: verification)
                }
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
            marks: PreviewData.releaseMarks,
            verification: PreviewData.releaseVerification,
            records: PreviewData.releaseRecordsPair
        )
        .importPreviewEnvironment()
    }

    #Preview("Nothing read off the folder") {
        ReleaseFactsPopover(
            marks: [],
            verification: nil,
            records: PreviewData.releaseRecordsPair
        )
        .importPreviewEnvironment()
    }
#endif
