import AppKit
import BaeKit
import OSLog
import Observation
import VisionKit

private let logger = Logger.bae("LightboxImageLoad")

/// The image a lightbox shows, as it loads: its bytes found and decoded to
/// fit the stage, then read for Live Text, or failed. Whoever shows the
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
        case failed
    }

    /// The image the last load was for; nil before the first and once the
    /// lightbox closes.
    private(set) var content: ImageContent?
    private(set) var phase = Phase.loading
    /// The image decoded at its own resolution, once a zoom asked for it.
    private(set) var fullResolution: NSImage?

    /// Where the image's bytes are, kept so the decode at full resolution
    /// never crosses the bridge again.
    @ObservationIgnored
    private var source: ImageLoader.Source?
    /// Moves with each load and each close, so a load that was replaced
    /// changes nothing when its work comes back.
    @ObservationIgnored
    private var generation = 0
    @ObservationIgnored
    private var upgrade: Task<Void, Never>?
    @ObservationIgnored
    private let analyzer = ImageAnalyzer()

    /// Load `image` from `images`, decoded to fit `points` on its longer
    /// side, then read it for Live Text. Replaces any load before it.
    func load(
        _ image: ImageContent,
        from images: ImageStore,
        fitting points: CGFloat,
        displayScale: CGFloat
    ) async {
        let current = restart(with: image)
        guard
            let source = await findSource(of: image, in: images, for: current)
        else { return }
        self.source = source
        guard
            let decoded = await decode(
                image,
                from: source,
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

    /// Where `image`'s bytes are, for load `current`: nil once the load
    /// failed, was replaced or was cancelled.
    private func findSource(
        of image: ImageContent,
        in images: ImageStore,
        for current: Int
    ) async -> ImageLoader.Source? {
        do {
            guard
                let source = try await images.decodeSource(
                    for: image,
                    at: .nativeResolution
                )
            else {
                logger.warning(
                    "No bytes for lightbox image \(image.description)"
                )
                fail(current)
                return nil
            }
            return isCurrent(current) ? source : nil
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "Failed to fetch lightbox image \(image.description): \(error)"
            )
            fail(current)
            return nil
        }
    }

    /// `image` decoded from `source` to fit `points`, for load `current`:
    /// nil once the load failed, was replaced or was cancelled.
    private func decode(
        _ image: ImageContent,
        from source: ImageLoader.Source,
        fitting points: CGFloat,
        displayScale: CGFloat,
        for current: Int
    ) async -> NSImage? {
        do {
            let decoded = try await ImageLoader.load(
                source: source,
                size: .fitTo(points: points),
                displayScale: displayScale
            )
            return isCurrent(current) ? decoded : nil
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "Failed to decode lightbox image \(image.description): \(error)"
            )
            fail(current)
            return nil
        }
    }

    /// Live Text's reading of `image`, or nil when it failed (logged) or
    /// was cancelled.
    private func readText(of image: NSImage) async -> ImageAnalysis? {
        do {
            return try await analyzer.analyze(
                image,
                orientation: .up,
                configuration: ImageAnalyzer.Configuration([.text])
            )
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.debug("Live Text analysis failed: \(error)")
            return nil
        }
    }

    /// Decode the image at its own resolution, for a zoom: once per image,
    /// and again after a decode that failed.
    func loadFullResolution(displayScale: CGFloat) {
        guard fullResolution == nil, upgrade == nil, let source else { return }
        let current = generation
        upgrade = Task {
            defer {
                if generation == current { upgrade = nil }
            }
            let loaded: NSImage
            do {
                loaded = try await ImageLoader.load(
                    source: source,
                    size: .native,
                    displayScale: displayScale
                )
            }
            catch is CancellationError {
                return
            }
            catch {
                logger.warning("Failed to load full-res image: \(error)")
                return
            }
            guard isCurrent(current) else { return }
            fullResolution = loaded
        }
    }

    /// The lightbox closed: nothing shown, and no load still running
    /// changes that.
    func close() {
        restart(with: nil)
    }

    @discardableResult
    private func restart(with image: ImageContent?) -> Int {
        generation += 1
        upgrade?.cancel()
        upgrade = nil
        content = image
        phase = .loading
        fullResolution = nil
        source = nil
        return generation
    }

    private func fail(_ load: Int) {
        guard isCurrent(load) else { return }
        phase = .failed
    }

    private func isCurrent(_ load: Int) -> Bool {
        load == generation && !Task.isCancelled
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
