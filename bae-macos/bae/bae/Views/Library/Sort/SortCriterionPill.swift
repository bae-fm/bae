import BaeKit
import SwiftUI

/// One sort criterion as a pill: a field menu that re-points it, an arrow that
/// flips its direction, and an "x" that removes it. A field another pill sorts
/// by shows disabled in the menu.
struct SortCriterionPill<Criterion: SortCriterionRepresentable>: View {
    @Binding
    var criterion: Criterion
    /// Fields other pills in the row already sort by.
    let takenFields: Set<Criterion.Field>
    let canRemove: Bool
    /// Re-point this criterion at `field`; the row owns the list.
    let onSetField: (Criterion.Field) -> Void
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 7) {
            fieldMenu
            Button {
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
            Theme.placeholder,
            in: RoundedRectangle(cornerRadius: ThemeRadius.control)
        )
    }

    /// Every field, the current one checked; picking it again is a no-op.
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
            // One run of text, since a menu label puts its image before the
            // title and the chevron belongs after it.
            Text(criterion.field.displayName)
                .font(ThemeText.strong.font)
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
