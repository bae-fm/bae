import BaeKit
import SwiftUI

/// The composer-mode sort menu: pick the sort field, flip the direction.
struct ComposerSortMenu: View {
    @Binding
    var criterion: BridgeComposerSortCriterion

    var body: some View {
        Menu {
            ForEach(BridgeComposerSortField.allCases, id: \.self) { field in
                Button {
                    criterion = BridgeComposerSortCriterion(
                        field: field,
                        direction: criterion.direction
                    )
                } label: {
                    if field == criterion.field {
                        Label(field.displayName, systemImage: "checkmark")
                    }
                    else {
                        Text(field.displayName)
                    }
                }
            }
            Divider()
            Button {
                let direction: BridgeSortDirection =
                    criterion.direction == .ascending ? .descending : .ascending
                criterion = BridgeComposerSortCriterion(
                    field: criterion.field,
                    direction: direction
                )
            } label: {
                Label(
                    criterion.direction == .ascending
                        ? String(localized: "Ascending")
                        : String(localized: "Descending"),
                    systemImage: criterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
        }
    }
}
