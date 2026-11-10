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

/// One row of a `FieldColumn`: the label, then the field, with the note
/// under it that starts where the field's text starts.
struct FieldRow<Field: View>: View {
    let label: LocalizedStringKey
    let note: LocalizedStringKey?
    private let field: Field

    init(
        _ label: LocalizedStringKey,
        note: LocalizedStringKey? = nil,
        @ViewBuilder field: () -> Field
    ) {
        self.label = label
        self.note = note
        self.field = field()
    }

    var body: some View {
        GridRow(alignment: .firstTextBaseline) {
            Text(label)
                .gridColumnAlignment(.trailing)
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                field
                if let note {
                    Text(note)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .padding(.leading, ThemeSpace.fieldInset)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
