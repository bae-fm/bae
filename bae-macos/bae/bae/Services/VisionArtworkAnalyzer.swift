import AppKit
import BaeKit
import Vision
import os.log

private let logger = Logger.bae("VisionArtworkAnalyzer")

/// Implements the core `ArtworkAnalyzerCallback` with Apple Vision: one
/// `analyze` pass loads the image once and runs `VNDetectBarcodesRequest`
/// (identify's barcode signal) and `VNRecognizeTextRequest` (the text signal)
/// against it in a single `perform`. Synchronous — `perform` blocks until the
/// completion handlers fire.
///
/// Rust calls this from `tokio::task::spawn_blocking`, so a slow Vision
/// pass won't park the async runtime and never touches Swift's cooperative
/// pool. No caching layer here: the extraction service makes one call per
/// image and re-runs are rare.
final class VisionArtworkAnalyzer: ArtworkAnalyzerCallback {
    func analyze(path: String) -> BridgeArtworkAnalysis {
        guard let cgImage = loadCGImage(path: path) else {
            return BridgeArtworkAnalysis(barcodes: [], textLines: [])
        }

        var payloads: [String] = []
        let barcodeRequest = VNDetectBarcodesRequest { request, _ in
            let observations =
                (request.results as? [VNBarcodeObservation]) ?? []
            payloads = observations.compactMap(\.payloadStringValue)
        }
        // CDs/LPs use EAN-13 almost universally; UPC-A is EAN-13 with a
        // leading "0". Keep UPC-E for the rare short form. QR/code128 don't
        // appear on music retail packaging and just add noise.
        barcodeRequest.symbologies = [.ean8, .ean13, .upce]

        var textLines: [String] = []
        let textRequest = VNRecognizeTextRequest { request, _ in
            let observations =
                (request.results as? [VNRecognizedTextObservation]) ?? []
            textLines =
                observations
                .compactMap { $0.topCandidates(1).first?.string }
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                .filter { $0.count >= 3 && $0.count <= 80 }
        }
        textRequest.recognitionLevel = .accurate
        textRequest.automaticallyDetectsLanguage = true
        // Leave `usesLanguageCorrection` at the default: catalog numbers sit
        // inside substrings and the core classifier's regex pulls them out
        // regardless of minor corrections to surrounding words.

        let handler = VNImageRequestHandler(
            cgImage: cgImage,
            orientation: .up,
            options: [:]
        )
        do {
            try handler.perform([barcodeRequest, textRequest])
        }
        catch {
            logger.error(
                "analyze perform failed for \(path): \(error.localizedDescription)"
            )
            return BridgeArtworkAnalysis(barcodes: [], textLines: [])
        }

        return BridgeArtworkAnalysis(
            barcodes: Array(Set(payloads)).sorted(),
            textLines: textLines
        )
    }

    private func loadCGImage(path: String) -> CGImage? {
        guard let nsImage = NSImage(contentsOfFile: path),
            let cgImage = nsImage.cgImage(
                forProposedRect: nil,
                context: nil,
                hints: nil
            )
        else {
            return nil
        }
        return cgImage
    }
}
