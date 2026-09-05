import BaeKit
import SwiftUI

/// One sort criterion as a capsule pill in three parts: the field name, whose
/// menu re-points the criterion at another field; the direction arrow, which
/// sets the opposite direction; and the trailing "x", which removes it.
///
/// Re-pointing keeps the pill's place and direction, so changing what a lone
/// pill sorts by is one pick rather than an add and a remove. A field another
/// pill already sorts by stays in the menu, disabled: that pill is where it
/// lives. Used by `SortCriteriaRow`, shared across the album, composer, and
/// artist library modes.
struct SortCriterionPill<Criterion: SortCriterionRepresentable>: View {
    @Binding
    var criterion: Criterion
    /// Fields other pills in the row already sort by.
    let takenFields: Set<Criterion.Field>
    let canRemove: Bool
    /// Re-point this criterion at `field`. The row owns the list, so the
    /// replacement is its write.
    let onSetField: (Criterion.Field) -> Void
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 7) {
            fieldMenu
            Button {
                // Absolute set: the target direction is computed from the
                // rendered one, not toggled blind.
                criterion.direction =
                    criterion.direction == .ascending ? .descending : .ascending
            } label: {
                Image(
                    systemName: criterion.direction == .ascending
                        ? "arrow.up" : "arrow.down"
                )
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(directionAction)
            .help(directionAction)
            if canRemove {
                Button(action: onRemove) {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Remove sort criterion"))
                .help("Remove sort criterion")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(
            Theme.placeholder.opacity(0.85),
            in: RoundedRectangle(cornerRadius: 9)
        )
    }

    /// Every field, the current one checked. Picking the current one again
    /// is a no-op the row refuses, so the menu need not special-case it.
    private var fieldMenu: some View {
        Menu {
            ForEach(Criterion.Field.allCases, id: \.self) { field in
                Toggle(
                    field.displayName,
                    isOn: Binding(
                        get: { field == criterion.field },
                        set: { _ in onSetField(field) }
                    )
                )
                .disabled(takenFields.contains(field))
            }
        } label: {
            // One run of text: a menu button lays a label's image ahead of
            // its title, and the chevron belongs after the name.
            Text(criterion.field.displayName)
                .font(.system(size: 13, weight: .semibold))
                + Text(verbatim: " ")
                + Text(Image(systemName: "chevron.down"))
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .accessibilityLabel(criterion.field.displayName)
        .help("Change sort field")
    }

    private var directionAction: String {
        criterion.direction == .ascending
            ? String(localized: "Sort Descending")
            : String(localized: "Sort Ascending")
    }
}

#if DEBUG
    #Preview("Sort Criterion Pill") {
        @Previewable
        @State
        var ascending = BridgeSortCriterion(
            field: .title,
            direction: .ascending
        )
        @Previewable
        @State
        var descending = BridgeSortCriterion(
            field: .dateAdded,
            direction: .descending
        )
        HStack(spacing: 8) {
            SortCriterionPill(
                criterion: $ascending,
                takenFields: [.dateAdded],
                canRemove: true,
                onSetField: { _ in },
                onRemove: {}
            )
            // Only criterion left: not removable.
            SortCriterionPill(
                criterion: $descending,
                takenFields: [],
                canRemove: false,
                onSetField: { _ in },
                onRemove: {}
            )
        }
        .padding()
    }
#endif
