import AppKit
import BaeKit
import CryptoKit
import Foundation

@testable import bae

extension PreviewData {
    /// A temporary PNG for a fixture that supplies its own image name.
    static func previewArtPath(_ name: String) -> String {
        // swiftlint:disable:next force_try
        return try! writeGradient(named: name).path
    }

    static func writeGradient(named name: String) throws -> URL {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("bae-preview-art", isDirectory: true)
        let data = BoardArtwork.gradient(named: name)
        let digest = SHA256.hash(data: data).prefix(8)
            .map { String(format: "%02x", $0) }
            .joined()
        let file =
            directory
            .appendingPathComponent(
                name.replacingOccurrences(of: "/", with: "-")
                    + "-\(digest)"
            )
            .appendingPathExtension("png")
        if !FileManager.default.fileExists(atPath: file.path) {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            try data.write(to: file, options: .atomic)
        }
        return file
    }

    /// `previewArtPath`'s file at the version on disk, as core's scan
    /// reports a folder's image.
    static func previewArtFile(_ name: String) -> BridgeFileVersion {
        // swiftlint:disable:next force_try
        return try! artVersion(at: URL(fileURLWithPath: previewArtPath(name)))
    }

    static func artVersion(at url: URL) throws -> BridgeFileVersion {
        let attributes = try FileManager.default.attributesOfItem(
            atPath: url.path
        )
        guard let size = attributes[.size] as? UInt64,
            let modified = attributes[.modificationDate] as? Date
        else {
            throw CocoaError(
                .fileReadCorruptFile,
                userInfo: [NSURLErrorKey: url]
            )
        }
        return BridgeFileVersion(
            path: url.pathBytes,
            size: size,
            modifiedAtNs: Int64(
                (modified.timeIntervalSince1970 * 1_000_000_000).rounded()
            )
        )
    }

    /// An image store that loads fixture images from their local paths.
    static func artImageStore() -> ImageStore {
        ImageStore(
            fetchRemoteImage: { image, _ in
                try Data(contentsOf: URL(fileURLWithPath: image.url))
            }
        )
    }
}
