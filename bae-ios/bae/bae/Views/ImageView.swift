import SwiftUI
import os.log

private let logger = Logger.bae("ImageView")

/// Renders a cover image from an absolute on-disk path, decoding off the main
/// thread to a point-sized `UIImage`. Shows the theme placeholder while loading
/// or when the file is absent (the cover isn't synced yet). Re-decodes when the
/// `path` changes.
struct ImageView: View {
    let path: String?
    /// Target point size for the thumbnail; pixels decoded are
    /// `pointSize * displayScale`.
    let pointSize: CGFloat
    var contentMode: ContentMode = .fill

    @Environment(\.displayScale)
    private var displayScale
    @State
    private var image: UIImage?

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .aspectRatio(contentMode: contentMode)
            }
            else {
                Theme.placeholder
            }
        }
        .task(id: path) {
            await load()
        }
    }

    private func load() async {
        guard let path else {
            image = nil
            return
        }
        let maxPixel = Int(pointSize * displayScale)
        let decoded: UIImage? = await Task.detached(priority: .userInitiated) {
            decodeThumbnail(path: path, maxPixelSize: maxPixel)
        }.value
        if Task.isCancelled {
            return
        }
        if decoded == nil {
            logger.warning(
                "Failed to load cover at \(path, privacy: .public)"
            )
        }
        image = decoded
    }
}

/// Decode the image at `identifier` to a `UIImage` whose largest edge is at
/// most `maxPixelSize` pixels, with the bytes-to-bitmap step forced eagerly so
/// the image draws without main-thread decode work. Returns nil if the file
/// can't be opened. `identifier` is the bridge's cache-bustable form
/// (`<path>#v=<mtime>`); the file is opened at the bare path with the version
/// stripped — the suffix only exists to change `.task(id:)`'s key on a cover
/// change.
private func decodeThumbnail(path identifier: String, maxPixelSize: Int) -> UIImage? {
    let url = URL(fileURLWithPath: MediaPaths.fileSystemPath(of: identifier)) as CFURL
    guard let source = CGImageSourceCreateWithURL(url, nil) else {
        return nil
    }
    let options: [CFString: Any] = [
        kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceShouldCacheImmediately: true,
    ]
    guard
        let cgImage = CGImageSourceCreateThumbnailAtIndex(
            source,
            0,
            options as CFDictionary
        )
    else {
        return nil
    }
    return UIImage(cgImage: cgImage)
}
