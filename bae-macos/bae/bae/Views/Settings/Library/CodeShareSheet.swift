import SwiftUI

/// QR-code + textual code + copy button for the library's recovery code — the
/// bearer code that grants full access on a new device when no existing device
/// is available to approve a join. `result` carries the loading/loaded/error
/// state as `Result<String, Error>?`: `nil` is loading, `.success(code)` shows
/// the code, `.failure(err)` shows the error message. It's a binding because the
/// presenter computes the code off-main after the sheet is already up — the
/// sheet has to re-render when that write lands, not show a stale snapshot.
struct CodeShareSheet: View {
    @Binding
    var result: Result<String, Error>?
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Recovery code")
                    .font(.headline)
                Spacer()
                Button("Done") { onDismiss() }
                    .buttonStyle(.borderless)
            }
            .padding()

            Divider()

            switch result {
            case nil:
                VStack {
                    Spacer()
                    ProgressView()
                    Spacer()
                }
            case .success(let code):
                VStack(spacing: 16) {
                    Spacer()

                    CodeDisplay(code: code, qrSize: 200)

                    Text(
                        "Anyone with this code has full access to your library. Keep it secret. Use it only to restore on a new device when you have no other device available."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                    Spacer()
                }
                .padding()
            case .failure(let error):
                VStack {
                    Spacer()
                    Text(error.displayLine ?? "")
                        .foregroundStyle(.red)
                        .font(.callout)
                    Spacer()
                }
                .padding()
            }
        }
        .frame(width: 400, height: 440)
    }
}

#if DEBUG
    // MARK: - Previews

    /// Holds the async result the presenter normally writes after the sheet is
    /// up, so each variant renders one of the loading / loaded / failed states.
    private struct CodeShareSheetPreview: View {
        @State
        var result: Result<String, Error>?

        var body: some View {
            CodeShareSheet(result: $result, onDismiss: {})
        }
    }

    #Preview("Loading") {
        CodeShareSheetPreview(result: nil)
    }

    #Preview("Code") {
        CodeShareSheetPreview(result: .success("recovery-code-preview-abcdef"))
    }

    #Preview("Failed") {
        CodeShareSheetPreview(
            result: .failure(
                NSError(
                    domain: "preview",
                    code: 1,
                    userInfo: [
                        NSLocalizedDescriptionKey:
                            "Couldn't generate a recovery code."
                    ],
                )
            )
        )
    }
#endif
