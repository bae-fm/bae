import Foundation

/// Image and file path resolution against the live library. Wraps the
/// narrow subset of `AppHandle` that views need to render artwork and
/// resolve local file paths — separated out so leaves can take this
/// instead of the full `AppService` god-object.
final class MediaPaths: Sendable, Observable {
    let imagePathIfExists: @Sendable (_ imageId: String) -> String?
    let filePath: @Sendable (_ fileId: String) throws -> String?
    let fetchCoverBytes: @Sendable (_ url: String) async throws -> Data

    init(
        imagePathIfExists: @escaping @Sendable (String) -> String? = { _ in nil
        },
        filePath: @escaping @Sendable (String) throws -> String? = { _ in nil },
        fetchCoverBytes: @escaping @Sendable (String) async throws -> Data = {
            _ in Data()
        }
    ) {
        self.imagePathIfExists = imagePathIfExists
        self.filePath = filePath
        self.fetchCoverBytes = fetchCoverBytes
    }

    // `fetchCoverBytes` pulls remote cover art for the desktop import flow and
    // isn't exported on iOS. This `handle`-wiring convenience initializer
    // references it, so it's desktop-only; the iOS `AppService` builds
    // `MediaPaths` via the designated initializer (it renders on-disk covers
    // only, so `fetchCoverBytes` stays at its empty-Data stub).
    #if !os(iOS)
        convenience init(handle: any AppHandleProtocol) {
            self.init(
                imagePathIfExists: { handle.imagePathIfExists(imageId: $0) },
                filePath: { try handle.filePath(fileId: $0) },
                fetchCoverBytes: { try await handle.fetchCoverBytes(url: $0) }
            )
        }
    #endif

    /// All-no-op instance for SwiftUI previews. Returns nil paths and
    /// empty cover bytes; previews don't have a live library to read
    /// from.
    // periphery:ignore
    static let stub = MediaPaths()

    /// Separates the on-disk path from its cache-busting version in the
    /// identifier `imagePathIfExists` returns. The bridge stamps
    /// `<path>#v=<mtime>` so the identifier changes when a cover is overwritten
    /// in place (the path never does); the changed identifier flows into
    /// `.task(id:)` so the view reloads. Mirrors `VERSION_SEPARATOR` in
    /// bae-core's `versioned_image_path`.
    static let versionSeparator = "#v="

    /// The bare on-disk path for an `imagePathIfExists` identifier: everything
    /// before the `#v=<mtime>` cache-busting suffix (the whole string when
    /// there's no suffix). Open the file at this path — the suffix is the cache
    /// key, not part of the filename.
    static func fileSystemPath(of identifier: String) -> String {
        guard let separator = identifier.range(of: versionSeparator) else {
            return identifier
        }
        return String(identifier[..<separator.lowerBound])
    }
}
