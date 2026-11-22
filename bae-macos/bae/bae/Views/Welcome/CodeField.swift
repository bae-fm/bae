import BaeKit
import SwiftUI

/// The field for a restore code or an invite, typed or pasted: several lines
/// tall and monospaced, showing the code in groups of four with a space
/// between, as Settings shows a restore code.
struct CodeField: View {
    @Bindable
    var form: BootstrapCodeForm

    var body: some View {
        BoxedField(
            placeholder: "Restore code or invite",
            text: Binding(
                get: { form.codeInput },
                set: { form.edit(code: $0) }
            ),
            font: .mono,
            multiline: true,
            focusesOnAppear: true
        )
    }
}
