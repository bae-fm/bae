import BaeKit
import SwiftUI

/// One row of a `FieldColumn`: the label, then the field, with the note
/// under it that starts where the field's text starts.
struct MacTextField<Field: View>: View {
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
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                field
                    .accessibilityLabel(Text(label))
                if let note {
                    FieldNote(note)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
