import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("RestoreCode")

/// Generates and shows this person's restore code, which adds their other
/// devices to the library; closing the sheet cancels generation.
struct RestoreCodeView: View {
    let generate: @Sendable () async throws -> BridgeRestoreCodeShare
    let onDismiss: () -> Void

    /// `nil` while generating.
    @State
    private var result: Result<BridgeRestoreCodeShare, Error>?

    var body: some View {
        NavigationStack {
            content
                .padding()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .navigationTitle("Restore code")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { onDismiss() }
                    }
                }
        }
        .task { await runGenerate() }
    }

    @ViewBuilder
    private var content: some View {
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

                CodeShareBlock(
                    code: share.code,
                    qrText: share.qrText,
                    contentDescription: "Restore code",
                    qrSize: 220
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
        case .failure(let error):
            VStack {
                Spacer()
                // `runGenerate` only records failures core gave a line for.
                if let displayed = DisplayError(error) {
                    ErrorDetailDisclosure(error: displayed)
                }
                Spacer()
            }
        }
    }

    private func runGenerate() async {
        do {
            // Cancelling this task also cancels the Rust call.
            let share = try await generate()
            try Task.checkCancellation()
            result = .success(share)
        }
        catch is CancellationError {
            logger.debug("restore code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate restore code: \(error.localizedDescription)"
            )
            // A failure with no line is a cancellation from core, which leaves
            // the spinner up like the arm above.
            guard DisplayError(error) != nil else { return }
            result = .failure(error)
        }
    }
}

#if DEBUG
#Preview {
    RestoreCodeView(
        generate: {
            BridgeRestoreCodeShare(
                code: "PREVIEW-RESTORE-CODE",
                qrText: "PREVIEW-RESTORE-CODE",
                qrCarriesS3Key: false
            )
        },
        onDismiss: {}
    )
}
#endif
