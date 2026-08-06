import CoreGraphics
import Foundation
import os.log

#if canImport(AppKit)
    import AppKit
#elseif canImport(UIKit)
    import UIKit
#endif

private let logger = Logger.bae("ImageStore")

/// What an image slot shows, and therefore where its bytes come from. Every
/// image in the app is one of these five; a view names the content and renders
/// whatever `ImageStore` hands back.
public enum ImageContent: Equatable, Hashable, Sendable {
    /// A curated library image — a release cover or an artist portrait — read
    /// by its versioned reference.
    case libraryImage(BridgeImageRef)
    /// One slot of a release's image strip: its cover, or one of the release's
    /// own image files. Core dispatches the read on the `BridgeGallerySource`,
    /// so the UI never picks the byte source itself.
    case releaseImage(releaseId: String, source: BridgeGallerySource)
    /// Provider art (Cover Art Archive, Discogs) that isn't in the library.
    /// Fetched through core, which owns every socket the app opens.
    case remote(url: String)
    /// A file on disk the user is previewing before it enters the library — an
    /// import candidate's cover or folder image.
    case localFile(path: String)
    /// Bytes already in hand. Decoded on demand and never cached: the caller
    /// holds the only identity these bytes have.
    case bytes(Data)

    /// The render content for a cover the import flow offers: a candidate file
    /// on disk, or provider art at a URL.
    public init(bridge: BridgeCoverImageSource) {
        switch bridge {
        case .local(let path):
            self = .localFile(path: path)
        case .remote(let url):
            self = .remote(url: url)
        }
    }

    /// Human-readable description for failure logs.
    public var description: String {
        switch self {
        case .libraryImage(let image):
            return "library image: \(image.imageType) \(image.id)"
        case .releaseImage(let releaseId, _):
            return "release image: \(releaseId)"
        case .remote(let url):
            return "remote image: \(url)"
        case .localFile(let path):
            return "image at path: \(path)"
        case .bytes(let bytes):
            return "in-memory image: \(bytes.count) bytes"
        }
    }
}

/// Provider-art bytes plus the token identifying this exact content: the
/// response's `ETag`, or a hash of the bytes when it carries none.
///
/// Mirrors the bridge's `BridgeRemoteImage`, which only the desktop bindings
/// export — the fetch behind it is desktop-only, and this store compiles for
/// both platforms. The conversion lives in the desktop `init(handle:)`.
public struct RemoteImageBytes: Equatable, Sendable {
    public let bytes: Data
    public let validator: String

    public init(bytes: Data, validator: String) {
        self.bytes = bytes
        self.validator = validator
    }
}

/// Per-kind byte budgets for the decoded cache. Eviction never crosses buckets,
/// so a native-size release image cannot evict the album grid's covers.
public struct ImageStoreBudgets: Equatable, Sendable {
    public var libraryImage: Int
    public var releaseImage: Int
    public var remote: Int
    public var localFile: Int

    public init(
        libraryImage: Int,
        releaseImage: Int,
        remote: Int,
        localFile: Int
    ) {
        self.libraryImage = libraryImage
        self.releaseImage = releaseImage
        self.remote = remote
        self.localFile = localFile
    }

    private static let megabyte = 1024 * 1024

    // Local files are import candidates and iOS has no import flow, so that
    // bucket takes one number on both platforms — nothing ever enters it there.
    #if os(iOS)
        public static let `default` = ImageStoreBudgets(
            libraryImage: 48 * megabyte,
            releaseImage: 16 * megabyte,
            remote: 8 * megabyte,
            localFile: 16 * megabyte
        )
    #else
        public static let `default` = ImageStoreBudgets(
            libraryImage: 192 * megabyte,
            releaseImage: 48 * megabyte,
            remote: 16 * megabyte,
            localFile: 16 * megabyte
        )
    #endif
}

