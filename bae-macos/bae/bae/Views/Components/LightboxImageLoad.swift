import AppKit
import BaeKit
import OSLog
import Observation
import VisionKit

private let logger = Logger.bae("LightboxImageLoad")

/// The image a lightbox shows, as it loads: decoded to fit the stage by the
/// image store, then read for Live Text, or failed. Whoever shows the
/// lightbox owns it, for as long as the lightbox can open there, so what
/// waits for the image watches `phase`.
@MainActor
@Observable
final class LightboxImageLoad {
    enum Phase {
        /// No image yet: its bytes are being found or decoded.
        case loading
        /// Decoded to fit the stage; Live Text is still reading it.
        case decoded(NSImage)
        /// Decoded and read for Live Text. No analysis when the read failed,
        /// which is logged and leaves the image without selectable text.
        case analysed(NSImage, ImageAnalysis?)
        /// No bytes for the image, or they would not read or decode; logged.
        case failed(LightboxFailure)
    }

    /// The image the last load was for; nil before the first and once the
    /// lightbox closes.
    private(set) var content: ImageContent?
    /// Reads text out of the picture and, once it is zoomed, the screen.
    let textReader = LiveTextReader()
    private(set) var phase = Phase.loading

    /// Moves with each load and each close, so a load that was replaced
    /// changes nothing when its work comes back.
    @ObservationIgnored
    private var generation = 0

    /// Load `image` from `images`, decoded to fit `points` on its longer
    /// side, then read it for Live Text. Replaces any load before it.
    func load(
        _ image: ImageContent,
        from images: ImageStore,
        fitting points: CGFloat,
        displayScale: CGFloat
    ) async {
        let current = restart(showing: image)
        guard
            let decoded = await decode(
                image,
                from: images,
                fitting: points,
                displayScale: displayScale,
                for: current
            )
        else { return }
        phase = .decoded(decoded)
        let analysis = await readText(of: decoded)
        guard isCurrent(current) else { return }
        phase = .analysed(decoded, analysis)
    }

    /// `image` as the store decodes it to fit `points`, for load `current`:
    /// nil once the load failed, was replaced or was cancelled.
    private func decode(
        _ image: ImageContent,
        from images: ImageStore,
        fitting points: CGFloat,
        displayScale: CGFloat,
        for current: Int
    ) async -> NSImage? {
        do {
            guard
                let decoded = try await images.image(
                    image,
                    pointSize: points,
                    displayScale: displayScale
                )
            else {
                logger.warning(
                    "No bytes for lightbox image \(image.description)"
                )
                fail(current, .noBytes)
                return nil
            }
            // Every decode makes a bitmap, so an image with none is a bug.
            guard decoded.drawnBitmap != nil else {
                logger.reportBug(
                    operation: "load a lightbox image",
                    detail:
                        "the decoded image of \(image.description) has no bitmap",
                    to: CoreBugReporter()
                )
                fail(current, .bug)
                return nil
            }
            return isCurrent(current) ? decoded : nil
        }
        catch is CancellationError {
            return nil
        }
        catch is ImageLoader.DecodeError {
            // The bytes are not an image this Mac decodes: reading them again
            // gets the same bytes.
            logger.warning(
                "Lightbox image \(image.description) did not decode"
            )
            fail(current, .undecodable)
            return nil
        }
        catch {
            logger.warning(
                "Failed to load lightbox image \(image.description): \(error)"
            )
            fail(
                current,
                LightboxFailure(of: error, operation: "load a lightbox image")
            )
            return nil
        }
    }

    /// Live Text's reading of the decoded `image`, or nil when the read
    /// failed or was cancelled.
    private func readText(of image: NSImage) async -> ImageAnalysis? {
        await textReader.read(image.bitmap)
    }

    /// The lightbox closed: nothing shown, and no load still running
    /// changes that.
    func close() {
        restart(showing: nil)
    }

    @discardableResult
    private func restart(showing content: ImageContent?) -> Int {
        generation += 1
        self.content = content
        phase = .loading
        return generation
    }

    private func fail(_ load: Int, _ failure: LightboxFailure) {
        guard isCurrent(load) else { return }
        phase = .failed(failure)
    }

    private func isCurrent(_ load: Int) -> Bool {
        load == generation && !Task.isCancelled
    }
}

/// Why the lightbox's picture did not load or zoom: what core said, if it
/// said anything to show, and whether trying again can get past it.
struct LightboxFailure: Equatable {
    let error: DisplayError?
    let offersRetry: Bool

    /// Why `operation` failed with `error`: a failure core reported, whose
    /// error says whether trying again can help, or else a bug in the app,
    /// which goes to the bug report and offers no Retry.
    init(of error: any Error, operation: String) {
        guard error is BridgeError else {
            logger.reportBug(
                operation: operation,
                detail: String(reflecting: error),
                to: CoreBugReporter()
            )
            self = .bug
            return
        }
        let shown = DisplayError(error)
        self.error = shown
        offersRetry = shown?.offersRetry ?? false
    }

    private init(offersRetry: Bool) {
        error = nil
        self.offersRetry = offersRetry
    }

    /// The store found no bytes for the image, which another read may find.
    static let noBytes = LightboxFailure(offersRetry: true)
    /// The bytes are not an image this Mac decodes: reading them again gets
    /// the same bytes.
    static let undecodable = LightboxFailure(offersRetry: false)
    /// A bug in the viewer, which goes to the bug report.
    static let bug = LightboxFailure(offersRetry: false)
}

extension LightboxImageLoad {
    /// How the load of `image` stands: loading while the load holds another
    /// image's phase, as it does until the load of this one starts.
    func phase(for image: ImageContent) -> Phase {
        content == image ? phase : .loading
    }
}

extension LightboxImageLoad.Phase {
    /// The image decoded to fit the stage, once there is one.
    var image: NSImage? {
        switch self {
        case .decoded(let image), .analysed(let image, _): image
        case .loading, .failed: nil
        }
    }

    var analysis: ImageAnalysis? {
        if case .analysed(_, let analysis) = self {
            analysis
        }
        else {
            nil
        }
    }
}
