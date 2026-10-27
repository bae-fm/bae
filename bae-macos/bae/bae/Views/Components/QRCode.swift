import BaeKit
import CoreImage.CIFilterBuiltins
import SwiftUI
import os.log

private let logger = Logger.bae("QRCode")

/// A string drawn as a QR code for another device's camera to read: the
/// device-join and in-person invite codes. A code carrying an S3 location with
/// detected settings and a key reaches QR version 20, so it is drawn 360 points
/// wide at the low error-correction level, whose fewer, larger modules a phone
/// camera resolves from 25 to 30 cm, inside the four-module quiet zone the QR
/// standard requires.
struct QRCode: View {
    let text: String

    static let side: CGFloat = 360

    var body: some View {
        if let image = Self.image(for: text) {
            // One pixel per module, scaled up without smoothing so module
            // edges stay sharp.
            Image(nsImage: image)
                .interpolation(.none)
                .resizable()
                .frame(width: Self.side, height: Self.side)
        }
    }

    private static let context = CIContext()

    /// The generator's own quiet zone is one module wide.
    private static let generatorQuietZone = 1
    private static let quietZone = 4

    private static func image(for text: String) -> NSImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "L"

        guard let symbol = filter.outputImage else {
            logger.error(
                "QR generator produced no image for a \(text.utf8.count)-byte payload"
            )
            return nil
        }

        let modules = Int(symbol.extent.width) - 2 * generatorQuietZone
        logger.info(
            "QR for a \(text.utf8.count)-byte payload: version \((modules - 17) / 4), \(modules) modules, correction L"
        )

        let margin = CGFloat(quietZone - generatorQuietZone)
        let bounds = symbol.extent.insetBy(dx: -margin, dy: -margin)
        let framed = symbol.composited(
            over: CIImage(color: .white).cropped(to: bounds)
        )
        guard let cgImage = context.createCGImage(framed, from: bounds) else {
            logger.error(
                "Core Image could not render the QR code for a \(text.utf8.count)-byte payload"
            )
            return nil
        }
        return NSImage(
            cgImage: cgImage,
            size: NSSize(width: bounds.width, height: bounds.height)
        )
    }
}
