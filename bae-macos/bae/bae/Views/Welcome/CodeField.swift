import BaeKit
import SwiftUI

/// The field for a restore code or an invite, typed or pasted: several lines
/// tall and monospaced, showing the code in groups of four with a space
/// between, as Settings shows a restore code.
struct CodeField: View {
    @Bindable
    var form: BootstrapCodeForm

    var body: some View {
        TextField(
            text: Binding(
                get: { form.codeInput },
                set: { form.edit(code: $0) }
            ),
            prompt: Text("Restore code or invite"),
            axis: .vertical
        ) {
            EmptyView()
        }
        .labelsHidden()
        .lineLimit(3...)
        .themeText(.mono)
        .boxedTextField(multiline: true)
    }
}
