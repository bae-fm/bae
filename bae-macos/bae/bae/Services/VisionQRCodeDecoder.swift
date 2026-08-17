import CoreVideo
import Vision

/// Decodes the QR payload in one camera frame. `AVCaptureMetadataOutput` does
/// not expose QR detection for every macOS camera, while Vision accepts the
/// video pixel buffer directly.
enum VisionQRCodeDecoder {
    static func payload(in pixelBuffer: CVPixelBuffer) throws -> String? {
        let request = VNDetectBarcodesRequest()
        request.symbologies = [.qr]

        let handler = VNImageRequestHandler(
            cvPixelBuffer: pixelBuffer,
            orientation: .up,
            options: [:]
        )
        try handler.perform([request])
        return request.results?.first?.payloadStringValue
    }
}
