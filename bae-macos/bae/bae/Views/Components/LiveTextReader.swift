import AppKit
import OSLog
import VisionKit

private let logger = Logger.bae("LiveTextReader")

/// Reads the text of a picture for Live Text: the lightbox's picture fitted
/// to the stage, and once it is zoomed, what is on screen.
@MainActor
final class LiveTextReader {
    private let analyzer = ImageAnalyzer()

    /// Live Text's reading of `image`, or nil when it failed, which is logged,
    /// or was cancelled.
    func read(_ image: CGImage) async -> ImageAnalysis? {
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
}
