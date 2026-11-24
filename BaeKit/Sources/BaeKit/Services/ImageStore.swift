import CoreGraphics
import Foundation
import Observation
import os.log

#if canImport(AppKit)
    import AppKit
#elseif canImport(UIKit)
    import UIKit
#endif

private let logger = Logger.bae("ImageStore")

/// What an image slot shows, and therefore where its bytes come from. Every
/// image in the app is one of these five; a view names the content and renders
/// whatever `ImageStore` hands back. Each names the identity its bytes have, so
/// its decodes are cached under it.
public enum ImageContent: Equatable, Hashable, Sendable {
    /// A curated library image — a release cover or an artist portrait — read
    /// by its versioned reference.
    case libraryImage(BridgeImageRef)
    /// One slot of a release's image strip: its cover, or one of the release's
    /// own image files. Core dispatches the read on the `BridgeGallerySource`,
    /// so the UI never picks the byte source itself.
    case releaseImage(releaseId: String, source: BridgeGallerySource)
    /// Provider art (Cover Art Archive, Discogs) that isn't in the library:
    /// the original and every downscaled copy its catalog serves. Fetched
    /// through core, which owns every socket the app opens and reads the copy
    /// the slot's pixel size needs — no view names a thumbnail, so none can
    /// draw one too small for it.
    case remote(BridgeRemoteImageSet)
    /// A file on disk the user is previewing before it enters the library — an
    /// import candidate's cover or folder image — at the version core's scan
    /// read. A file replaced at the same path is a new version, so it is new
    /// content; nothing here reads the file to find that out, and core reads
    /// it for every decode.
    case localFile(BridgeFileVersion)
    /// Artwork embedded in an audio file's tags, as core stored it in the
    /// candidate's tag snapshot, with the version of that file core read it
    /// from — the identity of these bytes. A list row names the artwork and
    /// carries none of it: the bytes are fetched, by candidate and source
    /// file, when a slot draws it.
    case embeddedCover(
        source: BridgeFileVersion,
        candidateKey: Data,
        sourceFileId: String
    )
    /// A person's avatar, at the version their profile names: a new picture
    /// is a new version, so it is new content.
    case avatar(person: String, version: String)

    /// The render content for a cover the import flow draws: a candidate file
    /// on disk, artwork its tags carry, provider art at a URL, or the cover of
    /// the library release imported from the folder.
    public init(bridge: BridgeCoverImageSource) {
        switch bridge {
        case .library(let image):
            self = .libraryImage(image)
        case .releaseImage(let releaseId, let source):
            self = .releaseImage(releaseId: releaseId, source: source)
        case .local(let file):
            self = .localFile(file)
        case .remote(let image):
            self = .remote(image)
        case .embedded(let source, let candidateKey, let sourceFileId):
            self = .embeddedCover(
                source: source,
                candidateKey: candidateKey,
                sourceFileId: sourceFileId
            )
        }
    }

    /// Human-readable description for failure logs.
    public var description: String {
        switch self {
        case .libraryImage(let image):
            return "library image: \(image.imageType) \(image.id)"
        case .releaseImage(let releaseId, _):
            return "release image: \(releaseId)"
        case .remote(let image):
            return "remote image: \(image.url)"
        case .localFile(let file):
            return "image at path: \(file.url.path)"
        case .embeddedCover(let source, _, _):
            return "image embedded in: \(source.url.path)"
        case .avatar(let person, _):
            return "avatar of: \(person)"
        }
    }
}

