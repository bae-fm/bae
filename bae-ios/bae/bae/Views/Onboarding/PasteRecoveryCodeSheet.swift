import BaeKit
import SwiftUI

/// The paste-a-recovery-code sheet: a field, and a Connect action that hands
/// the trimmed code to the owner once one is entered.
struct PasteRecoveryCodeSheet: View {
    @Binding
    var input: String
    let onCancel: () -> Void
    let onConnect: (String) -> Void

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: ThemeSpace.edge) {
                Text(
                    "Paste your recovery code. Use this only when you have no other device available to approve this one."
                )
                .themeText(.body)
                .foregroundStyle(.secondary)
                TextField(
                    "Paste your recovery code",
                    text: $input,
                    axis: .vertical
                )
                .textFieldStyle(.roundedBorder)
                .themeText(.mono)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .lineLimit(3, reservesSpace: true)
                Spacer()
            }
            .padding()
            .navigationTitle("Paste recovery code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { onCancel() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Connect") {
                        onConnect(
                            input.trimmingCharacters(in: .whitespacesAndNewlines)
                        )
                    }
                    .disabled(
                        input.trimmingCharacters(
                            in: .whitespacesAndNewlines
                        )
                        .isEmpty
                    )
                }
            }
        }
    }
}

#if DEBUG
#Preview {
    PasteRecoveryCodeSheet(
        input: .constant(""),
        onCancel: {},
        onConnect: { _ in }
    )
}
#endif