/// The app's image pipeline: bytes → decode at the slot's pixel size → bounded
/// decoded cache → synchronous first-frame read. One instance per running
/// library, read from the SwiftUI environment; views hold no fetch, cache, or
/// decode logic of their own.
///
/// What a cached decode is pinned to — its token — is the content's identity,
/// so no entry can outlive the bytes it came from: a curated image keys on its
/// `_updated_at` version, a release file on its file id (immutable — an import
/// mints a fresh id per file, and a re-import mints new ones rather than
/// repointing an existing row), provider art on its URL plus the validator core
/// returns with the bytes, and a local file on its path and modification date.
public final class ImageStore: Sendable, Observable {
    /// Bytes of a curated library image, or nil when no such image exists.
    private let fetchLibraryImageBytes:
        @Sendable (_ image: BridgeImageRef) async throws -> Data?
    /// Bytes of one of a release's image-strip slots, downloaded from the
    /// release's cloud home (and decrypted) when it isn't on disk here.
    private let fetchReleaseImageBytes:
        @Sendable (_ releaseId: String, _ source: BridgeGallerySource)
            async throws -> Data
    /// Bytes of provider art at a URL, with the validator identifying them, or
    /// nil when the source serves no image there — cover addresses are derived
    /// from a release's ids, so an offered one can hold nothing.
    /// Desktop-only; iOS has no import flow and leaves it unwired.
    private let fetchRemoteImage:
        @Sendable (_ url: String) async throws -> RemoteImageBytes?

    private let buckets: Buckets
    private let remoteValidators = RemoteValidators()

    public init(
        fetchLibraryImageBytes:
            @escaping @Sendable (BridgeImageRef) async throws -> Data? = {
                _ in nil
            },
        fetchReleaseImageBytes:
            @escaping @Sendable (String, BridgeGallerySource) async throws ->
            Data = { _, _ in throw ImageStoreUnavailable() },
        fetchRemoteImage:
            @escaping @Sendable (String) async throws -> RemoteImageBytes? = {
                _ in throw ImageStoreUnavailable()
            },
        budgets: ImageStoreBudgets = .default
    ) {
        self.fetchLibraryImageBytes = fetchLibraryImageBytes
        self.fetchReleaseImageBytes = fetchReleaseImageBytes
        self.fetchRemoteImage = fetchRemoteImage
        buckets = Buckets(budgets: budgets)
    }

