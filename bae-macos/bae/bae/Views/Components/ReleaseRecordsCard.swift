import BaeKit
import SwiftUI

/// Every catalog that describes a release, as the card the library expansion's
/// facts line opens.
struct ReleaseRecordsCard: View {
    nonisolated static let width: CGFloat = 320

    let records: [BridgeReleaseRecord]

    var body: some View {
        ReleaseRecordsRow(records: records, scale: .card)
            .padding(.vertical, 10)
            .padding(.horizontal, 12)
            .frame(width: Self.width, alignment: .leading)
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("The catalogs that describe the release") {
        ReleaseRecordsCard(records: PreviewData.releaseRecordsPair)
            .importPreviewEnvironment()
    }
#endif
