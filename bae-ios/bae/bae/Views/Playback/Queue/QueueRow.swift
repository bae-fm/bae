import BaeKit
import SwiftUI

/// One "Up Next" row in the queue sheet and the expanded now-playing view.
struct QueueRow: View {
    let item: QueueItem

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: item.coverImage, pointSize: 56)
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.body)
                    .lineLimit(1)
                Text(item.artistNames)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Text(item.albumTitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if !item.durationLabel.isEmpty {
                Text(item.durationLabel)
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
        }
        .contentShape(Rectangle())
        .padding(.vertical, 4)
    }
}

/// A skeleton for a queue row whose item is still loading.
struct QueueRowPlaceholder: View {
    var body: some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 4)
                .fill(Theme.placeholder)
                .frame(width: 56, height: 56)
            VStack(alignment: .leading, spacing: 6) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(Theme.placeholder)
                    .frame(width: 160, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(Theme.placeholder)
                    .frame(width: 100, height: 10)
                RoundedRectangle(cornerRadius: 3)
                    .fill(Theme.placeholder)
                    .frame(width: 120, height: 10)
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
    }
}

#if DEBUG
#Preview {
    List {
        QueueRow(item: PreviewData.queueItem)
        QueueRowPlaceholder()
    }
    .previewStores()
}
#endif
