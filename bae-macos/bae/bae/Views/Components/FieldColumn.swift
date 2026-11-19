import BaeKit
import SwiftUI

/// A column of fields, each with its label beside it. A label is aligned to
/// the edge of its field (the trailing edge in a left-to-right language), as
/// macOS lays out a column of labelled fields: every label ends the same
/// distance from its box, however long the longest label is.
struct FieldColumn<Rows: View>: View {
    @ViewBuilder
    let rows: Rows

    var body: some View {
        Grid(
            alignment: .leading,
            horizontalSpacing: ThemeSpace.group,
            verticalSpacing: ThemeSpace.related
        ) {
            rows
        }
    }
}
