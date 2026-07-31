import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("ImageView")

/// Renders a library cover image, fetching its bytes by id through `MediaPaths`
/// (which caches the decoded image by content version) and decoding off the
/// main thread to a point-sized `UIImage`. Shows the theme placeholder while
/// loading or when the cover is absent. Re-decodes when the `source` changes.
struct ImageView: View {
    let source: LibraryImageSource?
    /// Target point size for the thumbnail; pixels decoded are
    /// `pointSize * displayScale`.
    let pointSize: CGFloat
    var contentMode: ContentMode = .fill

    @Environment(MediaPaths.self)
    private var mediaPaths
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
        .task(id: source) {
            await load()
        }
    }

    private func load() async {
        guard let source else {
            image = nil
            return
        }
        do {
            image = try await mediaPaths.libraryImage(
                source,
                pointSize: pointSize,
                displayScale: displayScale
            )
        }
        catch is CancellationError {
            logger.debug("cover load cancelled: \(String(describing: source))")
            return
        }
        catch {
            logger.warning(
                "Failed to load cover \(String(describing: source)): \(error.localizedDescription)"
            )
            image = nil
        }
    }
}

extension ImageView {
    /// A library cover, fetched by id and cached by content version.
    init(
        imageRef: BridgeImageRef?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.init(
            source: imageRef.map { .image($0) },
            pointSize: pointSize,
            contentMode: contentMode
        )
    }

}

#if DEBUG
#Preview {
    ImageView(imageRef: nil, pointSize: 120)
        .frame(width: 120, height: 120)
        .previewStores()
}
#endif
