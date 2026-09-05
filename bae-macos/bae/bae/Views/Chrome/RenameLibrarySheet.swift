import BaeKit
import SwiftUI

/// Identifies the library being renamed and carries the in-progress
/// name plus any error the caller writes back after a failed rename.
/// `id` is the library id, so the value doubles as the `.sheet(item:)`
/// trigger.
struct RenameLibrarySheetState: Identifiable {
    let id: String
    var newName: String
    var error: String?
}

/// Small modal for renaming any local library (active or inactive).
/// The caller owns task lifecycle — this view just edits a name and
/// reports it back through `onCommit`. The `state` binding carries an
/// error message that the caller writes after a failed bridge call.
struct RenameLibrarySheet: View {
    @Binding
    var state: RenameLibrarySheetState
    let onCancel: () -> Void
    let onCommit: (String) -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Rename Library")
                    .font(.headline)
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
                            .foregroundStyle(.red)
                            .font(.callout)
                    }
                }
            }
            .formStyle(.grouped)
            .scrollDisabled(true)

            HStack(spacing: 12) {
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
