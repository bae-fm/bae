import BaeKit
import SwiftUI

/// A labelled control in a sheet's form: the label above the control and an
/// optional note under it, all on the form's one left edge. A text field
/// inside it runs the form's width; any other control keeps its own width.
struct MacFormField<Control: View>: View {
    let label: LocalizedStringKey
    let note: Text?
    private let control: Control

    init(
        _ label: LocalizedStringKey,
        note: Text? = nil,
        @ViewBuilder control: () -> Control
    ) {
        self.label = label
        self.note = note
        self.control = control()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(label)
            control
                .accessibilityLabel(Text(label))
            if let note {
                note
                    .fieldNote()
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
