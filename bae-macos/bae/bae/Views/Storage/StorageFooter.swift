import BaeKit
import Foundation
import SwiftUI

/// Footer under the storage table: the current filter's release count on the
/// left, the full-universe total size on the right.
struct StorageFooter: View {
    let list: StorageList
    /// The core aggregate over every row the current filter matches. `nil`
    /// until fetched — rendered as absence, not a zero/partial stand-in.
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
        .font(.callout)
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

#if DEBUG
    #Preview("With total") {
        StorageFooter(
            list: PreviewData.storageList(store: LibraryStore()),
            totalSize: 857_000_000
        )
        .frame(width: 700)
        .environment(PreviewData.outboxStore())
    }

    #Preview("Total not yet loaded") {
        StorageFooter(
            list: PreviewData.storageList(store: LibraryStore()),
            totalSize: nil
        )
        .frame(width: 700)
        .environment(PreviewData.outboxStore())
    }
#endif
