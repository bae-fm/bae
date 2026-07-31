import CoreGraphics
import Foundation
import os.log

#if canImport(AppKit)
    import AppKit
#elseif canImport(UIKit)
    import UIKit
#endif

private let logger = Logger.bae("MediaPaths")

/// Which library image to fetch and how. An `image` is a curated library image —
/// a release cover or an artist portrait — addressed by its full
/// `BridgeImageRef`. A `gallery` slot is read via `fetchReleaseImageBytes`,
/// which takes the whole `BridgeGallerySource` and dispatches the read in core —
/// the UI never picks the byte source. `cacheId` is the gallery item's stable
/// list identity, the decode-cache key, not a fetch decision.
public enum LibraryImageSource: Equatable, Hashable, Sendable {
    case image(BridgeImageRef)
    case gallery(
        releaseId: String,
        source: BridgeGallerySource,
        cacheId: String
    )

    /// Stable identity for the decode cache: a curated image keys on its kind +
    /// subject id + content version (so replacing a cover reloads), a gallery
    /// slot on its release + item id (the cover slot's item id is the constant
    /// `"cover"`, so the release scope keeps two releases' covers distinct in
    /// the shared cache).
    public var cacheToken: String {
        switch self {
        case .image(let image):
            "image:\(image.imageType):\(image.id):\(image.version)"
        case .gallery(let releaseId, _, let cacheId):
            "gallery:\(releaseId):\(cacheId)"
        }
    }
}

/// Image and file path resolution against the live library. Wraps the narrow
/// subset of `AppHandle` that views need to render artwork and resolve the one
/// remaining local file path (the DiscID external-file read) — separated out so
/// leaves can take this instead of the full `AppService` god-object.
public final class MediaPaths: Sendable, Observable {
    /// Filesystem path for the user's own external file behind a library file
    /// (the DiscID re-read of a rip's LOG/CUE/audio). NOT for images — library
    /// images are read through `fetchLibraryImageBytes` and
    /// `fetchReleaseImageBytes`.
    public let filePath: @Sendable (_ fileId: String) async throws -> String?
    /// Bytes of a curated library image (a release cover or an artist portrait)
    /// by full image ref, or nil when no such image exists.
    public let fetchLibraryImageBytes:
        @Sendable (_ image: BridgeImageRef) async throws -> Data?
    /// Bytes of one of a release's image-strip slots (its cover or an image
    /// file), dispatched in core on the `BridgeGallerySource` and downloaded
    /// from the release's cloud home (and decrypted) when it isn't on disk here.
    public let fetchReleaseImageBytes:
        @Sendable (_ releaseId: String, _ source: BridgeGallerySource)
            async throws -> Data
    /// Bytes of provider art at a URL, for the desktop import flow's cover
    /// search. Desktop-only; iOS has no import flow and stubs it.
    public let fetchRemoteImageBytes:
        @Sendable (_ url: String) async throws ->
            Data

    /// Decoded library images, keyed by source + content version + pixel size,
    /// so a grid of covers keeps decode and bridge marshalling off the scroll
    /// path. Owned here (one cache shared by every `ImageView` that reads this
    /// `MediaPaths` from the environment).
    private let imageCache = LibraryImageCache()

    public init(
        filePath: @escaping @Sendable (String) async throws -> String? = {
            _ in throw MediaPathsUnavailable()
        },
        fetchLibraryImageBytes:
            @escaping @Sendable (BridgeImageRef) async throws -> Data? = {
                _ in nil
            },
        fetchReleaseImageBytes:
            @escaping @Sendable (String, BridgeGallerySource) async throws ->
            Data = { _, _ in throw MediaPathsUnavailable() },
        fetchRemoteImageBytes:
            @escaping @Sendable (String) async throws -> Data = {
                _ in throw MediaPathsUnavailable()
            }
    ) {
        self.filePath = filePath
        self.fetchLibraryImageBytes = fetchLibraryImageBytes
        self.fetchReleaseImageBytes = fetchReleaseImageBytes
        self.fetchRemoteImageBytes = fetchRemoteImageBytes
    }

