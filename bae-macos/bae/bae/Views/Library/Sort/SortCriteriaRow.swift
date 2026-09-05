import BaeKit
import SwiftUI

/// A sortable field the pill row can label and enumerate.
protocol SortCriterionField: Hashable, CaseIterable where AllCases == [Self] {
    var displayName: String { get }
}

/// A field+direction criterion the pill row renders and mutates.
protocol SortCriterionRepresentable: Equatable {
    associatedtype Field: SortCriterionField
    var field: Field { get }
    var direction: BridgeSortDirection { get set }
    init(field: Field, direction: BridgeSortDirection)
}

extension BridgeSortField: SortCriterionField {}
extension BridgeComposerSortField: SortCriterionField {}
extension BridgeArtistSortField: SortCriterionField {}
extension BridgeSortCriterion: SortCriterionRepresentable {}
extension BridgeComposerSortCriterion: SortCriterionRepresentable {}
extension BridgeArtistSortCriterion: SortCriterionRepresentable {}

/// One mode's sort criteria as capsule pills: a pill's field menu re-points
/// it, its arrow inverts its direction, the trailing "x" removes it, and the
/// "+" menu appends a field not already in use. Reordering is gone by
/// design — precedence is the order pills were added. Shared by the album,
/// composer, and artist library modes.
struct SortCriteriaRow<Criterion: SortCriterionRepresentable>: View {
    @Binding
    var criteria: [Criterion]

    var body: some View {
        HStack(spacing: 8) {
            ForEach($criteria, id: \.field) { $criterion in
                let field = criterion.field
                SortCriterionPill(
                    criterion: $criterion,
                    takenFields: usedFields.subtracting([field]),
                    canRemove: criteria.count > 1,
                    onSetField: { criteria.replaceField(field, with: $0) },
                    onRemove: { criteria.removeAll { $0.field == field } },
                )
            }
            let unused = Criterion.Field.allCases.filter {
                !usedFields.contains($0)
            }
            if !unused.isEmpty {
                Menu {
                    ForEach(unused, id: \.self) { field in
                        Button(field.displayName) {
                            criteria.append(
                                Criterion(field: field, direction: .ascending)
                            )
                        }
                    }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .frame(width: 30, height: 30)
                        .background(
                            Theme.placeholder.opacity(0.85),
                            in: RoundedRectangle(cornerRadius: 9)
                        )
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .help("Add sort criterion")
            }
        }
    }

    private var usedFields: Set<Criterion.Field> {
        Set(criteria.map(\.field))
    }
}

extension Array where Element: SortCriterionRepresentable {
    /// Re-point the criterion sorting by `field` at `replacement`, keeping its
    /// place and direction. Nothing changes when `field` is not here or when
    /// `replacement` already is: a field sorts once, in one pill.
    mutating func replaceField(
        _ field: Element.Field,
        with replacement: Element.Field
    ) {
        guard let index = firstIndex(where: { $0.field == field }),
            !contains(where: { $0.field == replacement })
        else { return }
        self[index] = Element(
            field: replacement,
            direction: self[index].direction
        )
    }
}

#if DEBUG
    #Preview("Sort Criteria Row") {
        @Previewable
        @State
        var criteria: [BridgeSortCriterion] = [
            BridgeSortCriterion(field: .dateAdded, direction: .descending),
            BridgeSortCriterion(field: .title, direction: .ascending),
        ]
        SortCriteriaRow(criteria: $criteria)
            .padding()
    }

    #Preview("Sort Criteria Row \u{2014} Single") {
        @Previewable
        @State
        var criteria: [BridgeSortCriterion] = [
            BridgeSortCriterion(field: .artist, direction: .ascending)
        ]
        SortCriteriaRow(criteria: $criteria)
            .padding()
    }
#endif
