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

/// An image a decode made, beside the bitmap it was made from, so no reader
/// meets a decoded image that draws no bitmap.
public final class DecodedImage: @unchecked Sendable {
    /// The platform image to draw.
    public let image: PlatformImage
    /// The bitmap `image` was made from.
    public let bitmap: CGImage

    public init(image: PlatformImage, bitmap: CGImage) {
        self.image = image
        self.bitmap = bitmap
    }
}

public enum ImageLoader {
    public struct DecodeError: Error {}

    /// Decodes the image in `data`, bytes already in memory (core reads files
    /// and hands over what the slot needs), to at most `points ×
    /// displayScale` pixels on its longer side, by the IDCT thumbnail path.
    /// The decode runs on `queue`, never on Swift's cooperative pool, and
    /// produces an image backed by an already-decoded CGImage. Throws
    /// `CancellationError` if the surrounding task is cancelled, or
    /// `admission` says not to start, before the decode starts, or
    /// `DecodeError` if the bytes can't be decoded. A decode that has started
    /// is not interrupted: its image is returned even to a cancelled caller.
    public static func load(
        data: Data,
        points: CGFloat,
        displayScale: CGFloat,
        queue: ImageDecodeQueue,
        admission: DecodeAdmission
    ) async throws -> DecodedImage {
        let decoded = try await queue.run(admission) {
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
        // A viewer reads tiles of the picture the person zoomed into.
        let focused = DecodeAdmission(turn: { .focused })
        let decoded = try await queue.run(focused) {
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
            image: NSImage(
                cgImage: cg,
                size: NSSize(
                    width: CGFloat(cg.width) / displayScale,
                    height: CGFloat(cg.height) / displayScale
                )
            ),
            bitmap: cg
        )
    #elseif canImport(UIKit)
        // Orientation is `.up`: the thumbnail decode already applied any EXIF
        // transform.
        return DecodedImage(
            image: UIImage(cgImage: cg, scale: displayScale, orientation: .up),
            bitmap: cg
        )
    #endif
}
