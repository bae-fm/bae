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

/// An image a decode made, which carries the bitmap it was made from, so no
/// reader meets a decoded image that draws none. Its own initializer is the
/// only one that makes one here: it sets the bitmap, and the initializers the
/// class inherits make no instance.
public final class DecodedImage: PlatformImage, @unchecked Sendable {
    /// The bitmap the image draws.
    public private(set) var bitmap: CGImage!

    #if canImport(AppKit)
        /// `bitmap`, at `size` points.
        convenience init(bitmap: CGImage, size: CGSize) {
            self.init(cgImage: bitmap, size: size)
            self.bitmap = bitmap
        }
    #elseif canImport(UIKit)
        /// `bitmap`, at `scale` pixels to a point.
        convenience init(bitmap: CGImage, scale: CGFloat) {
            self.init(cgImage: bitmap, scale: scale, orientation: .up)
            self.bitmap = bitmap
        }
    #endif
}

public enum ImageLoader {
    public struct DecodeError: Error {}

    /// Decodes the image in `data`, bytes already in memory (core reads files
    /// and hands over what the slot needs), to at most `points ×
    /// displayScale` pixels on its longer side, by the IDCT thumbnail path.
    /// The decode runs on `queue`, never on Swift's cooperative pool, and
    /// produces an image backed by an already-decoded CGImage. Throws
    /// `CancellationError` if the surrounding task is cancelled, or
    /// `shouldStart` answers false, before the decode starts, or
    /// `DecodeError` if the bytes can't be decoded. A decode that has started
    /// is not interrupted: its image is returned even to a cancelled caller.
    public static func load(
        data: Data,
        points: CGFloat,
        displayScale: CGFloat,
        queue: ImageDecodeQueue,
        shouldStart: @escaping @Sendable () -> Bool
    ) async throws -> DecodedImage {
        let decoded = try await queue.run(shouldStart: shouldStart) {
            thumbnail(of: data, points: points, displayScale: displayScale)
        }
        guard let decoded else {
            throw DecodeError()
        }
        return decoded
    }

    /// Decodes the image in `data` at its own resolution, eagerly, for a
    /// viewer that draws it itself: a tile core cut, at most 512 pixels on a
    /// side. Throws as `load` does.
    public static func loadTile(
        data: Data,
        queue: ImageDecodeQueue
    ) async throws -> CGImage {
        let decoded = try await queue.run {
            CGImageSourceCreateWithData(data as CFData, nil)
                .flatMap {
                    // ShouldCacheImmediately forces the bytes-to-bitmap step to
                    // run on the decode queue's thread instead of the first draw.
                    CGImageSourceCreateImageAtIndex(
                        $0,
                        0,
                        [kCGImageSourceShouldCacheImmediately: true]
                            as CFDictionary
                    )
                }
        }
        guard let decoded else {
            throw DecodeError()
        }
        return decoded
    }
}

/// Decodes `data` to an image of at most `points × displayScale`
/// pixels, on the calling thread; nil when the bytes can't be decoded.
private func thumbnail(
    of data: Data,
    points: CGFloat,
    displayScale: CGFloat
) -> DecodedImage? {
    // ShouldCacheImmediately forces the bytes-to-bitmap step to run on the
    // decode queue's thread instead of being deferred until first draw on
    // the main thread.
    let options: [CFString: Any] = [
        kCGImageSourceThumbnailMaxPixelSize: Int(points * displayScale),
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceShouldCacheImmediately: true,
    ]
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
        let cg = CGImageSourceCreateThumbnailAtIndex(
            source,
            0,
            options as CFDictionary
        )
    else {
        return nil
    }
    // The platform image's point size is the pixel size divided by
    // `displayScale`.
    #if canImport(AppKit)
        return DecodedImage(
            bitmap: cg,
            size: NSSize(
                width: CGFloat(cg.width) / displayScale,
                height: CGFloat(cg.height) / displayScale
            )
        )
    #elseif canImport(UIKit)
        // Orientation is `.up`: the thumbnail decode already applied any EXIF
        // transform.
        return DecodedImage(bitmap: cg, scale: displayScale)
    #endif
}
