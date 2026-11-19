import BaeKit
import SwiftUI

/// One column of the storage table's row at `position`, drawn from the row
/// the list's query returned there, or as a placeholder until that row's page
/// lands. It reads the list itself, so it follows the row as the query
/// answers again.
struct StorageListCell: View {
    let list: StorageList
    let position: Int
    let column: StorageTableColumn

    var body: some View {
        if let row = list.row(at: position) {
            MacStorageCell(
                release: ReleaseSummary(from: row.release),
                album: row.album,
                status: row.status,
                dateAdded: row.dateAdded,
                column: column
            )
        }
        else {
            placeholder
        }
    }

    /// A not-yet-loaded row's cell: a placeholder bar in the release column,
    /// empty elsewhere.
    private var placeholder: some View {
        Group {
            if column == .release {
                MacPlaceholderLine(width: 180, height: 14)
            }
            else {
                EmptyView()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, ThemeSpace.inline)
    }
}
