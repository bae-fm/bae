import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("ImageView")

/// Renders one image slot over `ImageStore`: the already-decoded bitmap on the
/// first frame when the store holds one, the theme placeholder until the async
/// load lands, and the placeholder again when there is no such image. A load
/// that failed offers another try. Fetching, decoding, and caching are the
/// store's; this view only draws.
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
    /// The last load's outcome, with the content it was for. A slot whose
    /// content has since changed — a list row reused for another item — reads
    /// as not loaded yet rather than showing the previous item's image.
    @State
    private var lastLoad: SlotLoad?
    /// How many times the person asked for a failed load again. Part of the
    /// load's identity, so asking restarts it; the store caches no failure,
    /// so the restarted load goes back to the source.
    @State
    private var attempt = 0

    /// The bitmap to draw this frame: the completed load, else whatever the
    /// store already has decoded at this size, so a remounting row draws its art
    /// immediately instead of flashing the placeholder.
    private var displayedImage: UIImage? {
        if let lastLoad, lastLoad.content == content, let image = lastLoad.image {
            return image
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
            else if let lastLoad, lastLoad.content == content, lastLoad.failed {
                Button {
                    attempt += 1
                } label: {
                    Rectangle().fill(Theme.placeholder)
                        .overlay {
                            Image(systemName: "arrow.clockwise.circle.fill")
                                .font(.title2)
                                .foregroundStyle(.secondary)
                        }
                }
                .buttonStyle(.plain)
                .accessibilityLabel(
                    String(localized: "Try loading the image again")
                )
            }
            else {
                Rectangle().fill(Theme.placeholder)
            }
        }
        .task(id: LoadRequest(content: content, attempt: attempt)) {
            await load()
        }
    }

    private func load() async {
        let requested = content
        lastLoad = nil
        guard let requested else {
            return
        }
        do {
            let image = try await imageStore.image(
                requested,
                pointSize: pointSize,
                displayScale: displayScale
            )
            lastLoad = SlotLoad(content: requested, image: image, failed: false)
        }
        catch is CancellationError {
            logger.debug("image load cancelled: \(requested.description)")
            return
        }
        catch {
            logger.warning(
                "Failed to load \(requested.description): \(error.localizedDescription)"
            )
            lastLoad = SlotLoad(content: requested, image: nil, failed: true)
        }
    }
}

/// A load's outcome and the content it was for.
private struct SlotLoad {
    let content: ImageContent
    let image: UIImage?
    let failed: Bool
}

/// One load of a slot: the content, and which time of asking.
private struct LoadRequest: Equatable {
    let content: ImageContent?
    let attempt: Int
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
