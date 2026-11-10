import BaeKit
import SwiftUI

/// The sheet over the first screen that asks for a restore code or an invite
/// to a library. Continue reads the code: an invite joins, and a restore
/// starts, or asks for the S3 key first. A code that doesn't read, or a
/// restore or join that failed, says so under the field.
struct EnterCodeSheet: View {
    @Bindable
    var form: BootstrapCodeForm
    let onCancel: () -> Void

    /// The join this device had under way, while the field is empty: its code
    /// is kept, so Join resumes it, and a code typed in its place joins that.
    private var resumedJoin: BridgeInviteInfo? {
        guard isEmpty, case .resume(_, let invite) = form.attempt else {
            return nil
        }
        return invite
    }

    private var isEmpty: Bool {
        form.codeInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    var body: some View {
        MacSheet(
            "Enter code",
            message: "Enter your restore code, or an invite to a library.",
            width: 400
        ) {
            VStack(alignment: .leading, spacing: ThemeSpace.related) {
                if let resumedJoin {
                    LabeledContent("Library", value: resumedJoin.libraryName)
                }
                CodeField(form: form)
                if let decodeError = form.decodeError {
                    Notice(error: decodeError)
                }
                if let error = form.error {
                    Notice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                if resumedJoin != nil {
                    Button("Join") { form.start() }
                        .buttonStyle(PrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)
                }
                else {
                    Button("Continue") { form.acceptCode(form.codeInput) }
                        .buttonStyle(PrimaryButtonStyle())
                        .keyboardShortcut(.defaultAction)
                        .disabled(isEmpty)
                }
            }
        }
    }
}

#if DEBUG
    #Preview("Enter code") {
        EnterCodeSheet(
            form: LibrarySetup.stub()
                .bootstrapForm(entry: .code, onLibraryReady: { _, _ in }),
            onCancel: {}
        )
        .windowBackground()
    }
#endif
