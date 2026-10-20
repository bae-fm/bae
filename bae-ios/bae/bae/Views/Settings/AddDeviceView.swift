import BaeKit
import SwiftUI
import UIKit
import os.log

private let logger = Logger.bae("AddDevice")

/// Adds another of this person's devices: generates the QR code the new
/// device scans, and the person's recovery code to type there instead;
/// closing the sheet cancels generation.
struct AddDeviceView: View {
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
                .navigationTitle("Add a device")
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
            ScrollView {
                shareContent(share)
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

    private func shareContent(_ share: BridgeRestoreCodeShare) -> some View {
        VStack(spacing: ThemeSpace.edge) {
            if let qrImage = Self.qrImage(share.qrText) {
                Image(uiImage: qrImage)
                    .interpolation(.none)
                    .resizable()
                    .scaledToFit()
                    .frame(width: 220, height: 220)
                    .accessibilityLabel("Your recovery code")
            }

            Text(
                "Scan this with the new device. Or, on the new device, choose “Restore with your code” and type:"
            )
            .themeText(.detail)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)

            Text(share.code)
                .themeText(.mono)
                .textSelection(.enabled)
                .multilineTextAlignment(.center)

            Button("Copy") {
                UIPasteboard.general.string = share.code
            }

            if share.qrCarriesS3Key {
                Text(
                    "The QR code also carries your S3 access key, so a device that scans it won't ask for the key."
                )
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity)
    }

    /// The QR image, or nil if rendering failed and the code stands alone.
    private static func qrImage(_ text: String) -> UIImage? {
        guard let image = QRCode.image(from: text) else {
            logger.warning(
                "no QR image for \(text.count) characters; showing the code only"
            )
            return nil
        }
        return image
    }

    private func runGenerate() async {
        do {
            // Cancelling this task also cancels the Rust call.
            let share = try await generate()
            try Task.checkCancellation()
            result = .success(share)
        }
        catch is CancellationError {
            logger.debug("add-device code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate the add-device code: \(error.localizedDescription)"
            )
            // A failure with no line is a cancellation from core, which leaves
            // the spinner up like the arm above.
            guard DisplayError(error) != nil else { return }
            result = .failure(error)
        }
    }
}

#if DEBUG
#Preview("Code") {
    AddDeviceView(
        generate: {
            BridgeRestoreCodeShare(
                code: PreviewData.recoveryCode,
                qrText: PreviewData.recoveryCode,
                qrCarriesS3Key: false
            )
        },
        onDismiss: {}
    )
}

#Preview("Code with S3 key") {
    AddDeviceView(
        generate: {
            BridgeRestoreCodeShare(
                code: PreviewData.recoveryCode,
                qrText: "\(PreviewData.recoveryCode).s3-key-preview",
                qrCarriesS3Key: true
            )
        },
        onDismiss: {}
    )
}

#Preview("Loading") {
    AddDeviceView(
        generate: {
            try await Task.sleep(for: .seconds(3600))
            throw CancellationError()
        },
        onDismiss: {}
    )
}

#Preview("Failed") {
    AddDeviceView(
        generate: {
            throw BridgeError.Diagnostic(
                category: .config,
                detail: "preview: storage unreachable"
            )
        },
        onDismiss: {}
    )
}
#endif
