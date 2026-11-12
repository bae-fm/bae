import BaeKit
import SwiftUI

/// The library being renamed (`id`), its in-progress name, and the error
/// from a failed rename; the value also drives `.sheet(item:)`.
struct RenameLibrarySheetState: Identifiable {
    let id: String
    var newName: String
    var error: DisplayError?
}

/// Modal for renaming a local library; the caller performs the rename and
/// writes any error back into `state`.
struct RenameLibrarySheet: View {
    @Binding
    var state: RenameLibrarySheetState
    let onCancel: () -> Void
    let onCommit: (String) -> Void

    var body: some View {
        MacSheet("Rename library", width: 420) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                FieldColumn {
                    MacTextField(
                        "New name",
                        note: "Everyone in the library sees this name."
                    ) {
                        TextField(
                            "New name",
                            text: Binding(
                                get: { state.newName },
                                set: { state.newName = $0 }
                            )
                        )
                        .labelsHidden()
                        .boxedTextField()
                    }
                }
                if let error = state.error {
                    Notice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel") { onCancel() }
                    .keyboardShortcut(.cancelAction)
                Button("Rename") { onCommit(state.newName) }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        state.newName
                            .trimmingCharacters(
                                in: .whitespacesAndNewlines
                            )
                            .isEmpty
                    )
            }
        }
    }
}
