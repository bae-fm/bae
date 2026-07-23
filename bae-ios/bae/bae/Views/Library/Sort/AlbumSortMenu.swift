import BaeKit
import SwiftUI

/// The album-mode sort menu: pick the sort field, flip the direction.
struct AlbumSortMenu: View {
    @Binding
    var sortField: BridgeSortField
    @Binding
    var sortDirection: BridgeSortDirection

    var body: some View {
        Menu {
            ForEach(BridgeSortField.allCases, id: \.self) { field in
                Button {
                    sortField = field
                } label: {
                    if field == sortField {
                        Label(field.displayName, systemImage: "checkmark")
                    }
                    else {
                        Text(field.displayName)
                    }
                }
            }
            Divider()
            Button {
                sortDirection =
                    sortDirection == .ascending ? .descending : .ascending
            } label: {
                Label(
                    sortDirection == .ascending
                        ? String(localized: "Ascending")
                        : String(localized: "Descending"),
                    systemImage: sortDirection == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }
}
