import BaeKit
import SwiftUI

/// Shows this person's restore code for the library; `result` is nil while
/// the presenter loads it.
struct CodeShareSheet: View {
    @Binding
    var result: Result<BridgeRestoreCodeShare, Error>?
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
            case .success(let share):
                VStack(spacing: ThemeSpace.edge) {
                    Spacer()

                    CodeDisplay(
                        code: share.code,
                        qrText: share.qrText,
                        qrSize: 200
                    )

                    Text(
                        "Scan or type this code on your new device to add it to the library. Anyone with it and access to your storage can open the library. Keep it secret."
                    )
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                    if share.qrCarriesS3Key {
                        Text(
                            "The QR code also carries your S3 access key, so a device that scans it won't ask for the key."
                        )
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    }

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
        var result: Result<BridgeRestoreCodeShare, Error>?

        var body: some View {
            CodeShareSheet(result: $result, onDismiss: {})
        }
    }

    #Preview("Loading") {
        CodeShareSheetPreview(result: nil)
    }

    #Preview("Code") {
        CodeShareSheetPreview(
            result: .success(
                BridgeRestoreCodeShare(
                    code: "restore-code-preview-abcdef",
                    qrText: "restore-code-preview-abcdef",
                    qrCarriesS3Key: false
                )
            )
        )
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
