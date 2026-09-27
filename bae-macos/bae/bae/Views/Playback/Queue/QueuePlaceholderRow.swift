import BaeKit
import SwiftUI

/// A skeleton row shown while its queue entry loads.
struct QueuePlaceholderRow: View {
    /// The height of a detail line's bar.
    private static let detailBarHeight: CGFloat = 10

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(Theme.placeholder)
                .frame(
                    width: QueueItemRow.artworkSize,
                    height: QueueItemRow.artworkSize
                )
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 140, height: 12)
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 90, height: Self.detailBarHeight)
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 120, height: Self.detailBarHeight)
            }
            Spacer()
        }
        .padding(.horizontal, QueueItemRow.horizontalInset)
        .padding(.vertical, QueueItemRow.verticalInset)
    }
}

#if DEBUG
    #Preview("Loading row") {
        QueuePlaceholderRow()
            .frame(width: 380)
            .padding()
            .background(Theme.surface)
    }
#endif
