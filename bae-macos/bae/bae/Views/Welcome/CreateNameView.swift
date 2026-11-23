import BaeKit
import SwiftUI

/// The step before a library is made: it asks the creator's name, which
/// becomes their profile, then creates the library and goes on with the
/// steps the create runs. Cancel returns to the first screen.
struct CreateNameView: View {
    @State
    private var form: CreateLibraryForm
    let onCancel: () -> Void

    /// The step keeps the first form it is handed for as long as it shows.
    init(form: CreateLibraryForm, onCancel: @escaping () -> Void) {
        _form = State(initialValue: form)
        self.onCancel = onCancel
    }

    var body: some View {
        MacStep(String(localized: "Start a new library")) {
            VStack(spacing: ThemeSpace.group) {
                FieldColumn {
                    MacTextField("Your name") {
                        BoxedField(
                            placeholder:
                                "Appears on your profile, ratings and playlists",
                            text: $form.name,
                            focusesOnAppear: true
                        )
                        .disabled(form.isRunning)
                        .onSubmit { form.create() }
                    }
                }
                .frame(width: 400)
                if let steps = form.steps {
                    MacProgressContainer(steps: steps.steps) { EmptyView() }
                        .frame(maxWidth: StepProgressLayout.width)
                }
                if let error = form.error {
                    MacNotice(error: error)
                        .frame(maxWidth: WelcomeLayout.columnWidth)
                }
                MacStepActions {
                    MacButton("Cancel", kind: .borderless) {
                        form.cancel()
                        onCancel()
                    }
                    .neutral()
                    .keyboardShortcut(.cancelAction)
                    MacButton("Create", kind: .primary) { form.create() }
                        .keyboardShortcut(.defaultAction)
                        .disabled(!form.canCreate)
                }
            }
        }
        .onDisappear { form.cancel() }
    }
}
