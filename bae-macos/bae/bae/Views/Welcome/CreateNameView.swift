import BaeKit
import SwiftUI

/// The step before a library is made: it asks the creator's name, which
/// becomes their profile, then creates the library and goes on with the
/// steps the create runs. Cancel returns to the first screen.
struct CreateNameView: View {
    @State
    private var form: CreateLibraryForm
    let onCancel: () -> Void

    @FocusState
    private var focused: Bool

    /// The step keeps the first form it is handed for as long as it shows.
    init(form: CreateLibraryForm, onCancel: @escaping () -> Void) {
        _form = State(initialValue: form)
        self.onCancel = onCancel
    }

    var body: some View {
        MacStep(String(localized: "Start a new library")) {
            VStack(spacing: ThemeSpace.group) {
                FieldColumn {
                    FieldRow("Your name") {
                        TextField(
                            "Your name",
                            text: $form.name,
                            prompt: Text(
                                "Appears on your profile, ratings and playlists"
                            )
                        )
                        .labelsHidden()
                        .focused($focused)
                        .disabled(form.isRunning)
                        .onSubmit { form.create() }
                    }
                }
                .frame(width: 400)
                if let steps = form.steps {
                    StepProgressLine(steps: steps.steps) { EmptyView() }
                        .frame(maxWidth: StepProgressLayout.width)
                }
                if let error = form.error {
                    Notice(error: error)
                        .frame(maxWidth: WelcomeLayout.columnWidth)
                }
                MacStepActions {
                    Button("Cancel") {
                        form.cancel()
                        onCancel()
                    }
                    .buttonStyle(.borderless)
                    .keyboardShortcut(.cancelAction)
                    Button("Create") { form.create() }
                        .buttonStyle(PrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)
                        .disabled(!form.canCreate)
                }
            }
        }
        .onAppear { focused = true }
        .onDisappear { form.cancel() }
    }
}
