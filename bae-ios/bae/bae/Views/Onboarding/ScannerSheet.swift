import BaeKit
import SwiftUI

/// A full-screen QR scanner with a close button, for recovery and pairing
/// codes.
struct ScannerSheet: View {
    let onScanned: (String) -> Void
    let onError: (String) -> Void
    let onClose: () -> Void

    var body: some View {
        ZStack(alignment: .topTrailing) {
            QRScannerView(
                onScanned: onScanned,
                onError: onError
            )
            .ignoresSafeArea()
            Button {
                onClose()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .themeIcon(.large)
                    .foregroundStyle(Theme.onFill)
                    .padding()
            }
        }
    }
}

#if DEBUG
#Preview {
    ScannerSheet(onScanned: { _ in }, onError: { _ in }, onClose: {})
}
#endif
