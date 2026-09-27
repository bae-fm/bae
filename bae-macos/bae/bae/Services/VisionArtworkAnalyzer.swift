import BaeKit
import CoreImage
import ImageIO
import Vision
import os.log

private let logger = Logger.bae("VisionArtworkAnalyzer")

/// Implements the core `ArtworkAnalyzerCallback` with Apple Vision: one
/// `analyze` pass decodes and prepares the image once, then runs barcode
/// detection and text recognition against it in a single synchronous
/// `perform`. Rust calls it from `spawn_blocking`, one call per image.
final class VisionArtworkAnalyzer: ArtworkAnalyzerCallback {
    /// An image whose long side is under this many pixels is enlarged to it
    /// before Vision reads it. A sweep of one Vision pass per setting over a
    /// real library's 1,639 artwork scans chose this: enlarging to 800 with
    /// Lanczos and sharpening read 148 verified barcodes and catalog numbers
    /// against 145 at the stored size unsharpened — among them a catalog
    /// number on a 636 px back cover that only reads enlarged — while larger
    /// targets (1000–2000) read no more and cost more Vision time per pass.
    static let enlargedLongSide: CGFloat = 800
    /// The unsharp mask every image gets before Vision reads it, enlarged or
    /// not — the same sweep's best setting (radius 1, intensity 0.8); radius
    /// 2 read no more.
    static let unsharpRadius: Double = 1.0
    static let unsharpIntensity: Double = 0.8

    /// Renders the prepared image. Core Image converts to the context's
    /// sRGB output, so a grayscale or wide-gamut scan reaches Vision as sRGB
    /// RGBA — as it did in the sweep.
    private let ciContext = CIContext()

    func analyze(path: String) -> BridgeArtworkAnalysis {
        guard let decoded = decode(path: path) else {
            return BridgeArtworkAnalysis(barcodes: [], textLines: [])
        }
        guard let cgImage = prepared(decoded.image) else {
            logger.error("analyze could not prepare \(path) for reading")
            return BridgeArtworkAnalysis(barcodes: [], textLines: [])
        }

        var barcodes: [String] = []
        let barcodeRequest = VNDetectBarcodesRequest { request, _ in
            let observations =
                (request.results as? [VNBarcodeObservation]) ?? []
            barcodes = Self.detectedBarcodes(observations)
        }
        // CDs/LPs use EAN-13 almost universally; UPC-A is EAN-13 with a
        // leading "0". Keep UPC-E for the rare short form. QR/code128 don't
        // appear on music retail packaging and just add noise.
        barcodeRequest.symbologies = [.ean8, .ean13, .upce]

        var textLines: [String] = []
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
                return trimmed
            }
        }
        textRequest.recognitionLevel = .accurate
        textRequest.automaticallyDetectsLanguage = true
        // No language correction: the codes this pass is for — catalog
        // numbers, the digits under a barcode — are not words, and correction
        // rewrites them toward words ("FSR-CD 322" read as "FSA-CO 322"). It
        // is also the configuration the preparation above was measured with.
        // Artist and album text gives up dictionary correction for it.
        textRequest.usesLanguageCorrection = false

        let handler = VNImageRequestHandler(
            cgImage: cgImage,
            orientation: decoded.orientation,
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

    /// Each payload once, sorted, so two passes over one image read the same.
    private static func detectedBarcodes(
        _ observations: [VNBarcodeObservation]
    ) -> [String] {
        Set(observations.compactMap(\.payloadStringValue)).sorted()
    }

    /// The stored image as ImageIO decodes it, with the orientation its
    /// EXIF names so Vision reads it upright.
    private func decode(
        path: String
    ) -> (image: CGImage, orientation: CGImagePropertyOrientation)? {
        let url = URL(fileURLWithPath: path)
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else {
            return nil
        }
        let properties =
            CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
            as? [CFString: Any]
        let orientation =
            (properties?[kCGImagePropertyOrientation] as? UInt32)
            .flatMap(CGImagePropertyOrientation.init(rawValue:)) ?? .up
        return (image, orientation)
    }

    /// The image Vision reads: enlarged with Lanczos when its long side is
    /// under `enlargedLongSide`, then unsharp-masked. Rendered after each
    /// step, as the sweep that chose these settings did, so what Vision sees
    /// is what was measured. Nothing is written to disk.
    private func prepared(_ image: CGImage) -> CGImage? {
        let longSide = CGFloat(max(image.width, image.height))
        var working = image
        if longSide < Self.enlargedLongSide {
            let enlarged = CIImage(cgImage: image)
                .applyingFilter(
                    "CILanczosScaleTransform",
                    parameters: [
                        kCIInputScaleKey: Self.enlargedLongSide / longSide,
                        kCIInputAspectRatioKey: 1.0,
                    ]
                )
            guard let rendered = render(enlarged, in: enlarged.extent.integral)
            else {
                return nil
            }
            working = rendered
        }
        let input = CIImage(cgImage: working)
        let sharpened = input.applyingFilter(
            "CIUnsharpMask",
            parameters: [
                kCIInputRadiusKey: Self.unsharpRadius,
                kCIInputIntensityKey: Self.unsharpIntensity,
            ]
        )
        return render(sharpened, in: input.extent)
    }

    private func render(_ image: CIImage, in extent: CGRect) -> CGImage? {
        ciContext.createCGImage(image.cropped(to: extent), from: extent)
    }
}
