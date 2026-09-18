import BaeKit
import SwiftUI

/// What a release says about itself: the names read off the object, what the
/// rip databases said about its audio, then every catalog that describes it.
/// Either half is absent when there is nothing of it to state.
///
/// One card wherever the question is asked — the candidate row's glyphs and
/// the library expansion's facts line — because it is one answer.
struct ReleaseFactsPopover: View {
    let marks: [BridgeReleaseMark]
    let verification: BridgeVerification?
    let records: [BridgeReleaseRecord]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if !marks.isEmpty || verification != nil {
                VStack(alignment: .leading, spacing: 4) {
                    MarkLines(marks: marks)
                    if let verification {
                        RipMatchLine(verification: verification)
                    }
                }
            }
            if !records.isEmpty {
                ReleaseRecordsRow(records: records)
            }
        }
        .padding(.vertical, 10)
        .padding(.horizontal, 12)
        .frame(width: 300)
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
