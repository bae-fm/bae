import CoreImage.CIFilterBuiltins
import SwiftUI

/// QR-code + textual code + copy button for pairing another device against
/// this library. `result` carries the loading/loaded/error state as
/// `Result<String, Error>?`: `nil` is loading, `.success(code)` shows the
/// code, `.failure(err)` shows the error message. It's a binding because the
/// presenter computes the code off-main after the sheet is already up — the
/// sheet has to re-render when that write lands, not show a stale snapshot.
struct CodeShareSheet: View {
    @Binding
    var result: Result<String, Error>?
    let onDismiss: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Connect another device")
                    .font(.headline)
                Spacer()
                Button("Done") { onDismiss() }
                    .buttonStyle(.borderless)
            }
            .padding()

            Divider()

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

                    if let qrImage = generateQRCode(from: code) {
                        Image(nsImage: qrImage)
                            .interpolation(.none)
                            .resizable()
                            .scaledToFit()
                            .frame(width: 200, height: 200)
                    }

                    Text(code)
                        .font(.system(.caption, design: .monospaced))
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity)

                    Button("Copy Code") {
                        SystemActions.copyToPasteboard(code)
                    }

                    Text(
                        "Scan this QR code or paste the code on another device to sync this library there."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                    Spacer()
                }
                .padding()
            case .failure(let error):
                VStack {
                    Spacer()
                    Text(error.localizedDescription)
                        .foregroundStyle(.red)
                        .font(.callout)
                    Spacer()
                }
                .padding()
            }
        }
        .frame(width: 400, height: 420)
    }

    private func generateQRCode(from string: String) -> NSImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(string.utf8)
        filter.correctionLevel = "M"

        guard let ciImage = filter.outputImage else {
            return nil
        }

        let scale = 10.0
        let scaled = ciImage.transformed(
            by: CGAffineTransform(scaleX: scale, y: scale)
        )

        let rep = NSCIImageRep(ciImage: scaled)
        let nsImage = NSImage(size: rep.size)
        nsImage.addRepresentation(rep)
        return nsImage
    }
}
