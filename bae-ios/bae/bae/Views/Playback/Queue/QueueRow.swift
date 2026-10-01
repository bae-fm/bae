import BaeKit
import SwiftUI

/// One "Up Next" row in the queue sheet and the expanded now-playing view.
struct QueueRow: View {
    let item: BridgeQueueEntry

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            ImageView(imageRef: item.display.coverImage, pointSize: ThemeSize.rowArtwork)
                .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
                .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.artwork))
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(item.display.title)
                    .themeText(.rowTitle)
                    .lineLimit(1)
                Text(item.display.artistNames)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Text(item.display.albumTitle)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if !item.durationLabel.isEmpty {
                Text(item.durationLabel)
                    .monospacedDigit()
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
        }
        .contentShape(Rectangle())
        .padding(.vertical, ThemeSpace.inline)
    }
}

/// A skeleton for a queue row whose item is still loading.
struct QueueRowPlaceholder: View {
    private static let detailBarHeight: CGFloat = 10

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            RoundedRectangle(cornerRadius: ThemeRadius.artwork)
                .fill(Theme.placeholder)
                .frame(width: ThemeSize.rowArtwork, height: ThemeSize.rowArtwork)
            VStack(alignment: .leading, spacing: ThemeSpace.compact) {
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 160, height: 12)
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 100, height: Self.detailBarHeight)
                RoundedRectangle(cornerRadius: ThemeRadius.bar)
                    .fill(Theme.placeholder)
                    .frame(width: 120, height: Self.detailBarHeight)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, ThemeSpace.inline)
    }
}

#if DEBUG
#Preview {
    List {
        QueueRow(item: PreviewData.queueEntries[0])
        QueueRowPlaceholder()
    }
    .previewStores()
}
#endif
