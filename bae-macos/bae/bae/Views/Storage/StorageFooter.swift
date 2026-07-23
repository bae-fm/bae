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

    var body: some View {
        HStack {
            Text("\(list.totalCount) releases")
                .foregroundStyle(.secondary)
            Spacer()
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
