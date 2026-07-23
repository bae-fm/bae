import SwiftUI

/// A full-screen QR scanner with a close button, used for both the recovery-code
/// and invite-code scans. The owner decides what a scanned code does and how the
/// sheet dismisses.
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
                    .font(.title)
                    .foregroundStyle(.white)
                    .padding()
            }
        }
    }
}
