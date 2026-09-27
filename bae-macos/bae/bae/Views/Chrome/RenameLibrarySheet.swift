import BaeKit
import SwiftUI

/// The library being renamed (`id`), its in-progress name, and the error
/// from a failed rename; the value also drives `.sheet(item:)`.
struct RenameLibrarySheetState: Identifiable {
    let id: String
    var newName: String
    var error: String?
}

/// Modal for renaming a local library; the caller performs the rename and
/// writes any error back into `state`.
struct RenameLibrarySheet: View {
    @Binding
    var state: RenameLibrarySheetState
    let onCancel: () -> Void
    let onCommit: (String) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Rename Library")
                    .themeText(.heading)
                Spacer()
            }
            .padding()
            Divider()

            Form {
                Section {
                    TextField(
                        "New name",
                        text: Binding(
                            get: { state.newName },
                            set: { state.newName = $0 }
                        )
                    )
                }
                if let error = state.error {
                    Section {
                        Text(error)
                            .foregroundStyle(Theme.danger)
                            .themeText(.body)
                    }
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)

            HStack(spacing: ThemeSpace.group) {
                Spacer()
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
            .padding()
        }
        .frame(width: 420, height: 240)
    }
}

#if DEBUG
    #Preview("Rename Library") {
        @Previewable
        @State
        var state = RenameLibrarySheetState(
            id: "lib-preview",
            newName: "Album Library",
            error: nil
        )
        RenameLibrarySheet(
            state: $state,
            onCancel: {},
            onCommit: { _ in }
        )
    }

    #Preview("Rename Library \u{2014} Error") {
        @Previewable
        @State
        var state = RenameLibrarySheetState(
            id: "lib-preview",
            newName: "Album Library",
            error: "A library with that name already exists"
        )
        RenameLibrarySheet(
            state: $state,
            onCancel: {},
            onCommit: { _ in }
        )
    }
#endif
