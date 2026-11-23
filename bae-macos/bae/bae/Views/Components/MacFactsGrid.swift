import BaeKit
import SwiftUI

/// Label and value pairs in two columns: each label in the secondary colour
/// over a fixed-width column, its value beside it, selectable. Its content is
/// `MacFact` rows. `wide` draws a 140-point label column and body-sized text,
/// as an album's credits do.
struct MacFactsGrid<Facts: View>: View {
    let wide: Bool
    let facts: () -> Facts

    init(wide: Bool = false, @ViewBuilder facts: @escaping () -> Facts) {
        self.wide = wide
        self.facts = facts
    }

    var body: some View {
        Grid(
            alignment: .leadingFirstTextBaseline,
            horizontalSpacing: ThemeSpace.group,
            verticalSpacing: wide ? ThemeSpace.inline : ThemeSpace.compact
        ) {
            facts()
        }
        .themeText(wide ? .body : .detail)
        .environment(\.factsGridWide, wide)
    }
}

extension EnvironmentValues {
    // swiftui-environment-audit: optional
    /// Whether the `MacFactsGrid` around a fact draws its wide look. A fact
    /// outside a grid draws the narrow look, which is this default.
    @Entry
    var factsGridWide = false
}

/// One label and value pair of a `MacFactsGrid`.
struct MacFact<Value: View>: View {
    let label: Text
    let value: () -> Value

    init(
        _ label: LocalizedStringKey,
        @ViewBuilder value: @escaping () -> Value
    ) {
        self.label = Text(label)
        self.value = value
    }

    /// A label already in the person's language.
    init(
        verbatim label: String,
        @ViewBuilder value: @escaping () -> Value
    ) {
        self.label = Text(verbatim: label)
        self.value = value
    }

    @Environment(\.factsGridWide)
    private var wide

    /// The labels' column.
    private var labelWidth: CGFloat { wide ? 140 : 84 }

    var body: some View {
        GridRow {
            label
                .foregroundStyle(Theme.textSecondary)
                .frame(width: labelWidth, alignment: .leading)
            value()
                .textSelection(.enabled)
        }
    }
}
