import BaeKit
import SwiftUI

/// A column of fields, each with its label beside it. A label is aligned to
/// the edge of its field (the trailing edge in a left-to-right language), as
/// macOS lays out a column of labelled fields: every label ends the same
/// distance from its box, however long the longest label is. With
/// `labelWidth`, the labels' column is that wide, as a board that sets it
/// draws it.
struct FieldColumn<Rows: View>: View {
    let labelWidth: CGFloat?
    private let rows: Rows

    init(labelWidth: CGFloat? = nil, @ViewBuilder rows: () -> Rows) {
        self.labelWidth = labelWidth
        self.rows = rows()
    }

    var body: some View {
        Grid(
            alignment: .leading,
            horizontalSpacing: ThemeSpace.group,
            verticalSpacing: ThemeSpace.related
        ) {
            rows
        }
        .environment(\.fieldLabelWidth, labelWidth)
    }
}

extension EnvironmentValues {
    /// The width of the label column of the `FieldColumn` around a view.
    @Entry
    var fieldLabelWidth: CGFloat?
}