/// The app's image pipeline: bytes → decode at the slot's pixel size on the
/// decode queue → one bounded decoded cache → synchronous first-frame read. One instance per running
/// library, read from the SwiftUI environment; views hold no fetch, cache, or
/// decode logic of their own.
///
/// A decode is keyed by the content reference the caller supplied and its pixel
/// size. Content at one library, release, or remote reference is stable for the
/// lifetime of an image in bae; a workflow that replaces it supplies a new
/// reference. A local file's reference is the version core's scan read — its
/// path, size, and modification time — so a file replaced at the same path
/// arrives as a new reference once core rescans it, and the store never reads
/// a file to decide what it holds.
public final class ImageStore: Sendable, Observable {
    /// Bytes of a curated library image, or nil when no such image exists.
    private let fetchLibraryImageBytes:
        @Sendable (_ image: BridgeImageRef) async throws -> Data?
    /// Bytes of one of a release's image-strip slots, downloaded from the
    /// release's storage (and decrypted) when it isn't on disk here.
    private let fetchReleaseImageBytes:
        @Sendable (_ releaseId: String, _ source: BridgeGallerySource)
            async throws -> Data
    /// Bytes of provider art for a slot `pixels` wide on its longer side (nil:
    /// the original), or nil when the source serves no image there — cover
    /// addresses are derived from a release's ids, so an offered one can hold
    /// nothing. Core picks the copy.
    private let fetchRemoteImage:
        @Sendable (_ image: BridgeRemoteImageSet, _ pixels: UInt32?)
            async throws -> Data?
    /// Bytes of one of an import candidate's pictures: a copy core made at
    /// the step covering `pixels`, or the original's own bytes when `pixels`
    /// is nil. Core reads the file, once and off the app's threads, and
    /// stops when the call is cancelled and no other call wants the picture.
    private let fetchPicture:
        @Sendable (
            _ source: BridgeCoverImageSource,
            _ pixels: UInt32?,
            _ turn: BridgePictureTurn
        ) async throws -> Data
    /// Bytes of a person's avatar, or nil when they have none.
    private let fetchAvatarBytes:
        @Sendable (_ person: String) async throws -> Data?

    private let cache: DecodedImageCache
    private let decodeQueue: ImageDecodeQueue
    private let inFlightLoads = InFlightImageLoads()

    /// What the decoded cache holds at most, in decoded bytes, for every kind
    /// of content together.
    private static let decodedByteBudget = 256 * 1024 * 1024

    #if DEBUG
        /// Posted after each image enters the decoded cache, for a test to
        /// wait on.
        public static let didStoreImage = DecodedImageCache.didStore

        private let asked = UnansweredCount()

        /// How many images `image(_:pointSize:displayScale:)` was asked for
        /// and has not yet handed back, decoded or failed, for a screenshot
        /// that waits until the art it shows is there. Read under
        /// observation tracking.
        public var unansweredImages: Int { asked.value }
    #endif

    public init(
        fetchLibraryImageBytes:
            @escaping @Sendable (BridgeImageRef) async throws -> Data? = {
                _ in nil
            },
        fetchReleaseImageBytes:
            @escaping @Sendable (String, BridgeGallerySource) async throws ->
            Data = { _, _ in throw ImageStoreUnavailable() },
        fetchRemoteImage:
            @escaping @Sendable (BridgeRemoteImageSet, UInt32?) async throws ->
            Data? = { _, _ in throw ImageStoreUnavailable() },
        fetchPicture:
            @escaping @Sendable (
                BridgeCoverImageSource,
                UInt32?,
                BridgePictureTurn
            ) async throws -> Data = { _, _, _ in
                throw ImageStoreUnavailable()
            },
        fetchAvatarBytes:
            @escaping @Sendable (String) async throws -> Data? = { _ in nil },
        decodeQueue: ImageDecodeQueue = ImageDecodeQueue()
    ) {
        self.fetchLibraryImageBytes = fetchLibraryImageBytes
        self.fetchReleaseImageBytes = fetchReleaseImageBytes
        self.fetchRemoteImage = fetchRemoteImage
        self.fetchPicture = fetchPicture
        self.fetchAvatarBytes = fetchAvatarBytes
        cache = DecodedImageCache(totalCostLimit: Self.decodedByteBudget)
        self.decodeQueue = decodeQueue
    }

