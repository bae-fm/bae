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

    var body: some View {
        MacStep(String(localized: "You are invited to \(libraryName)")) {
            VStack(spacing: ThemeSpace.section) {
                FieldColumn {
                    MacTextField("Your name") {
                        BoxedField(
                            placeholder:
                                "Appears on your profile, ratings and playlists",
                            text: $name,
                            focusesOnAppear: true
                        )
                        .onSubmit { if canJoin { onJoin() } }
                    }
                }
                .frame(width: 400)
                if let error {
                    MacNotice(error: error)
                        .frame(maxWidth: WelcomeLayout.columnWidth)
                }
                MacStepActions {
                    MacButton("Cancel", kind: .borderless, action: onCancel)
                        .neutral()
                        .keyboardShortcut(.cancelAction)
                    MacButton("Join", kind: .primary, action: onJoin)
                        .keyboardShortcut(.defaultAction)
                        .disabled(!canJoin)
                }
            }
        }
    }
}
