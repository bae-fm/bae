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
                    MacRow("Library", value: resumedJoin.libraryName)
                }
                CodeField(form: form)
                if let decodeError = form.decodeError {
                    MacNotice(error: decodeError)
                }
                if let error = form.error {
                    MacNotice(error: error)
                }
            }
        } footer: {
            MacSheetFooter {
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                if resumedJoin != nil {
                    MacButton("Join", kind: .primary) { form.start() }
                        .keyboardShortcut(.defaultAction)
                }
                else {
                    MacButton("Continue", kind: .primary) {
                        form.acceptCode(form.codeInput)
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(isEmpty)
                }
            }
        }
    }
}
