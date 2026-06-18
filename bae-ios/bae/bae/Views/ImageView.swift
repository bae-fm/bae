import SwiftUI
import os.log

private let logger = Logger.bae("ImageView")

/// Renders a cover image from an absolute on-disk path, decoding off the main
/// thread to a point-sized `UIImage` via the shared `ImageLoader`. Shows the
/// theme placeholder while loading or when the file is absent (the cover isn't
/// synced yet). Re-decodes when the `path` changes.
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
        do {
            image = try await ImageLoader.load(
                source: .local(path: path),
                size: .fitTo(points: pointSize),
                displayScale: displayScale
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "Failed to load cover at \(path, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            image = nil
        }
    }
}
