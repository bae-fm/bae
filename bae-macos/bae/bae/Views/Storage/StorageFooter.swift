import BaeKit
import Foundation
import SwiftUI

/// Footer under the storage table: the current filter's release count on the
/// left, the full-universe total size on the right.
struct StorageFooter: View {
    let list: StorageList
    /// The size of every row the current filter matches; nil, and not shown,
    /// until fetched.
    let totalSize: UInt64?
    @Environment(OutboxStore.self)
    private var outboxStore

    var body: some View {
        HStack {
            Text("\(list.totalCount) releases")
                .foregroundStyle(.secondary)
            Spacer()
            if let throughputText = outboxStore.snapshot.throughputText {
                Text(throughputText)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            if let totalSize {
                Text(
                    "Total: \(ByteCountFormatter.string(fromByteCount: Int64(totalSize), countStyle: .file))"
                )
                .foregroundStyle(.secondary)
            }
        }
        .themeText(.body)
        .padding(.horizontal)
        .padding(.vertical, ThemeSpace.related)
    }
}

#if DEBUG
    /// The storage table's width the previews draw in.
    private let previewWidth: CGFloat = 700

    #Preview("With total") {
        StorageFooter(
            list: PreviewData.storageList(store: LibraryStore()),
            totalSize: 857_000_000
        )
        .frame(width: previewWidth)
        .environment(PreviewData.outboxStore())
    }

    #Preview("Total not yet loaded") {
        StorageFooter(
            list: PreviewData.storageList(store: LibraryStore()),
            totalSize: nil
        )
        .frame(width: previewWidth)
        .environment(PreviewData.outboxStore())
    }
#endif
