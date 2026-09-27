import BaeKit
import SwiftUI
import UIKit
import os.log

private let logger = Logger.bae("CodeShareBlock")

/// Shows a code as a QR image and selectable text, with a button to copy it.
struct CodeShareBlock: View {
    let code: String
    /// Describes what the QR encodes, for VoiceOver (the image itself is opaque).
    let contentDescription: LocalizedStringKey
    /// Side of the square QR image.
    var qrSize: CGFloat = 200

    /// The QR image, or nil if rendering failed and the text stands alone.
    private var qrImage: UIImage? {
        guard let image = QRCode.image(from: code) else {
            logger.warning("no QR image for a \(code.count)-char code; showing text only")
            return nil
        }
        return image
    }

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            if let qrImage {
                Image(uiImage: qrImage)
                    .interpolation(.none)
                    .resizable()
                    .scaledToFit()
                    .frame(width: qrSize, height: qrSize)
                    .accessibilityLabel(contentDescription)
            }

            Text(code)
                .themeText(.mono)
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
