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
        HStack(spacing: 6) {
            Button {
                // Absolute set: the target direction is computed from the
                // rendered one, not toggled blind.
                criterion.direction =
                    criterion.direction == .ascending ? .descending : .ascending
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
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Remove sort criterion"))
                .help("Remove sort criterion")
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .background(Theme.surfaceElevated, in: Capsule())
    }
}