    #if !os(iOS)
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
                fetchLibraryImageBytes: {
                    try await handle.fetchLibraryImageBytes(image: $0)
                },
                fetchReleaseImageBytes: {
                    try await handle.fetchReleaseImageBytes(
                        releaseId: $0,
                        source: $1
                    )
                },
                fetchRemoteImage: {
                    guard
                        let image = try await handle.fetchRemoteImageBytes(
                            url: $0
                        )
                    else {
                        return nil
                    }
                    return RemoteImageBytes(
                        bytes: image.bytes,
                        validator: image.validator
                    )
                }
            )
        }
    #else
        // The provider-art fetch backs the desktop import flow and is absent
        // from the iOS bindings. This wires the library and release reads iOS
        // makes; the remote fetch keeps its throwing default.
        public convenience init(handle: any AppHandleProtocol) {
            self.init(
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
        /// All-no-op instance for SwiftUI previews. Resolves no bytes; previews
        /// don't have a live library to read from.
        public static let stub = ImageStore()
    #endif

    /// The already-decoded image for `content` at this size, or nil when it
    /// isn't cached. Synchronous, so a view can draw the real art on its very
    /// first frame after (re)mounting — an async load that lands mid-animation
    /// inserts the image leaf with no prior position, which snaps it to its
    /// final place while everything around it is still animating.
    ///
    /// The only I/O is the `stat` a local file's modification date needs; every
    /// other kind is a pure cache lookup, and `.bytes` is never cached.
    public func cachedImage(
        _ content: ImageContent,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) -> PlatformImage? {
        guard
            let key = cacheKey(
                content,
                pointSize: pointSize,
                displayScale: displayScale
            )
        else {
            return nil
        }
        return buckets[content.bucket].image(for: key)
    }

    /// Decoded image for `content` at `pointSize`: the cached decode when there
    /// is one, otherwise fetch the bytes, decode off the main thread at
    /// `pointSize × displayScale`, and store the result. Returns nil when no
    /// such image exists (a library image with no bytes); a fetch or decode
    /// failure surfaces, not masked.
    public func image(
        _ content: ImageContent,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) async throws -> PlatformImage? {
        let key = cacheKey(
            content,
            pointSize: pointSize,
            displayScale: displayScale
        )
        if let key, let cached = buckets[content.bucket].image(for: key) {
            return cached
        }
        guard let source = try await decodeSource(for: content) else {
            return nil
        }
        let image = try await ImageLoader.load(
            source: source,
            size: .fitTo(points: pointSize),
            displayScale: displayScale
        )
        // A key is absent only for `.bytes`, whose identity the caller holds —
        // there is nothing to pin a cache entry to.
        if let key {
            buckets[content.bucket].store(image, for: key)
            if case .remote(let url) = content {
                remoteValidators.record(key: key, for: url)
            }
        }
        return image
    }

    /// Decode `contents` at `pointSize` into the cache, so a view that mounts
    /// later draws the art on its first frame instead of a loading placeholder.
    ///
    /// For art the app already knows it is about to show somewhere the user has
    /// not looked yet. Each entry costs a cache lookup once it is warm, and a
    /// failure is logged and skipped rather than failing the rest — warming is
    /// about what a later frame draws, and that frame's own load reports the
    /// failure where it can be seen.
    public func warm(
        _ contents: [ImageContent],
        pointSize: CGFloat,
        displayScale: CGFloat
    ) async {
        for content in contents {
            if cachedImage(
                content,
                pointSize: pointSize,
                displayScale: displayScale
            ) != nil {
                continue
            }
            do {
                _ = try await image(
                    content,
                    pointSize: pointSize,
                    displayScale: displayScale
                )
            }
            catch is CancellationError {
                return
            }
            catch {
                logger.warning(
                    """
                    Failed to warm \
                    \(content.description): \
                    \(error.localizedDescription)
                    """
                )
            }
        }
    }

    /// Where `content`'s bytes come from, resolved for a decode: a path for a
    /// local file (which streams rather than loading whole), the fetched bytes
    /// otherwise. Nil when no such image exists.
    ///
    /// The zoomable viewers take this to re-decode the same bytes at native
    /// resolution without crossing the bridge again.
    public func decodeSource(
        for content: ImageContent
    ) async throws -> ImageLoader.Source? {
        switch content {
        case .libraryImage(let image):
            guard let bytes = try await fetchLibraryImageBytes(image) else {
                logger.warning(
                    "Missing library image bytes for \(image.imageType) \(image.id)"
                )
                return nil
            }
            return .data(bytes)
        case .releaseImage(let releaseId, let source):
            return .data(try await fetchReleaseImageBytes(releaseId, source))
        case .remote(let url):
            guard let fetched = try await fetchRemoteImage(url) else {
                logger.debug("No provider art is served at \(url)")
                return nil
            }
            dropDecodesPredating(validator: fetched.validator, of: url)
            return .data(fetched.bytes)
        case .localFile(let path):
            return .local(path: path)
        case .bytes(let bytes):
            return .data(bytes)
        }
    }

    /// Evict every decode of `url` made from an older validator. Core's byte
    /// cache decides when a URL's bytes are re-read; when they come back
    /// different, the decodes taken from the old ones are stale at *every*
    /// pixel size, not just the one being loaded.
    ///
    /// This runs on a fetch, which is the only moment the store learns the
    /// bytes moved — a size it has already decoded is served from the cache and
    /// asks core nothing.
    private func dropDecodesPredating(validator: String, of url: String) {
        let stale = remoteValidators.adopt(validator: validator, for: url)
        for key in stale {
            buckets[.remote].removeImage(for: key)
        }
    }

    /// Cache key for a decoded image: its content identity plus the decode
    /// resolution, so the now-playing bar's 48pt decode never serves the detail
    /// view's 400pt slot, and vice versa. Nil when the content has no cacheable
    /// identity.
    private func cacheKey(
        _ content: ImageContent,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) -> String? {
        guard let token = token(for: content) else {
            return nil
        }
        let pixelSize = Int((pointSize * displayScale).rounded())
        return "\(token)#\(pixelSize)"
    }

    /// What pins a cached decode to the exact bytes it came from. Nil for
    /// `.bytes` (no identity to pin to) and for a local file whose modification
    /// date can't be read.
    private func token(for content: ImageContent) -> String? {
        switch content {
        case .libraryImage(let image):
            return Self.libraryToken(image)
        case .releaseImage(_, let source):
            switch source {
            case .cover(let image):
                // A cover slot IS the release's curated cover; its version
                // moves whenever the bytes do.
                return Self.libraryToken(image)
            case .releaseFile(let fileId):
                // A release file's bytes are immutable per id: an import mints
                // a fresh id per file and a re-import mints new ones, so an id
                // never comes to name different bytes.
                return "file:\(fileId)"
            }
        case .remote(let url):
            return "remote:\(url)"
        case .localFile(let path):
            guard let modified = Self.modificationDate(ofFileAt: path) else {
                return nil
            }
            return "path:\(path)#\(modified.timeIntervalSince1970)"
        case .bytes:
            return nil
        }
    }

    private static func libraryToken(_ image: BridgeImageRef) -> String {
        "library:\(image.imageType):\(image.id):\(image.version)"
    }

    /// When the file at `path` last changed, or nil when it can't be read — a
    /// candidate the user moved or deleted. Nothing is cached under a date we
    /// don't have; the load path surfaces the read failure itself.
    private static func modificationDate(ofFileAt path: String) -> Date? {
        let attributes: [FileAttributeKey: Any]
        do {
            attributes = try FileManager.default.attributesOfItem(atPath: path)
        }
        catch {
            logger.debug(
                "No modification date for \(path): \(error.localizedDescription)"
            )
            return nil
        }
        guard let modified = attributes[.modificationDate] as? Date else {
            logger.warning("File \(path) reports no modification date")
            return nil
        }
        return modified
    }
}

/// A capability this `ImageStore` isn't wired for — the preview stub, or the
/// desktop-only provider-art fetch on iOS. Throwing surfaces the misuse instead
/// of masking it with empty bytes.
public struct ImageStoreUnavailable: Error {
    public init() {}
}

extension ImageContent {
    /// Which cache the decodes of this content live in. Derived here so callers
    /// never pass a bucket.
    fileprivate var bucket: DecodedImageBucket {
        switch self {
        case .libraryImage: .libraryImage
        case .releaseImage: .releaseImage
        case .remote: .remote
        // `.bytes` is never cached, but every content still names a bucket so
        // the lookup needs no second "is this cacheable" branch — its key is
        // nil, which is what keeps it out.
        case .localFile, .bytes: .localFile
        }
    }
}

/// The decoded cache's four independent budgets, one per content kind.
enum DecodedImageBucket: CaseIterable {
    case libraryImage
    case releaseImage
    case remote
    case localFile
}

/// The four per-kind caches, addressed by bucket.
private struct Buckets: Sendable {
    private let caches: [DecodedImageBucket: DecodedImageCache]

    init(budgets: ImageStoreBudgets) {
        caches = [
            .libraryImage: DecodedImageCache(
                totalCostLimit: budgets.libraryImage
            ),
            .releaseImage: DecodedImageCache(
                totalCostLimit: budgets.releaseImage
            ),
            .remote: DecodedImageCache(totalCostLimit: budgets.remote),
            .localFile: DecodedImageCache(totalCostLimit: budgets.localFile),
        ]
    }

    subscript(bucket: DecodedImageBucket) -> DecodedImageCache {
        guard let cache = caches[bucket] else {
            preconditionFailure("every bucket is built in init")
        }
        return cache
    }
}

/// The validator each remote URL's cached decodes were made from, and the keys
/// they live under. `NSCache` can't enumerate its keys, so the store tracks
/// what it wrote in order to drop a URL's decodes when its bytes change.
private final class RemoteValidators: @unchecked Sendable {
    private struct Entry {
        var validator: String?
        var keys: Set<String>
    }

    private let lock = NSLock()
    private var entries: [String: Entry] = [:]

    /// Note that `url`'s decode at this size lives under `key`.
    func record(key: String, for url: String) {
        lock.lock()
        defer { lock.unlock() }
        entries[url, default: Entry(validator: nil, keys: [])].keys.insert(key)
    }

    /// Adopt the validator a fetch just returned for `url`, and hand back the
    /// keys holding decodes of the previous one. Empty when the validator is
    /// unchanged, or when this is the first fetch for the URL.
    func adopt(validator: String, for url: String) -> Set<String> {
        lock.lock()
        defer { lock.unlock() }
        var entry = entries[url] ?? Entry(validator: nil, keys: [])
        guard let previous = entry.validator, previous != validator else {
            entry.validator = validator
            entries[url] = entry
            return []
        }
        let stale = entry.keys
        entries[url] = Entry(validator: validator, keys: [])
        return stale
    }
}

/// One bucket of the decoded cache: images by key, bounded by their decoded
/// byte cost.
///
/// `@unchecked Sendable` over `NSCache`: `PlatformImage` (NSImage/UIImage) is
/// documented thread-safe for reads after construction, and `NSCache` is
/// synchronized internally.
final class DecodedImageCache: @unchecked Sendable {
    private static let bytesPerPixel = 4

    private let cache: NSCache<NSString, PlatformImage>

    init(totalCostLimit: Int) {
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

    func removeImage(for key: String) {
        cache.removeObject(forKey: key as NSString)
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

        preconditionFailure("the decoded image cache stores decoded images")
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
