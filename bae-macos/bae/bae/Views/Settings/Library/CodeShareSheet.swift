import BaeKit
import SwiftUI

/// Shows the library's recovery code; `result` is nil while the presenter
/// loads it.
struct CodeShareSheet: View {
    @Binding
    var result: Result<String, Error>?
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Recovery code")
                    .themeText(.heading)
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
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                    Spacer()
                }
                .padding()
            case .failure(let error):
                VStack {
                    Spacer()
                    // The presenter only records failures that have a line.
                    if let line = error.displayLine {
                        Text(line)
                            .foregroundStyle(Theme.danger)
                            .themeText(.body)
                    }
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

    /// Holds the result the presenter normally writes after the sheet is up.
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
