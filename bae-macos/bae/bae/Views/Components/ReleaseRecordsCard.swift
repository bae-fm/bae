import BaeKit
import SwiftUI

/// Every catalog that describes a release, as the card the library expansion's
/// facts line opens.
struct ReleaseRecordsCard: View {
    nonisolated static let width: CGFloat = 320

    let records: [BridgeReleaseRecord]

    var body: some View {
        MacRecordLinks(records: records)
            .padding(ThemeSpace.group)
            .frame(width: Self.width, alignment: .leading)
    }
}
