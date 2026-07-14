import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("RecoveryCode")

/// Reveals the library's recovery code on demand. The recovery code is a bearer
/// credential — anyone holding it gains full access — so it's kept behind a
/// presented sheet and labelled as sensitive, used only to restore on a new
/// device when no existing device is available to approve a join. The `.task`
/// owns the generation lifecycle: it fires on appear and is cancelled
/// automatically when the sheet dismisses, which propagates through to the
/// underlying Rust future since `generate` is a genuine uniffi async call.
/// Presented as a sheet from `SettingsView`.
struct RecoveryCodeView: View {
    let generate: @Sendable () async throws -> String
    let onDismiss: () -> Void

    /// `nil` is loading, `.success(code)` shows the code, `.failure(err)` shows
    /// the error message.
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
            VStack(spacing: 16) {
                Spacer()

                CodeShareBlock(
                    code: code,
                    contentDescription: "Recovery code",
                    qrSize: 220
                )

                Text(
                    "Anyone with this code has full access to your library. Keep it secret. Use it only to restore on a new device when you have no other device available."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

                Spacer()
            }
        case .failure(let error):
            VStack {
                Spacer()
                Text(error.displayLine)
                    .foregroundStyle(.red)
                    .font(.callout)
                    .multilineTextAlignment(.center)
                Spacer()
            }
        }
    }

    private func runGenerate() async {
        do {
            // generateRestoreCode is a genuine uniffi async call: it suspends
            // without blocking a thread, and cancelling this task propagates
            // through to the underlying Rust future.
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
            result = .failure(error)
        }
    }
}
