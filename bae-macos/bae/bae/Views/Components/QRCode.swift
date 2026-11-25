import BaeKit
import CoreImage.CIFilterBuiltins
import SwiftUI
import os.log

private let logger = Logger.bae("QRCode")

/// A string drawn as a QR code for another device's camera to read: the
/// device-join and invite codes. It is drawn at the low error-correction
/// level, whose fewer, larger modules a phone camera resolves from 25 to 30 cm,
/// inside the four-module quiet zone the QR standard requires, and as wide as
/// its modules need: every module is `minimumModuleSide` points, so the side
/// follows what the code holds. A code carrying an S3 location with detected
/// settings and a key reaches QR version 20, 97 modules and the quiet zone's 8,
/// and is drawn 360 points wide; a short pairing code stays small. A caller
/// never gives a side, because a side chosen for one payload is too small for
/// a larger one.
struct QRCode: View {
    let text: String

    /// The smallest a module is drawn, in points: the 360 points a version 20
    /// code (105 modules with its quiet zone) was measured to need for a phone
    /// camera to resolve it from 25 to 30 cm.
    static let minimumModuleSide: CGFloat = 360 / 105

    @Environment(\.displayScale)
    private var displayScale

    var body: some View {
        if let modules = Self.modules(for: text) {
            // Each module is a rectangle on the device's pixel grid, so its
            // edges stay sharp however a screenshot or the display resamples
            // an image.
            let side = CGFloat(modules.side)
            Canvas { context, _ in
                context.fill(
                    Path(
                        CGRect(
                            x: 0,
                            y: 0,
                            width: side * Self.minimumModuleSide,
                            height: side * Self.minimumModuleSide
                        )
                    ),
                    with: .color(.white)
                )
                let scale = displayScale
                func edge(_ index: Int) -> CGFloat {
                    (CGFloat(index) * Self.minimumModuleSide * scale).rounded()
                        / scale
                }
                var path = Path()
                for row in 0..<modules.side {
                    for column in 0..<modules.side
                    where modules.isDark[row * modules.side + column] {
                        path.addRect(
                            CGRect(
                                x: edge(column),
                                y: edge(row),
                                width: edge(column + 1) - edge(column),
                                height: edge(row + 1) - edge(row)
                            )
                        )
                    }
                }
                context.fill(path, with: .color(.black))
            }
            .frame(
                width: side * Self.minimumModuleSide,
                height: side * Self.minimumModuleSide
            )
            // Assistive technology reads the drawn code as one image.
            .accessibilityElement()
            .accessibilityAddTraits(.isImage)
        }
    }

    private static let context = CIContext()

    /// The generator's own quiet zone is one module wide.
    private static let generatorQuietZone = 1
    private static let quietZone = 4

    /// A code's modules, one flag for each, quiet zone included: a square
    /// `side` modules wide, row by row from the top.
    struct Modules {
        let side: Int
        let isDark: [Bool]
    }

    static func modules(for text: String) -> Modules? {
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
        // One gray byte for each module: the bounds are square, and the
        // generator's symbol is one pixel for each module.
        let side = Int(bounds.width)
        var gray = [UInt8](repeating: 0, count: side * side)
        context.render(
            framed,
            toBitmap: &gray,
            rowBytes: side,
            bounds: bounds,
            format: .L8,
            colorSpace: CGColorSpaceCreateDeviceGray()
        )
        return Modules(side: side, isDark: gray.map { $0 < 128 })
    }
}
