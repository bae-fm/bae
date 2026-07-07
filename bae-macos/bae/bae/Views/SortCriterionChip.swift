import BaeKit
import SwiftUI

/// One album sort criterion, rendered as a borderless dropdown chip: the label
/// shows the field and direction; the menu flips direction, removes the
/// criterion, or switches the field. Used by `LibraryView`'s album sort
/// controls in the pinned library header.
struct SortCriterionChip: View {
    @Binding
    var criterion: BridgeSortCriterion
    let choosableFields: [BridgeSortField]
    let canRemove: Bool
    let onRemove: () -> Void

    var body: some View {
        Menu {
            Button {
                criterion.direction =
                    criterion.direction == .ascending ? .descending : .ascending
            } label: {
                Label(
                    criterion.direction == .ascending
                        ? "Sort Descending" : "Sort Ascending",
                    systemImage: criterion.direction == .ascending
                        ? "arrow.down" : "arrow.up",
                )
            }
            if canRemove {
                Button(role: .destructive, action: onRemove) {
                    Label("Remove", systemImage: "xmark.circle")
                }
            }
            Divider()
            ForEach(choosableFields, id: \.self) { field in
                Button {
                    criterion.field = field
                } label: {
                    HStack {
                        Text(field.displayName)
                        if criterion.field == field {
                            Image(systemName: "checkmark")
                        }
                    }
                }
            }
        } label: {
            HStack(spacing: 2) {
                Text(criterion.field.displayName)
                Image(
                    systemName: criterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
            }
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
    }
}
