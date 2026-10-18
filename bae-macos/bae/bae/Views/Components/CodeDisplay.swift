import BaeKit
import SwiftUI

/// The QR image, selectable code and "Copy code" button for handing a code to
/// another device, as a flat group so the enclosing stack sets the spacing.
struct CodeDisplay: View {
    let code: String
    let qrSize: CGFloat

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

        Button("Copy code") {
            SystemActions.copyToPasteboard(code)
        }
    }
}

#if DEBUG
    #Preview("Code Display") {
        let qrSize: CGFloat = 160
        VStack(spacing: ThemeSpace.section) {
            // Bare cluster: QR + code + copy button.
            VStack(spacing: ThemeSpace.group) {
                CodeDisplay(code: "BAE-4F2A-9C81-7D30", qrSize: qrSize)
            }
        }
        .padding(ThemeSpace.section)
        .frame(width: 320)
        .background(Theme.background)
        .preferredColorScheme(.dark)
    }
#endif
