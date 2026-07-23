import SwiftUI

/// The paste-a-recovery-code sheet: a multi-line field and a Connect action,
/// disabled until something non-blank is entered. Trimming and the connect
/// itself are the owner's.
struct PasteRecoveryCodeSheet: View {
    @Binding
    var input: String
    let onCancel: () -> Void
    let onConnect: (String) -> Void

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text(
                    "Paste your recovery code. Use this only when you have no other device available to approve this one."
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                TextField(
                    "Paste your recovery code",
                    text: $input,
                    axis: .vertical
                )
                .textFieldStyle(.roundedBorder)
                .font(.body.monospaced())
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