    // The provider-art fetch is absent from the iOS bindings.
    #if os(macOS)
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
                        let bytes = try await handle.fetchRemoteImageBytes(
                            image: $0,
                            pixels: $1
                        )
                    else {
                        return nil
                    }
                    return Data(bytes)
                },
                fetchPicture: {
                    Data(
                        try await handle.picture(
                            source: $0,
                            pixels: $1,
                            turn: $2
                        )
                    )
                },
                fetchAvatarBytes: {
                    try await handle.avatar(person: $0).map { Data($0) }
                }
            )
        }
    #endif

    #if DEBUG
        // periphery:ignore
        /// All-no-op instance for shots. Resolves no bytes; shots
        /// don't have a live library to read from.
        public static func stub() -> ImageStore { ImageStore() }
    #endif

    /// The already-decoded image for `content` at this size, or nil when it
    /// isn't cached. Synchronous, so a view can draw the real art on its very
    /// first frame after (re)mounting — an async load that lands mid-animation
    /// inserts the image leaf with no prior position, which snaps it to its
    /// final place while everything around it is still animating.
    ///
    /// A memory-only lookup: it reads no file.
    public func cachedImage(
        _ content: ImageContent,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) -> PlatformImage? {
        let key = cacheKey(
            content,
            pointSize: pointSize,
            displayScale: displayScale
        )
        return cache.image(for: key)
    }

    /// Decoded image for `content` at `pointSize`: the cached decode when there
    /// is one, otherwise fetch the bytes, decode them on the decode queue at
    /// `pointSize × displayScale`, and store the result. Requests for the same
    /// content and size share one load, including a decode that is already
    /// running after every earlier request for it was cancelled. Returns nil when no
    /// such image exists (a library image with no bytes); a fetch or decode
    /// failure surfaces, not masked.
    public func image(
        _ content: ImageContent,
        pointSize: CGFloat,
        displayScale: CGFloat,
        turn: BridgePictureTurn = .visible
    ) async throws -> PlatformImage? {
        #if DEBUG
            asked.change(by: 1)
            defer { asked.change(by: -1) }
        #endif
        let key = cacheKey(
            content,
            pointSize: pointSize,
            displayScale: displayScale
        )
        if let cached = cache.image(for: key) {
            return cached
        }

        let waiter = inFlightLoads.join(key) { [self] control in
            if let cached = cache.image(for: key) {
                return cached
            }
            return try await loadImage(
                content,
                pointSize: pointSize,
                displayScale: displayScale,
                turn: turn,
                control: control
            )
        }
        return try await withTaskCancellationHandler {
            do {
                let image = try await waiter.load.task.value
                inFlightLoads.finish(waiter)
                return image
            }
            catch {
                inFlightLoads.finish(waiter)
                throw error
            }
        } onCancel: {
            inFlightLoads.leave(waiter)
        }
    }

    private func loadImage(
        _ content: ImageContent,
        pointSize: CGFloat,
        displayScale: CGFloat,
        turn: BridgePictureTurn,
        control: LoadControl
    ) async throws -> PlatformImage? {
        let pixels = Int((pointSize * displayScale).rounded())
        guard
            let data = try await decodeSource(
                for: content,
                at: .slot(pixels: pixels),
                turn: turn
            )
        else {
            return nil
        }
        let image = try await ImageLoader.load(
            data: data,
            size: .fitTo(points: pointSize),
            displayScale: displayScale,
            queue: decodeQueue,
            shouldStart: { control.beginDecoding() }
        )
        cache.store(
            image,
            for: cacheKey(
                content,
                pointSize: pointSize,
                displayScale: displayScale
            )
        )
        return image
    }

}

