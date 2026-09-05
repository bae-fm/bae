import BaeKit
import SwiftUI

/// A not-yet-loaded queue row: a skeleton shape, no text — `loadRange` is
/// already in flight for it via the row's `.task(id:)`.
struct QueuePlaceholderRow: View {
    var body: some View {
        HStack(spacing: 12) {
            RoundedRectangle(cornerRadius: 8)
                .fill(.secondary.opacity(0.15))
                .frame(width: 48, height: 48)
            VStack(alignment: .leading, spacing: 4) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.15))
                    .frame(width: 140, height: 12)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 90, height: 10)
                RoundedRectangle(cornerRadius: 3)
                    .fill(.secondary.opacity(0.12))
                    .frame(width: 120, height: 10)
            }
            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
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
