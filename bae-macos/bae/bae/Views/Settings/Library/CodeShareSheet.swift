import BaeKit
import SwiftUI

/// Shows this person's restore code for the library; `result` is nil while
/// the presenter loads it.
struct CodeShareSheet: View {
    @Binding
    var result: Result<String, Error>?
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Restore code")
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
                VStack(spacing: ThemeSpace.edge) {
                    Spacer()

                    CodeDisplay(code: code, qrSize: 200)

                    Text(
                        "Scan or type this code on your new device to add it to the library. Anyone with it and access to your storage can open the library. Keep it secret."
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
                        ErrorText(line)
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
        CodeShareSheetPreview(result: .success("restore-code-preview-abcdef"))
    }

    #Preview("Failed") {
        CodeShareSheetPreview(
            result: .failure(
                NSError(
                    domain: "preview",
                    code: 1,
                    userInfo: [
                        NSLocalizedDescriptionKey:
                            "Couldn't generate a restore code."
                    ],
                )
            )
        )
    }
#endif
