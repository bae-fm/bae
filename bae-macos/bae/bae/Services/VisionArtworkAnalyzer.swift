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
/// Every payload and line crosses with the box Vision drew around it, so a
/// surface can show the printed value itself rather than the whole scan.
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

        var barcodes: [BridgeDetectedBarcode] = []
        let barcodeRequest = VNDetectBarcodesRequest { request, _ in
            let observations =
                (request.results as? [VNBarcodeObservation]) ?? []
            barcodes = Self.detectedBarcodes(observations)
        }
        // CDs/LPs use EAN-13 almost universally; UPC-A is EAN-13 with a
        // leading "0". Keep UPC-E for the rare short form. QR/code128 don't
        // appear on music retail packaging and just add noise.
        barcodeRequest.symbologies = [.ean8, .ean13, .upce]

        var textLines: [BridgeRecognizedLine] = []
        let textRequest = VNRecognizeTextRequest { request, _ in
            let observations =
                (request.results as? [VNRecognizedTextObservation]) ?? []
            textLines = observations.compactMap { observation in
                guard let text = observation.topCandidates(1).first?.string
                else { return nil }
                let trimmed = text.trimmingCharacters(
                    in: .whitespacesAndNewlines
                )
                guard trimmed.count >= 3, trimmed.count <= 80 else {
                    return nil
                }
                return BridgeRecognizedLine(
                    text: trimmed,
                    region: Self.region(of: observation.boundingBox)
                )
            }
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

        return BridgeArtworkAnalysis(barcodes: barcodes, textLines: textLines)
    }

    /// Each payload once, at the box it was first seen in, in payload order
    /// so two passes over one image read the same.
    private static func detectedBarcodes(
        _ observations: [VNBarcodeObservation]
    ) -> [BridgeDetectedBarcode] {
        var seen: Set<String> = []
        return
            observations
            .compactMap { observation -> BridgeDetectedBarcode? in
                guard let payload = observation.payloadStringValue,
                    seen.insert(payload).inserted
                else { return nil }
                return BridgeDetectedBarcode(
                    payload: payload,
                    region: region(of: observation.boundingBox)
                )
            }
            .sorted { $0.payload < $1.payload }
    }

    /// Vision's box, whose origin is the image's bottom-left corner, as core
    /// takes it: fractions of the image from its top-left corner.
    private static func region(of box: CGRect) -> BridgeImageRegion {
        BridgeImageRegion(
            x: Float(box.minX),
            y: Float(1 - box.maxY),
            width: Float(box.width),
            height: Float(box.height)
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
