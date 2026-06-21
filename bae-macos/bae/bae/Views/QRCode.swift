import CoreImage.CIFilterBuiltins
import SwiftUI
import os.log

private let logger = Logger.bae("QRCode")

/// Renders a string as a QR-code image. Shared by the device-join,
/// member-invite, and recovery-code surfaces that hand a code to another
/// device by display.
enum QRCode {
    static func image(from string: String) -> NSImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(string.utf8)
        filter.correctionLevel = "M"

        guard let ciImage = filter.outputImage else {
            logger.error(
                "QR generator produced no image for a \(string.count)-char string"
            )
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
