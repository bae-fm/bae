import BaeKit
import SwiftUI

/// One "Up Next" row — shared by the queue sheet and the expanded now-playing
/// view's embedded queue.
struct QueueRow: View {
    let item: QueueItem

    var body: some View {
        HStack(spacing: 12) {
            ImageView(imageRef: item.coverImage, pointSize: 44)
                .frame(width: 44, height: 44)
                .clipShape(RoundedRectangle(cornerRadius: 4))
            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .font(.body)
                    .lineLimit(1)
                if !item.albumTitle.isEmpty {
                    Text(item.albumTitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
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

/// A not-yet-loaded row: a skeleton shape, no text — `loadRange` is already in
/// flight for it via the row's `.task(id:)`.
struct QueueRowPlaceholder: View {
    var body: some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 4)
                .fill(.secondary.opacity(0.15))
                .frame(width: 44, height: 44)
            VStack(alignment: .leading, spacing: 6) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 160, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 100, height: 10)
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
