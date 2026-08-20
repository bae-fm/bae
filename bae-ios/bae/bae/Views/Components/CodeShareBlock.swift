import BaeKit
import SwiftUI
import UIKit
import os.log

private let logger = Logger.bae("CodeShareBlock")

/// Hands a short code to another device by display: its QR rendering, the code
/// as selectable monospaced text, and a button to copy it. Shared by the
/// device-join, member-invite, and recovery-code surfaces, which all present a
/// code the same way.
struct CodeShareBlock: View {
    let code: String
    /// Describes what the QR encodes, for VoiceOver (the image itself is opaque).
    let contentDescription: LocalizedStringKey
    /// Side of the square QR image. The surfaces differ only in how much room
    /// they have for it.
    var qrSize: CGFloat = 200

    /// The QR image, or nil if rendering failed — in which case the code text
    /// alongside is the fallback. Logs at the skip so a missing QR is visible.
    private var qrImage: UIImage? {
        guard let image = QRCode.image(from: code) else {
            logger.warning("no QR image for a \(code.count)-char code; showing text only")
            return nil
        }
        return image
    }

    var body: some View {
        VStack(spacing: 12) {
            if let qrImage {
                Image(uiImage: qrImage)
                    .interpolation(.none)
                    .resizable()
                    .scaledToFit()
                    .frame(width: qrSize, height: qrSize)
                    .accessibilityLabel(contentDescription)
            }

            Text(code)
                .font(.system(.caption, design: .monospaced))
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity)

            Button("Copy code") {
                UIPasteboard.general.string = code
            }
        }
    }
}

#if DEBUG
#Preview {
    // Routed through a `String` so the extractor never takes preview-only copy
    // into the catalog.
    let description = "Preview code"
    CodeShareBlock(
        code: "PREVIEW-CODE",
        contentDescription: LocalizedStringKey(description),
        qrSize: 180
    )
}
#endif
