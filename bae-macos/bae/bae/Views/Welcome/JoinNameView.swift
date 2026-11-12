import BaeKit
import SwiftUI

/// The step after an invite's code reads: it asks the joining person's name,
/// which their join request carries, then joins. Cancel returns to the first
/// screen. A name core refuses shows as the step's error.
struct JoinNameView: View {
    let libraryName: String
    @Binding
    var name: String
    let error: DisplayError?
    let canJoin: Bool
    let onJoin: () -> Void
    let onCancel: () -> Void

    @FocusState
    private var focused: Bool

    var body: some View {
        MacStep(String(localized: "You are invited to \(libraryName)")) {
            VStack(spacing: ThemeSpace.group) {
                FieldColumn {
                    MacTextField("Your name") {
                        TextField(
                            "Your name",
                            text: $name,
                            prompt: Text(
                                "Appears on your profile, ratings and playlists"
                            )
                        )
                        .labelsHidden()
                        .accessibilityLabel(Text("Your name"))
                        .focused($focused)
                        .onSubmit { if canJoin { onJoin() } }
                    }
                }
                .frame(width: 400)
                if let error {
                    Notice(error: error)
                        .frame(maxWidth: WelcomeLayout.columnWidth)
                }
                MacStepActions {
                    Button("Cancel", action: onCancel)
                        .buttonStyle(.borderless)
                        .foregroundStyle(Theme.textPrimary)
                        .keyboardShortcut(.cancelAction)
                    Button("Join", action: onJoin)
                        .buttonStyle(PrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)
                        .disabled(!canJoin)
                }
            }
        }
        .onAppear { focused = true }
    }
}
