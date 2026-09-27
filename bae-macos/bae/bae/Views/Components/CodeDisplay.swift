import BaeKit
import SwiftUI

/// The QR image, selectable code and "Copy code" button for handing a code to
/// another device, as a flat group so the enclosing stack sets the spacing.
struct CodeDisplay: View {
    let code: String
    let qrSize: CGFloat
    /// This device's public-key fingerprint, rendered between the code and
    /// the copy button. The approving device shows the same fingerprint;
    /// matching them confirms the right device is being added.
    let deviceFingerprint: String?

    init(code: String, qrSize: CGFloat, deviceFingerprint: String? = nil) {
        self.code = code
        self.qrSize = qrSize
        self.deviceFingerprint = deviceFingerprint
    }

    var body: some View {
        if let qrImage = QRCode.image(from: code) {
            Image(nsImage: qrImage)
                .interpolation(.none)
                .resizable()
                .scaledToFit()
                .frame(width: qrSize, height: qrSize)
        }

        Text(code)
            .themeText(.mono)
            .lineLimit(1)
            .truncationMode(.middle)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity)

        if let deviceFingerprint {
            Text("This device: \(deviceFingerprint)")
                .themeText(.mono)
                .foregroundStyle(.secondary)
        }

        Button("Copy code") {
            SystemActions.copyToPasteboard(code)
        }
    }
}

#if DEBUG
    #Preview("Code Display") {
        VStack(spacing: 28) {
            // Bare cluster: QR + code + copy button.
            VStack(spacing: 12) {
                CodeDisplay(code: "BAE-4F2A-9C81-7D30", qrSize: 160)
            }
            Divider()
            // With this device's fingerprint line between code and copy button.
            VStack(spacing: 12) {
                CodeDisplay(
                    code: "BAE-4F2A-9C81-7D30",
                    qrSize: 160,
                    deviceFingerprint: "AB12 CD34 EF56 7890"
                )
            }
        }
        .padding(28)
        .frame(width: 320)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
