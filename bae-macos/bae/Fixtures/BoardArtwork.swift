import AppKit
import BaeKit
import CryptoKit
import Foundation
import Testing

@testable import bae

/// The artwork shared by the board and the native screenshot fixtures.
final class BoardArtwork {
    private static let bundle = Bundle(for: BoardArtwork.self)

    private static func requireURL(_ name: String, extension ext: String) throws
        -> URL
    {
        guard
            let file = bundle.url(
                forResource: name,
                withExtension: ext,
                subdirectory: "Artwork"
            )
        else {
            throw CocoaError(
                .fileNoSuchFile,
                userInfo: [
                    NSFilePathErrorKey: bundle.bundlePath + "/Artwork/" + name
                        + "." + ext
                ]
            )
        }
        return file
    }

    private static let titles = Result {
        let catalog = try Data(
            contentsOf: requireURL("titles", extension: "json")
        )
        return try JSONDecoder().decode(Set<String>.self, from: catalog)
    }

    /// A named board picture comes from the catalog. The board draws other names as gradients.
    private static func bundledURL(named name: String) throws -> URL? {
        guard try titles.get().contains(name) else { return nil }
        return try requireURL(name, extension: "png")
    }

    static func data(named name: String) throws -> Data {
        if let file = try bundledURL(named: name) {
            return try Data(contentsOf: file)
        }
        return gradient(named: name)
    }

    /// The folder scan's local image. A packaging failure fails the capture which asks for it.
    static func scannedFile(named name: String) -> BridgeFileVersion? {
        do {
            let url =
                try bundledURL(named: name)
                ?? URL(fileURLWithPath: PreviewData.previewArtPath(name))
            return try PreviewData.artVersion(at: url)
        }
        catch {
            Issue.record(error)
            return nil
        }
    }

    fileprivate static func gradient(named name: String) -> Data {
        let side: CGFloat = 600
        let hue = name.utf16.reduce(0) { ($0 * 31 + Int($1)) % 360 }
        guard
            let gradient = NSGradient(
                starting: hsl(hue, saturation: 0.42, lightness: 0.52),
                ending: hsl(hue + 48, saturation: 0.46, lightness: 0.24)
            )
        else {
            preconditionFailure("a cover gradient has two colours")
        }
        let image = NSImage(
            size: NSSize(width: side, height: side),
            flipped: false
        ) { rect in
            // From the top left corner to the bottom right one.
            gradient.draw(in: rect, angle: -45)
            return true
        }
        guard
            let tiff = image.tiffRepresentation,
            let bitmap = NSBitmapImageRep(data: tiff),
            let png = bitmap.representation(using: .png, properties: [:])
        else {
            preconditionFailure("placeholder art must encode as PNG")
        }
        return png
    }

    /// The colour CSS's `hsl(hue saturation lightness)` names, with the
    /// hue in degrees.
    private static func hsl(
        _ hue: Int,
        saturation: CGFloat,
        lightness: CGFloat
    ) -> NSColor {
        let brightness = lightness + saturation * min(lightness, 1 - lightness)
        return NSColor(
            hue: CGFloat(hue % 360) / 360,
            saturation: brightness == 0 ? 0 : 2 * (1 - lightness / brightness),
            brightness: brightness,
            alpha: 1
        )
    }

}

extension PreviewData {
    /// A temporary PNG for a fixture that supplies its own image name.
    static func previewArtPath(_ name: String) -> String {
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
            // Crash with the reason rather than draw failure placeholders.
            // swiftlint:disable force_try
            try! FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            try! data.write(to: file)
            // swiftlint:enable force_try
        }
        return file.path
    }

    /// `previewArtPath`'s file at the version on disk, as core's scan
    /// reports a folder's image.
    static func previewArtFile(_ name: String) -> BridgeFileVersion {
        // swiftlint:disable:next force_try
        return try! artVersion(at: URL(fileURLWithPath: previewArtPath(name)))
    }

    fileprivate static func artVersion(at url: URL) throws -> BridgeFileVersion
    {
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
