import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("RecoveryCode")

/// Generates and shows the library's recovery code, which grants full access
/// to the library; closing the sheet cancels generation.
struct RecoveryCodeView: View {
    let generate: @Sendable () async throws -> String
    let onDismiss: () -> Void

    /// `nil` while generating.
    @State
    private var result: Result<String, Error>?

    var body: some View {
        NavigationStack {
            content
                .padding()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .navigationTitle("Recovery code")
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
        case .success(let code):
            VStack(spacing: ThemeSpace.edge) {
                Spacer()

                CodeShareBlock(
                    code: code,
                    contentDescription: "Recovery code",
                    qrSize: 220
                )

                Text(
                    "Anyone with this code has full access to your library. Keep it secret. Use it only to restore on a new device when you have no other device available."
                )
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

                Spacer()
            }
        case .failure(let error):
            VStack {
                Spacer()
                // `runGenerate` only records failures core gave a line for.
                if let line = error.displayLine {
                    Text(line)
                        .foregroundStyle(Theme.danger)
                        .themeText(.body)
                        .multilineTextAlignment(.center)
                }
                Spacer()
            }
        }
    }

    private func runGenerate() async {
        do {
            // Cancelling this task also cancels the Rust call.
            let code = try await generate()
            try Task.checkCancellation()
            result = .success(code)
        }
        catch is CancellationError {
            logger.debug("recovery code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate recovery code: \(error.localizedDescription)"
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
    RecoveryCodeView(
        generate: { "PREVIEW-RECOVERY-CODE" },
        onDismiss: {}
    )
}
#endif
