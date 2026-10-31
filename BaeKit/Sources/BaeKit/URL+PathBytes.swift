import Foundation

extension URL {
    /// The file URL of a path as it crosses the bridge: coven's bytes for it
    /// (`coven::path_bytes`), which on Apple platforms are the path's own
    /// file-system representation.
    public init(filePathBytes bytes: Data, isDirectory: Bool) {
        var terminated = [CChar](repeating: 0, count: bytes.count + 1)
        terminated.withUnsafeMutableBytes { buffer in
            _ = bytes.copyBytes(to: buffer)
        }
        self.init(
            fileURLWithFileSystemRepresentation: terminated,
            isDirectory: isDirectory,
            relativeTo: nil
        )
    }

    /// This file URL's path as it crosses the bridge: its file-system
    /// representation, which is what coven's bytes for it are on Apple
    /// platforms.
    public var pathBytes: Data {
        withUnsafeFileSystemRepresentation { pointer in
            guard let pointer else {
                preconditionFailure(
                    "a file URL has a file-system representation"
                )
            }
            return Data(bytes: pointer, count: strlen(pointer))
        }
    }
}
