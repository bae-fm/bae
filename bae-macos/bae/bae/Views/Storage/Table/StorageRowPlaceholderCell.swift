import SwiftUI

/// Placeholder shown for a row whose page hasn't loaded yet. Only the first
/// column carries the standing bar; the rest stay empty.
struct StorageRowPlaceholderCell: View {
    let column: StorageTableColumn

    var body: some View {
        Group {
            if column == .album {
                RoundedRectangle(cornerRadius: 3)
                    .fill(Color.gray.opacity(0.15))
                    .frame(height: 14)
            }
            else {
                EmptyView()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 4)
    }
}

#if DEBUG
    #Preview("Placeholder row") {
        HStack(spacing: 0) {
            ForEach(StorageTableColumn.allCases, id: \.self) { column in
                StorageRowPlaceholderCell(column: column)
                    .frame(width: 110)
            }
        }
        .frame(width: 700)
        .padding(.vertical)
    }
#endif
