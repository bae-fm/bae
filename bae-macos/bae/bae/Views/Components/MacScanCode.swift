import BaeKit
import SwiftUI

/// A code another device scans, one thing at a time: what to do, in
/// `instruction`; the QR code of `qr`; then Copy code and Share…, which hand
/// out `code` as text and nothing else. The code's text is never shown beside
/// the QR code. `code` is not always what the QR code holds: a QR code can
/// carry what a person typing the text does not need to.
struct MacScanCode: View {
    let instruction: LocalizedStringKey
    let qr: String
    let code: String

    var body: some View {
        VStack(spacing: ThemeSpace.edge) {
            Text(instruction)
                .themeText(.body)
                .multilineTextAlignment(.center)
            QRCode(text: qr)
                .accessibilityLabel("QR code")
            HStack(spacing: ThemeSpace.related) {
                Button("Copy code") {
                    SystemActions.copyToPasteboard(code)
                }
                ShareLink(item: code) {
                    Text("Share…")
                }
            }
        }
        .frame(maxWidth: .infinity)
    }
}
