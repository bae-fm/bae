import CoreGraphics
import Foundation
import os.log

#if canImport(AppKit)
    import AppKit
#elseif canImport(UIKit)
    import UIKit
#endif

private let logger = Logger.bae("MediaPaths")

/// Which library image to fetch and how. A `cover` is a release cover addressed
/// by release id; an `image` is a release cover or artist image with a full
/// `BridgeImageRef`. A `gallery` slot is read via `fetchGalleryBytes`, which
/// takes the whole
/// `BridgeGallerySource` and dispatches the read in core — the UI never picks
/// the byte source. `cacheId` is the gallery item's stable list identity, the
/// decode-cache key, not a fetch decision.
public enum LibraryImageSource: Equatable, Hashable, Sendable {
    case cover(id: String, version: String?)
    case image(BridgeImageRef)
    case gallery(
        releaseId: String,
        source: BridgeGallerySource,
        cacheId: String
    )

    /// Stable identity for the decode cache: a cover keys on its id + content
    /// version (so replacing a cover reloads), a gallery slot on its release +
    /// item id (the cover slot's item id is the constant `"cover"`, so the
    /// release scope keeps two releases' covers distinct in the shared cache).
    public var cacheToken: String {
        switch self {
        case .cover(let id, let version):
            "cover:\(id):\(version ?? id)"
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
    /// images are read through `fetchImageBytes` and `fetchGalleryBytes`.
    public let filePath: @Sendable (_ fileId: String) async throws -> String?
    /// Bytes of a release cover by release id, or nil when no such cover exists.
    public let fetchCoverImageBytes:
        @Sendable (_ releaseId: String) async throws -> Data?
    /// Bytes of a host-provided library image (a cover or an artist image) by
    /// full image ref, or nil when no such image exists.
    public let fetchImageBytes:
        @Sendable (_ image: BridgeImageRef) async throws -> Data?
    /// Bytes of one of a release's gallery slots (its cover or an image file),
    /// dispatched in core on the `BridgeGallerySource` and downloaded from the
    /// release's cloud home (and decrypted) when it isn't on disk here.
    public let fetchGalleryBytes:
        @Sendable (_ releaseId: String, _ source: BridgeGallerySource)
            async throws -> Data
    /// Remote cover-art bytes for the desktop import flow's cover-art search.
    /// Desktop-only; iOS has no import flow and stubs it.
    public let fetchCoverBytes: @Sendable (_ url: String) async throws -> Data

    /// Decoded library images, keyed by source + content version + pixel size,
    /// so a grid of covers keeps decode and bridge marshalling off the scroll
    /// path. Owned here (one cache shared by every `ImageView` that reads this
    /// `MediaPaths` from the environment).
    private let imageCache = LibraryImageCache()

    public init(
        filePath: @escaping @Sendable (String) async throws -> String? = {
            _ in throw MediaPathsUnavailable()
        },
        fetchCoverImageBytes:
            @escaping @Sendable (String) async throws -> Data? = { _ in nil },
        fetchImageBytes:
            @escaping @Sendable (BridgeImageRef) async throws -> Data? = {
                _ in nil
            },
        fetchGalleryBytes:
            @escaping @Sendable (String, BridgeGallerySource) async throws ->
            Data = { _, _ in throw MediaPathsUnavailable() },
        fetchCoverBytes: @escaping @Sendable (String) async throws -> Data = {
            _ in throw MediaPathsUnavailable()
        }
    ) {
        self.filePath = filePath
        self.fetchCoverImageBytes = fetchCoverImageBytes
        self.fetchImageBytes = fetchImageBytes
        self.fetchGalleryBytes = fetchGalleryBytes
        self.fetchCoverBytes = fetchCoverBytes
    }

    #if !os(iOS)
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                filePath: { try await handle.filePath(fileId: $0) },
                fetchCoverImageBytes: {
                    try await handle.fetchCoverImageBytes(releaseId: $0)
                },
                fetchImageBytes: {
                    try await handle.fetchImageBytes(image: $0)
                },
                fetchGalleryBytes: {
                    try await handle.fetchGalleryBytes(
                        releaseId: $0,
                        source: $1
                    )
                },
                fetchCoverBytes: { try await handle.fetchCoverBytes(url: $0) }
            )
        }
    #else
        // `fetchCoverBytes` (remote cover-art search) backs the desktop import
        // flow and is absent from the iOS bindings; iOS also never reads
        // arbitrary library images by ref (`fetchImageBytes`). This wires the
        // path/cover/gallery reads iOS makes; the rest keep their defaults.
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                filePath: { try await handle.filePath(fileId: $0) },
                fetchCoverImageBytes: {
                    try await handle.fetchCoverImageBytes(releaseId: $0)
                },
                fetchGalleryBytes: {
                    try await handle.fetchGalleryBytes(
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

    /// Decoded library image for `source` at `pointSize`, reading from the cache
    /// when present and otherwise fetching the bytes (an image ref or a gallery
    /// slot via the bridge), decoding off the main thread, and caching the
    /// result. Returns nil when no
    /// such image exists (a `cover` with no bytes); a fetch/decode error
    /// surfaces, not masked.
    public func libraryImage(
        _ source: LibraryImageSource,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) async throws -> PlatformImage? {
        // The pixel size pins the decode resolution so the now-playing bar's
        // 48pt decode never serves the detail view's 400pt slot, and vice versa.
        let pixelSize = Int((pointSize * displayScale).rounded())
        let key = "\(source.cacheToken)#\(pixelSize)"
        if let cached = imageCache.image(for: key) {
            return cached
        }
        let bytes: Data
        switch source {
        case .cover(let id, _):
            guard let data = try await fetchCoverImageBytes(id) else {
                logger.warning("Missing cover image bytes for \(id)")
                return nil
            }
            bytes = data
        case .image(let image):
            guard let data = try await fetchImageBytes(image) else {
                logger.warning(
                    "Missing library image bytes for \(image.imageType) \(image.id)"
                )
                return nil
            }
            bytes = data
        case .gallery(let releaseId, let gallerySource, _):
            bytes = try await fetchGalleryBytes(releaseId, gallerySource)
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
/// stub, or a desktop-only closure (`fetchCoverBytes`) that iOS never calls.
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
