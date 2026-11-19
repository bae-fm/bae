import BaeKit
import SwiftUI

/// Label and value pairs in two columns: each label in the secondary colour
/// over a fixed-width column, its value beside it, selectable. Its content is
/// `MacFact` rows.
struct MacFactsGrid<Facts: View>: View {
    let facts: () -> Facts

    init(@ViewBuilder facts: @escaping () -> Facts) {
        self.facts = facts
    }

    var body: some View {
        Grid(
            alignment: .leadingFirstTextBaseline,
            horizontalSpacing: ThemeSpace.group,
            verticalSpacing: ThemeSpace.compact
        ) {
            facts()
        }
        .themeText(.detail)
    }
}

/// One label and value pair of a `MacFactsGrid`.
struct MacFact<Value: View>: View {
    let label: LocalizedStringKey
    let value: () -> Value

    init(
        _ label: LocalizedStringKey,
        @ViewBuilder value: @escaping () -> Value
    ) {
        self.label = label
        self.value = value
    }

    /// The labels' column.
    private static var labelWidth: CGFloat { 84 }

    var body: some View {
        GridRow {
            Text(label)
                .foregroundStyle(.secondary)
                .frame(width: Self.labelWidth, alignment: .leading)
            value()
                .textSelection(.enabled)
        }
    }
}
