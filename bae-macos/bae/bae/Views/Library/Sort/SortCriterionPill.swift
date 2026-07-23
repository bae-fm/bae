import BaeKit
import SwiftUI

/// One sort criterion, rendered as a capsule pill: the main button (field
/// label + direction arrow) sets the opposite direction, and the trailing
/// "x" removes the criterion. Field switching and reordering are gone by
/// design — the sort is built by adding and removing pills. Used by
/// `SortCriteriaRow`, shared across the album, composer, and artist library
/// modes.
struct SortCriterionPill<Criterion: SortCriterionRepresentable>: View {
    @Binding
    var criterion: Criterion
    let canRemove: Bool
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 7) {
            Button {
                // Absolute set: the target direction is computed from the
                // rendered one, not toggled blind.
                criterion.direction =
                    criterion.direction == .ascending ? .descending : .ascending
            } label: {
                HStack(spacing: 5) {
                    Text(criterion.field.displayName)
                        .font(.system(size: 13, weight: .semibold))
                    Image(
                        systemName: criterion.direction == .ascending
                            ? "arrow.up" : "arrow.down"
                    )
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(.secondary)
                }
            }
            .buttonStyle(.plain)
            .accessibilityLabel(criterion.field.displayName)
            .help(
                criterion.direction == .ascending
                    ? String(localized: "Sort Descending")
                    : String(localized: "Sort Ascending")
            )
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
                canRemove: true,
                onRemove: {}
            )
            // Only criterion left: not removable.
            SortCriterionPill(
                criterion: $descending,
                canRemove: false,
                onRemove: {}
            )
        }
        .padding()
    }
#endif
