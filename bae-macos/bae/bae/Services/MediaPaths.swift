import CoreGraphics
import Foundation

/// Which library image to fetch and how. A `cover` (a release cover or an
/// artist image) is read by image id via `fetchImageBytes` and cached under its
/// content `version`; a `releaseFile` (a release's own image file) is read by
/// release id + file id via `fetchGalleryImage`, fetched from the release's
/// cloud home when it isn't on disk here. The discriminator that picks the byte
/// source mirrors `BridgeGalleryItem.coverVersion`.
enum LibraryImageSource: Equatable, Hashable, Sendable {
    /// `version` is nil for now-playing/queue covers, which carry only an id;
    /// those cache by id alone, accepted on those single/small surfaces.
    case cover(id: String, version: String?)
    case releaseFile(releaseId: String, fileId: String)

    /// Stable identity for the decode cache: a cover keys on its id + content
    /// version (so replacing a cover reloads), a release-file image on its
    /// release + file id.
    var cacheToken: String {
        switch self {
        case .cover(let id, let version): "cover:\(id):\(version ?? "")"
        case .releaseFile(let releaseId, let fileId):
            "file:\(releaseId):\(fileId)"
        }
    }
}

/// Image and file path resolution against the live library. Wraps the narrow
/// subset of `AppHandle` that views need to render artwork and resolve the one
/// remaining local file path (the DiscID external-file read) — separated out so
/// leaves can take this instead of the full `AppService` god-object.
final class MediaPaths: Sendable, Observable {
    /// Filesystem path for the user's own external file behind a library file
    /// (the DiscID re-read of a rip's LOG/CUE/audio). NOT for images — library
    /// images are read by id through `fetchImageBytes` / `fetchGalleryImage`.
    let filePath: @Sendable (_ fileId: String) throws -> String?
    /// Bytes of a host-provided library image (a cover or an artist image) by
    /// id, or nil when no such image exists.
    let fetchImageBytes: @Sendable (_ imageId: String) async throws -> Data?
    /// Bytes of a release's gallery image (a release-file image), downloaded
    /// from the release's cloud home (and decrypted) when it isn't on disk here.
    let fetchGalleryImage:
        @Sendable (_ releaseId: String, _ fileId: String) async throws -> Data
    /// Remote cover-art bytes for the desktop import flow's cover-art search.
    /// Desktop-only; iOS has no import flow and stubs it.
    let fetchCoverBytes: @Sendable (_ url: String) async throws -> Data

    /// Decoded library images, keyed by source + content version + pixel size,
    /// so a grid of covers keeps decode and bridge marshalling off the scroll
    /// path. Owned here (one cache shared by every `ImageView` that reads this
    /// `MediaPaths` from the environment).
    private let imageCache = LibraryImageCache()

    init(
        filePath: @escaping @Sendable (String) throws -> String? = { _ in nil },
        fetchImageBytes: @escaping @Sendable (String) async throws -> Data? = {
            _ in nil
        },
        fetchGalleryImage:
            @escaping @Sendable (String, String) async throws ->
            Data = { _, _ in throw MediaPathsUnavailable() },
        fetchCoverBytes: @escaping @Sendable (String) async throws -> Data = {
            _ in throw MediaPathsUnavailable()
        }
    ) {
        self.filePath = filePath
        self.fetchImageBytes = fetchImageBytes
        self.fetchGalleryImage = fetchGalleryImage
        self.fetchCoverBytes = fetchCoverBytes
    }

    #if !os(iOS)
        convenience init(handle: any AppHandleProtocol) {
            self.init(
                filePath: { try handle.filePath(fileId: $0) },
                fetchImageBytes: {
                    try await handle.fetchImageBytes(imageId: $0)
                },
                fetchGalleryImage: {
                    try await handle.fetchGalleryImage(
                        releaseId: $0,
                        fileId: $1
                    )
                },
                fetchCoverBytes: { try await handle.fetchCoverBytes(url: $0) }
            )
        }
    #endif

    // periphery:ignore
    /// All-no-op instance for SwiftUI previews. Returns nil paths/bytes;
    /// previews don't have a live library to read from.
    static let stub = MediaPaths()

    /// Decoded library image for `source` at `pointSize`, reading from the cache
    /// when present and otherwise fetching the bytes (by id or release+file id),
    /// decoding off the main thread, and caching the result. Returns nil when no
    /// such image exists (a `cover` with no bytes); a fetch/decode error
    /// surfaces, not masked.
    func libraryImage(
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
            guard let data = try await fetchImageBytes(id) else {
                return nil
            }
            bytes = data
        case .releaseFile(let releaseId, let fileId):
            bytes = try await fetchGalleryImage(releaseId, fileId)
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
struct MediaPathsUnavailable: Error {}

/// In-memory cache of decoded library images. The key pins the content version
/// (so replacing a cover reloads) and the pixel size (so the now-playing bar's
/// 48pt decode never serves the detail view's 400pt slot, and vice versa).
///
/// `@unchecked Sendable` over a lock-guarded dictionary: `PlatformImage`
/// (NSImage/UIImage) is documented thread-safe for reads after construction, so
/// caching and handing back the same instance across tasks is safe — the lock
/// only guards the dictionary itself.
private final class LibraryImageCache: @unchecked Sendable {
    private let lock = NSLock()
    private var entries: [String: PlatformImage] = [:]

    func image(for key: String) -> PlatformImage? {
        lock.withLock { entries[key] }
    }

    func store(_ image: PlatformImage, for key: String) {
        lock.withLock { entries[key] = image }
    }
}
