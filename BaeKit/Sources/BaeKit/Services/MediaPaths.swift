import Foundation

/// Filesystem path resolution against the live library. Wraps the one path read
/// views need — separated out so leaves can take this instead of the full
/// `AppService` god-object. Images are not paths: they load through
/// `ImageStore`, by reference or URL.
public final class MediaPaths: Sendable, Observable {
    /// Filesystem path for the user's own external file behind a library file
    /// (the DiscID re-read of a rip's LOG/CUE/audio).
    public let filePath: @Sendable (_ fileId: String) async throws -> String?

    public init(
        filePath: @escaping @Sendable (String) async throws -> String? = {
            _ in throw MediaPathsUnavailable()
        }
    ) {
        self.filePath = filePath
    }

    public convenience init(handle: any AppHandleProtocol) {
        self.init(filePath: { try await handle.filePath(fileId: $0) })
    }

    #if DEBUG
        // periphery:ignore
        /// All-no-op instance for SwiftUI previews. Previews don't have a live
        /// library to resolve paths against.
        public static func stub() -> MediaPaths { MediaPaths() }
    #endif
}

/// A `MediaPaths` capability that isn't wired in this context — the preview
/// stub. Throwing surfaces misuse instead of masking it with a missing path.
public struct MediaPathsUnavailable: Error {
    public init() {}
}
