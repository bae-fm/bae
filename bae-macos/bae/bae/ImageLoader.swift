import AppKit
import SwiftUI

enum ImageLoader {
    enum Source: Equatable {
        /// A local image addressed by the bridge's cache-bustable identifier
        /// (`<path>#v=<mtime>`, or a bare path when no version is stamped). The
        /// whole identifier is the cache key; the file is opened at
        /// `MediaPaths.fileSystemPath(of:)` with the version stripped.
        case local(path: String)
        case remote(url: String)

        /// Human-readable description for failure logs.
        var description: String {
            switch self {
            case .local(let path):
                return "image at path: \(path)"
            case .remote(let url):
                return "remote image: \(url)"
            }
        }
    }

    /// Decode strategy for `load`. `.fitTo` takes the IDCT thumbnail
    /// path, decoding at most `points × displayScale` pixels. `.native`
    /// decodes the source at its full pixel resolution, eagerly, so
    /// the returned NSImage is ready to draw without main-thread
    /// decode work.
    enum Size {
        case fitTo(points: CGFloat)
        case native
    }

    struct DecodeError: Error {}

    /// Loads an image for `source` at the given `size`. Local sources
    /// read directly from disk; remote sources call `fetchRemoteBytes`
    /// to pull the raw bytes (production: `MediaPaths.fetchCoverBytes`;
    /// previews: the stub closure). The decode runs on a background
    /// task and produces an NSImage backed by an already-decoded
    /// CGImage. Throws `CancellationError` if the surrounding task is
    /// cancelled, or `DecodeError` if the source can't be opened or
    /// decoded.
    static func load(
        source: Source,
        size: Size,
        displayScale: CGFloat,
        fetchRemoteBytes: @Sendable (_ url: String) async throws -> Data
    ) async throws -> NSImage {
        switch source {
        case .local(let identifier):
            // Open the bare file path, not the cache-busting identifier: the
            // `#v=<mtime>` suffix is the cache key, not part of the filename.
            let path = MediaPaths.fileSystemPath(of: identifier)
            return try await decodeAsNSImage(displayScale: displayScale) {
                let url = URL(fileURLWithPath: path) as CFURL
                guard let cgSource = CGImageSourceCreateWithURL(url, nil)
                else {
                    return nil
                }
                return decodeCGImage(
                    from: cgSource,
                    size: size,
                    displayScale: displayScale
                )
            }
        case .remote(let url):
            let bytes = try await fetchRemoteBytes(url)
            return try await decodeAsNSImage(displayScale: displayScale) {
                guard
                    let cgSource = CGImageSourceCreateWithData(
                        bytes as CFData,
                        nil
                    )
                else {
                    return nil
                }
                return decodeCGImage(
                    from: cgSource,
                    size: size,
                    displayScale: displayScale
                )
            }
        }
    }
}

/// Decodes `source` to a CGImage at the resolution implied by `size`.
private func decodeCGImage(
    from source: CGImageSource,
    size: ImageLoader.Size,
    displayScale: CGFloat
) -> CGImage? {
    // ShouldCacheImmediately forces the bytes-to-bitmap step to run on
    // the calling task's thread instead of being deferred until first
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

/// Runs `decoder` on a background task, wraps the resulting CGImage in
/// an NSImage with point-sized dimensions (pixel dims divided by
/// `displayScale`), and propagates cancellation after the decode
/// completes. Throws `ImageLoader.DecodeError` if the decoder returns
/// nil.
private func decodeAsNSImage(
    displayScale: CGFloat,
    decoder: @Sendable @escaping () -> CGImage?
) async throws -> NSImage {
    let loaded: NSImage? =
        await Task.detached(priority: .userInitiated) {
            guard let cg = decoder() else {
                return nil
            }
            return NSImage(
                cgImage: cg,
                size: NSSize(
                    width: CGFloat(cg.width) / displayScale,
                    height: CGFloat(cg.height) / displayScale
                )
            )
        }
        .value
    try Task.checkCancellation()
    guard let loaded else {
        throw ImageLoader.DecodeError()
    }
    return loaded
}
