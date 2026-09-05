import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("ImageView")

/// Renders one image slot over `ImageStore`: the already-decoded bitmap on the
/// first frame when the store holds one, the theme placeholder until the async
/// load lands, and the placeholder again when there is no such image. Fetching,
/// decoding, and caching are the store's; this view only draws.
struct ImageView: View {
    let content: ImageContent?
    /// Target point size for the thumbnail; pixels decoded are
    /// `pointSize * displayScale`.
    let pointSize: CGFloat
    var contentMode: ContentMode = .fill

    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var loaded: UIImage?

    /// The bitmap to draw this frame: the completed load, else whatever the
    /// store already has decoded at this size, so a remounting row draws its art
    /// immediately instead of flashing the placeholder.
    private var displayedImage: UIImage? {
        if let loaded {
            return loaded
        }
        guard let content else {
            return nil
        }
        return imageStore.cachedImage(
            content,
            pointSize: pointSize,
            displayScale: displayScale
        )
    }

    var body: some View {
        Group {
            if let image = displayedImage {
                Image(uiImage: image)
                    .resizable()
                    .aspectRatio(contentMode: contentMode)
            }
            else {
                Rectangle().fill(Theme.placeholder)
            }
        }
        .task(id: content) {
            await load()
        }
    }

    private func load() async {
        loaded = nil
        guard let content else {
            return
        }
        do {
            loaded = try await imageStore.image(
                content,
                pointSize: pointSize,
                displayScale: displayScale
            )
        }
        catch is CancellationError {
            logger.debug("image load cancelled: \(content.description)")
            return
        }
        catch {
            logger.warning(
                "Failed to load \(content.description): \(error.localizedDescription)"
            )
            loaded = nil
        }
    }
}

extension ImageView {
    /// A curated library image, cached by its content version.
    init(
        imageRef: BridgeImageRef?,
        contentMode: ContentMode = .fill,
        pointSize: CGFloat
    ) {
        self.init(
            content: imageRef.map { .libraryImage($0) },
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
