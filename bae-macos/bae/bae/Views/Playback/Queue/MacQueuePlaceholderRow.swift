import BaeKit
import SwiftUI

/// A skeleton row shown while its queue entry loads.
struct MacQueuePlaceholderRow: View {
    /// The height of a detail line's bar.
    private static let detailBarHeight: CGFloat = 10

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            MacPlaceholderLine(
                width: MacQueueRow.artworkSize,
                height: MacQueueRow.artworkSize,
                radius: ThemeRadius.artwork
            )
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                MacPlaceholderLine(width: 140, height: 12)
                MacPlaceholderLine(width: 90, height: Self.detailBarHeight)
                MacPlaceholderLine(width: 120, height: Self.detailBarHeight)
            }
            Spacer()
        }
        .padding(.horizontal, MacQueueRow.horizontalInset)
        .padding(.vertical, MacQueueRow.verticalInset)
    }
}