extension ImageStore {
    /// `content` decoded at its own resolution, for a viewer that zooms in:
    /// not cached, and nil when no such image exists. Runs on the decode
    /// queue like every other decode.
    public func fullResolutionImage(
        _ content: ImageContent,
        displayScale: CGFloat
    ) async throws -> PlatformImage? {
        guard
            let data = try await decodeSource(
                for: content,
                at: .nativeResolution
            )
        else {
            return nil
        }
        return try await ImageLoader.load(
            data: data,
            size: .native,
            displayScale: displayScale,
            queue: decodeQueue
        )
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
                    displayScale: displayScale,
                    turn: .prefetch
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
                    \(String(reflecting: error))
                    """
                )
            }
        }
    }

    /// `content`'s bytes, fetched for a decode: a copy at the slot's size for
    /// a picture of the import list, the original's own bytes for a viewer.
    /// Nil when no such image exists.
    func decodeSource(
        for content: ImageContent,
        at target: DecodeTarget,
        turn: BridgePictureTurn = .visible
    ) async throws -> Data? {
        switch content {
        case .libraryImage(let image):
            guard let bytes = try await fetchLibraryImageBytes(image) else {
                logger.warning(
                    "Missing library image bytes for \(image.imageType) \(image.id)"
                )
                return nil
            }
            return bytes
        case .releaseImage(let releaseId, let source):
            return try await fetchReleaseImageBytes(releaseId, source)
        case .remote(let image):
            let pixels: UInt32? =
                switch target {
                case .slot(let pixels): UInt32(clamping: pixels)
                case .nativeResolution: nil
                }
            guard let bytes = try await fetchRemoteImage(image, pixels) else {
                logger.debug("No provider art is served at \(image.url)")
                return nil
            }
            return bytes
        case .localFile(let file):
            return try await fetchPicture(
                .local(file: file),
                target.pictureEdge,
                turn
            )
        case .embeddedCover(let source, let candidateKey, let sourceFileId):
            return try await fetchPicture(
                .embedded(
                    source: source,
                    candidateKey: candidateKey,
                    sourceFileId: sourceFileId
                ),
                target.pictureEdge,
                turn
            )
        case .avatar(let person, _):
            return try await fetchAvatarBytes(person)
        }
    }
}

extension ImageStore {
    /// Cache key for a decoded image: its content identity plus the decode
    /// resolution, so the now-playing bar's 48pt decode never serves the detail
    /// view's 400pt slot, and vice versa.
    fileprivate func cacheKey(
        _ content: ImageContent,
        pointSize: CGFloat,
        displayScale: CGFloat
    ) -> String {
        let pixelSize = Int((pointSize * displayScale).rounded())
        return "\(referenceToken(for: content))#\(pixelSize)"
    }

    /// The content reference the caller supplied.
    fileprivate func referenceToken(for content: ImageContent) -> String {
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
        case .remote(let image):
            // The original names the image; the pixel size beside it in the
            // key already separates the copies decoded for different slots.
            return "remote:\(image.url)"
        case .localFile(let file):
            return "local:\(Self.versionToken(file))"
        case .embeddedCover(let source, _, _):
            return "embedded:\(Self.versionToken(source))"
        case .avatar(let person, let version):
            return "avatar:\(person):\(version)"
        }
    }

    fileprivate static func versionToken(_ file: BridgeFileVersion) -> String {
        "\(file.path.base64EncodedString())@\(file.modifiedAtNs):\(file.size)"
    }

    fileprivate static func libraryToken(_ image: BridgeImageRef) -> String {
        "library:\(image.imageType):\(image.id):\(image.version)"
    }
}

extension ImageStore {
    /// The size a decode's bytes are fetched for. It only chooses among the
    /// copies of provider art; every other content has one set of bytes.
    public enum DecodeTarget: Equatable, Sendable {
        /// A slot `pixels` wide on its longer side: the smallest copy that
        /// fills it.
        case slot(pixels: Int)
        /// The image's own resolution, for a viewer that zooms in.
        case nativeResolution

        /// The pixels a request for a picture of the import list names:
        /// the slot's, or none for the original's own bytes.
        var pictureEdge: UInt32? {
            switch self {
            case .slot(let pixels): UInt32(clamping: pixels)
            case .nativeResolution: nil
            }
        }
    }
}

/// A capability this `ImageStore` isn't wired for — the stub. Throwing
/// surfaces the misuse instead of masking it with empty bytes.
public struct ImageStoreUnavailable: Error {
    public init() {}
}

/// How far one load has come, which decides whether the last request to
/// leave it may cancel it. A decode that has started can't be interrupted, so
/// its load stays, and the next request for the same image joins it; a load
/// still fetching or waiting for a decode thread is cancelled and forgotten.
private final class LoadControl: @unchecked Sendable {
    private enum State {
        case waiting
        case decoding
        case cancelled
    }

    private let lock = NSLock()
    private var state = State.waiting

    /// The decode is about to start: false when the load was cancelled first.
    func beginDecoding() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if state == .cancelled {
            return false
        }
        state = .decoding
        return true
    }

    /// Cancel the load unless its decode has started; true when cancelled.
    func cancelUnlessDecoding() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if state == .decoding {
            return false
        }
        state = .cancelled
        return true
    }
}

private final class InFlightImageLoad: @unchecked Sendable {
    let control: LoadControl
    let task: Task<PlatformImage?, Error>
    /// The requests still waiting on this load. Guarded by the owning
    /// `InFlightImageLoads`' lock.
    fileprivate var waiters = 0

    /// `onEnd` runs when the load's task ends, however it ends.
    init(
        operation:
            @escaping @Sendable (LoadControl) async throws ->
            PlatformImage?,
        onEnd: @escaping @Sendable (LoadControl) -> Void
    ) {
        let control = LoadControl()
        self.control = control
        task = Task {
            defer { onEnd(control) }
            return try await operation(control)
        }
    }
}

/// One request's place among those sharing a load. It leaves the load once,
/// whether its request finishes or is cancelled.
private final class InFlightImageWaiter: @unchecked Sendable {
    let load: InFlightImageLoad
    let key: String
    fileprivate var left = false

    init(load: InFlightImageLoad, key: String) {
        self.load = load
        self.key = key
    }
}

/// Requests for one decoded cache entry share the same fetch and decode. A
/// load is forgotten when its task ends, and a later request finds the image
/// in the cache or, after a failure, starts a new load. The load runs while
/// some request still wants it. When the last waiting request is cancelled,
/// the load is cancelled and forgotten if its decode has not started, so a
/// tile scrolled away does not keep its read waiting in core. A load whose
/// decode has started stays, so the next request for it joins that decode.
private final class InFlightImageLoads: @unchecked Sendable {
    private let lock = NSLock()
    private var loads: [String: InFlightImageLoad] = [:]

    func join(
        _ key: String,
        operation:
            @escaping @Sendable (LoadControl) async throws ->
            PlatformImage?
    ) -> InFlightImageWaiter {
        lock.lock()
        defer { lock.unlock() }
        let load: InFlightImageLoad
        if let existing = loads[key] {
            load = existing
        }
        else {
            load = InFlightImageLoad(operation: operation) {
                [self] control in
                ended(key, control: control)
            }
            loads[key] = load
        }
        load.waiters += 1
        return InFlightImageWaiter(load: load, key: key)
    }

    /// The waiter's request has its answer.
    func finish(_ waiter: InFlightImageWaiter) {
        lock.lock()
        defer { lock.unlock() }
        departed(waiter)
    }

    /// The waiter's request was cancelled. The last one to go cancels the
    /// load, unless its decode has started.
    func leave(_ waiter: InFlightImageWaiter) {
        lock.lock()
        defer { lock.unlock() }
        if departed(waiter), waiter.load.waiters == 0,
            waiter.load.control.cancelUnlessDecoding()
        {
            forget(waiter.load, forKey: waiter.key)
            waiter.load.task.cancel()
        }
    }

    /// The task of the load with `control` ended.
    private func ended(_ key: String, control: LoadControl) {
        lock.lock()
        defer { lock.unlock() }
        if loads[key]?.control === control {
            loads.removeValue(forKey: key)
        }
    }

    private func forget(_ load: InFlightImageLoad, forKey key: String) {
        if loads[key] === load {
            loads.removeValue(forKey: key)
        }
    }

    /// Counts the waiter out of its load once; false when it already was.
    @discardableResult
    private func departed(_ waiter: InFlightImageWaiter) -> Bool {
        if waiter.left {
            return false
        }
        waiter.left = true
        waiter.load.waiters -= 1
        return true
    }
}

/// The decoded cache: images by key (the source's key and the pixel size),
/// bounded by their decoded byte cost.
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
        #if DEBUG
            NotificationCenter.default.post(name: Self.didStore, object: nil)
        #endif
    }

    #if DEBUG
        static let didStore = Notification.Name("ImageStore.didStoreImage")
    #endif

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
