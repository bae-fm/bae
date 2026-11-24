import CoreGraphics
import Foundation
import ImageIO

#if canImport(AppKit)
    import AppKit

    /// The platform's image type: `NSImage` on macOS, `UIImage` on iOS. Decode
    /// is identical on both (ImageIO / CoreGraphics); only the final wrapper
    /// differs.
    public typealias PlatformImage = NSImage
#elseif canImport(UIKit)
    import UIKit

    public typealias PlatformImage = UIImage
#endif

public enum ImageLoader {
    /// What a decode reads from. Fetching is `ImageStore`'s job — by the time
    /// bytes reach here they are either on disk or in memory.
    public enum Source: Equatable, Sendable {
        /// A file already on disk. Streamed rather than read whole, so a huge
        /// image never sits in memory twice.
        case local(URL)
        /// Image bytes already in memory — fetched through the bridge, or held
        /// by the caller. Decoded at the requested size without any fetch.
        case data(Data)

        /// Human-readable description for failure logs.
        public var description: String {
            switch self {
            case .local(let url):
                return "image at path: \(url.path)"
            case .data(let bytes):
                return "in-memory image: \(bytes.count) bytes"
            }
        }
    }

    /// Decode strategy for `load`. `.fitTo` takes the IDCT thumbnail
    /// path, decoding at most `points × displayScale` pixels. `.native`
    /// decodes the source at its full pixel resolution, eagerly, so
    /// the returned image is ready to draw without main-thread
    /// decode work. The lightbox loads `.fitTo` for the screen-fit view
    /// and upgrades to `.native` only when the user zooms in, so a huge
    /// (e.g. 35 MB) JPEG never decodes at full resolution until needed.
    public enum Size: Sendable {
        case fitTo(points: CGFloat)
        case native
    }

    public struct DecodeError: Error {}

    /// Loads an image for `source` at the given `size`. Local sources read
    /// directly from disk; `.data` sources decode bytes already in memory. The
    /// decode runs on `queue`, never on Swift's cooperative pool, and produces
    /// an image backed by an already-decoded CGImage. Throws
    /// `CancellationError` if the surrounding task is cancelled, or
    /// `shouldStart` answers false, before the decode starts, or `DecodeError`
    /// if the source can't be opened or decoded. A decode that has started is
    /// not interrupted: its image is returned even to a cancelled caller.
    public static func load(
        source: Source,
        size: Size,
        displayScale: CGFloat,
        queue: ImageDecodeQueue = .shared,
        shouldStart: @escaping @Sendable () -> Bool = { true }
    ) async throws -> PlatformImage {
        let decoded = try await queue.run(shouldStart: shouldStart) {
            decode(source: source, size: size, displayScale: displayScale)
        }
        guard let decoded else {
            throw DecodeError()
        }
        return decoded
    }
}

/// Decodes `source` to a platform image at the requested size, on the calling
/// thread; nil when the source can't be opened or decoded.
private func decode(
    source: ImageLoader.Source,
    size: ImageLoader.Size,
    displayScale: CGFloat
) -> PlatformImage? {
    let cgSource: CGImageSource?
    switch source {
    case .local(let url):
        cgSource = CGImageSourceCreateWithURL(url as CFURL, nil)
    case .data(let bytes):
        cgSource = CGImageSourceCreateWithData(bytes as CFData, nil)
    }
    guard let cgSource,
        let cg = decodeCGImage(
            from: cgSource,
            size: size,
            displayScale: displayScale
        )
    else {
        return nil
    }
    // The platform image's point size is the pixel size divided by
    // `displayScale`.
    #if canImport(AppKit)
        return NSImage(
            cgImage: cg,
            size: NSSize(
                width: CGFloat(cg.width) / displayScale,
                height: CGFloat(cg.height) / displayScale
            )
        )
    #elseif canImport(UIKit)
        // Orientation is `.up`: the thumbnail decode already applied any EXIF
        // transform.
        return UIImage(cgImage: cg, scale: displayScale, orientation: .up)
    #endif
}

/// Decodes `source` to a CGImage at the resolution implied by `size`.
private func decodeCGImage(
    from source: CGImageSource,
    size: ImageLoader.Size,
    displayScale: CGFloat
) -> CGImage? {
    // ShouldCacheImmediately forces the bytes-to-bitmap step to run on
    // the decode queue's thread instead of being deferred until first
    // draw on the main thread. It defaults to true for the thumbnail
    // call and false for CreateImageAtIndex, but we set it explicitly
    // in both branches so the policy is visible at the callsite.
    switch size {
    case .fitTo(let points):
        let options: [CFString: Any] = [
            kCGImageSourceThumbnailMaxPixelSize: Int(points * displayScale),
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        return CGImageSourceCreateThumbnailAtIndex(
            source,
            0,
            options as CFDictionary
        )
    case .native:
        let options: [CFString: Any] = [
            kCGImageSourceShouldCacheImmediately: true
        ]
        return CGImageSourceCreateImageAtIndex(
            source,
            0,
            options as CFDictionary
        )
    }
}
