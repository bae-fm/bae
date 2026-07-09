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

/// One mode's sort criteria as capsule pills: clicking a pill inverts its
/// direction, the trailing "x" removes it, and the "+" menu is the only
/// dropdown left, offering fields not already in use. Field switching and
/// reordering are gone by design — the sort is built by adding and removing
/// pills. Shared by the album, composer, and artist library modes.
struct SortCriteriaRow<Criterion: SortCriterionRepresentable>: View {
    @Binding
    var criteria: [Criterion]

    var body: some View {
        HStack(spacing: 4) {
            ForEach($criteria, id: \.field) { $criterion in
                let field = criterion.field
                SortCriterionPill(
                    criterion: $criterion,
                    canRemove: criteria.count > 1,
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
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .help("Add sort criterion")
            }
        }
    }

    private var usedFields: Set<Criterion.Field> {
        Set(criteria.map(\.field))
    }
}
