import SwiftUI

/// QR image + selectable monospaced code + "Copy code" button — the cluster
/// every surface that hands a code to another device renders. Emitted as a
/// flat group (no stack of its own) so the enclosing stack's spacing applies;
/// the three call sites use different spacings.
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
            .font(.system(.caption, design: .monospaced))
            .lineLimit(1)
            .truncationMode(.middle)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity)

        if let deviceFingerprint {
            Text("This device: \(deviceFingerprint)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }

        Button("Copy code") {
            SystemActions.copyToPasteboard(code)
        }
    }
}
