import AppKit

/// PNG bytes of a black image, for a test that feeds a fetch closure a
/// picture to decode.
func makePngBytes(width: Int, height: Int) throws -> Data {
    struct NoPng: Error {}
    guard
        let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
    else { throw NoPng() }
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    guard let cgImage = context.makeImage(),
        let png = NSBitmapImageRep(cgImage: cgImage)
            .representation(using: .png, properties: [:])
    else { throw NoPng() }
    return png
}