    #if !os(iOS)
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                filePath: { try await handle.filePath(fileId: $0) },
                fetchLibraryImageBytes: {
                    try await handle.fetchLibraryImageBytes(image: $0)
                },
                fetchReleaseImageBytes: {
                    try await handle.fetchReleaseImageBytes(
                        releaseId: $0,
                        source: $1
                    )
                },
                fetchRemoteImageBytes: {
                    try await handle.fetchRemoteImageBytes(url: $0).bytes
                }
            )
        }
    #else
        // `fetchRemoteImageBytes` (the provider-art search) backs the desktop
        // import flow and is absent from the iOS bindings. This wires the
        // path/library/release reads iOS makes; the rest keeps its default.
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                filePath: { try await handle.filePath(fileId: $0) },
                fetchLibraryImageBytes: {
                    try await handle.fetchLibraryImageBytes(image: $0)
                },
                fetchReleaseImageBytes: {
                    try await handle.fetchReleaseImageBytes(
                        releaseId: $0,
                        source: $1
                    )
                }
            )
        }
    #endif

    #if DEBUG
        // periphery:ignore
        /// All-no-op instance for SwiftUI previews. Returns nil paths/bytes;
        /// previews don't have a live library to read from.
        public static let stub = MediaPaths()
    #endif

    /// Cache key for a decoded library image. The pixel size pins the decode
    /// resolution so the now-playing bar's 48pt decode never serves the detail
    /// view's 400pt slot, and vice versa.
    private func cacheKey(
        _ source: LibraryImageSource,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) -> String {
        let pixelSize = Int((pointSize * displayScale).rounded())
        return "\(source.cacheToken)#\(pixelSize)"
    }

    /// The already-decoded image for `source`, or nil when it isn't cached.
    /// Synchronous, so a view can render the image on its very first frame
    /// after (re)mounting — an async load that lands mid-animation inserts the
    /// image leaf with no prior position, which snaps it to its final place
    /// while everything around it is still animating.
    public func cachedLibraryImage(
        _ source: LibraryImageSource,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) -> PlatformImage? {
        imageCache.image(
            for: cacheKey(
                source,
                pointSize: pointSize,
                displayScale: displayScale
            )
        )
    }

    /// Decoded library image for `source` at `pointSize`, reading from the cache
    /// when present and otherwise fetching the bytes (an image ref or a gallery
    /// slot via the bridge), decoding off the main thread, and caching the
    /// result. Returns nil when no such image exists; a fetch/decode error
    /// surfaces, not masked.
    public func libraryImage(
        _ source: LibraryImageSource,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) async throws -> PlatformImage? {
        let key = cacheKey(
            source,
            pointSize: pointSize,
            displayScale: displayScale
        )
        if let cached = imageCache.image(for: key) {
            return cached
        }
        let bytes: Data
        switch source {
        case .image(let image):
            guard let data = try await fetchLibraryImageBytes(image) else {
                logger.warning(
                    "Missing library image bytes for \(image.imageType) \(image.id)"
                )
                return nil
            }
            bytes = data
        case .gallery(let releaseId, let gallerySource, _):
            bytes = try await fetchReleaseImageBytes(releaseId, gallerySource)
        }
        let image = try await ImageLoader.load(
            source: .data(bytes),
            size: .fitTo(points: pointSize),
            displayScale: displayScale
        )
        imageCache.store(image, for: key)
        return image
    }
}

/// A `MediaPaths` capability that isn't wired in this context — the preview
/// stub, or a desktop-only closure (`fetchRemoteImageBytes`) iOS never calls.
/// Throwing surfaces misuse instead of masking it with empty bytes.
public struct MediaPathsUnavailable: Error {
    public init() {}
}

/// In-memory cache of decoded library images. The key pins the content version
/// (so replacing a cover reloads) and the pixel size (so the now-playing bar's
/// 48pt decode never serves the detail view's 400pt slot, and vice versa).
///
/// `@unchecked Sendable` over `NSCache`: `PlatformImage` (NSImage/UIImage) is
/// documented thread-safe for reads after construction, and `NSCache` is
/// synchronized internally.
final class LibraryImageCache: @unchecked Sendable {
    private static let bytesPerPixel = 4

    #if os(iOS)
        private static let defaultTotalCostLimit = 64 * 1024 * 1024
    #else
        private static let defaultTotalCostLimit = 256 * 1024 * 1024
    #endif

    private let cache: NSCache<NSString, PlatformImage>

    init(totalCostLimit: Int = LibraryImageCache.defaultTotalCostLimit) {
        let cache = NSCache<NSString, PlatformImage>()
        cache.totalCostLimit = totalCostLimit
        self.cache = cache
    }

    func image(for key: String) -> PlatformImage? {
        cache.object(forKey: key as NSString)
    }

    func store(_ image: PlatformImage, for key: String) {
        cache.setObject(
            image,
            forKey: key as NSString,
            cost: Self.decodedByteCost(of: image)
        )
    }

    static func decodedByteCost(of image: PlatformImage) -> Int {
        #if canImport(AppKit)
            var proposedRect = CGRect(origin: .zero, size: image.size)
            if let cgImage = image.cgImage(
                forProposedRect: &proposedRect,
                context: nil,
                hints: nil
            ) {
                return decodedByteCost(of: cgImage)
            }
        #elseif canImport(UIKit)
            if let cgImage = image.cgImage {
                return decodedByteCost(of: cgImage)
            }
        #endif

        preconditionFailure("library image cache stores decoded images")
    }

    private static func decodedByteCost(of cgImage: CGImage) -> Int {
        decodedByteCost(width: cgImage.width, height: cgImage.height)
    }

    private static func decodedByteCost(width: Int, height: Int) -> Int {
        guard width > 0, height > 0 else {
            preconditionFailure("decoded image dimensions must be positive")
        }

        let (pixels, pixelsOverflow) =
            width.multipliedReportingOverflow(by: height)
        let (bytes, bytesOverflow) =
            pixels.multipliedReportingOverflow(by: bytesPerPixel)
        if pixelsOverflow || bytesOverflow {
            preconditionFailure("decoded image byte cost overflow")
        }
        return bytes
    }
}
