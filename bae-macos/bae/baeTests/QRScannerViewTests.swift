import CoreImage.CIFilterBuiltins
import CoreVideo
import Testing

@testable import bae

@Suite("QR scanner frame decoding")
struct QRScannerViewTests {
    @Test("a camera pixel buffer containing a QR code yields its payload")
    func decodesQRCodeFromPixelBuffer() throws {
        let payload = "bae-device-invite"
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(payload.utf8)
        let image = try #require(filter.outputImage)
            .transformed(
                by: CGAffineTransform(scaleX: 12, y: 12)
            )

        var storage: CVPixelBuffer?
        let status = CVPixelBufferCreate(
            kCFAllocatorDefault,
            Int(image.extent.width),
            Int(image.extent.height),
            kCVPixelFormatType_32BGRA,
            [kCVPixelBufferIOSurfacePropertiesKey: [:]] as CFDictionary,
            &storage
        )
        #expect(status == kCVReturnSuccess)
        let pixelBuffer = try #require(storage)
        CIContext()
            .render(
                image,
                to: pixelBuffer,
                bounds: image.extent,
                colorSpace: CGColorSpaceCreateDeviceRGB()
            )

        #expect(try VisionQRCodeDecoder.payload(in: pixelBuffer) == payload)
    }
}
