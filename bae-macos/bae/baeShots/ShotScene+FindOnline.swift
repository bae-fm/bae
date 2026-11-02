import BaeKit
import SwiftUI

@testable import bae

extension ShotScene {
    /// Find online states whose words the tests leave to the gallery: a row
    /// whose release could not be read, with its failure line and Retry.
    static let findOnlineScenes: [ShotScene] = [
        ShotScene(
            id: "import-search-row-failed",
            size: CGSize(width: 900, height: 160)
        ) {
            AnyView(
                failedRow
                    .frame(width: 900, height: 160, alignment: .top)
                    .importPreviewEnvironment()
                    .background(Theme.background)
            )
        }
    ]

    /// The first exact match, its documents unread.
    @ViewBuilder
    private static var failedRow: some View {
        let offered = PreviewData.searchGroupExact.pressings[0]
        if let pressing = Pressing(
            bridge: BridgePressing(
                releases: offered.releases,
                labels: offered.labels,
                documentFailure: .timeout,
                pick: offered.link
            )
        ) {
            ImportSearchResultRow(
                pressing: pressing,
                isImporting: false,
                libraryStatus: nil,
                isSelected: false,
                onRetryUnread: {},
                onSelect: { _ in }
            )
        }
    }
}
