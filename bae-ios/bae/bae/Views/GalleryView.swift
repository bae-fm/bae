import SwiftUI
import os.log

private let logger = Logger.bae("GalleryView")

/// Full-screen artwork viewer over a release's gallery items (cover first, then
/// every image file the release has). Swipeable when there's more than one. An
/// item already on disk renders from its `localPath`; a cloud-only item (no
/// local path) has its bytes fetched on demand via `loadImage`, keyed by the
/// item's id.
struct GalleryView: View {
    let items: [BridgeGalleryItem]
    /// Fetches a cloud-only gallery item's bytes by its id, for items whose
    /// `localPath` is nil (image files not downloaded on this device).
    let loadImage: @Sendable (_ fileId: String) async throws -> Data

    @Environment(\.dismiss)
    private var dismiss
    @State
    private var selection = 0

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black.ignoresSafeArea()
            TabView(selection: $selection) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    GalleryPage(item: item, loadImage: loadImage)
                        .tag(index)
                }
            }
            .tabViewStyle(
                .page(indexDisplayMode: items.count > 1 ? .automatic : .never)
            )
            // The current item's label (e.g. "Cover", "Back.jpg") so multi-image
            // galleries aren't a blind swipe-through. Sits above the page dots
            // and doesn't intercept swipes. `selection` is TabView-bounded and
            // the gallery is never shown empty, so the subscript is safe; core
            // always sets a non-empty label ("Cover" or the filename).
            Text(items[selection].label)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.85))
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .padding(.bottom, 44)
                .allowsHitTesting(false)
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.title)
                    .foregroundStyle(.white)
                    .padding()
            }
        }
    }
}

/// One page of the gallery: a single view per `ForEach` element (so the paging
/// container's element type stays stable) that renders an on-disk item from its
/// path and fetches a cloud-only one on demand.
private struct GalleryPage: View {
    let item: BridgeGalleryItem
    let loadImage: @Sendable (_ fileId: String) async throws -> Data

    var body: some View {
        if let path = item.localPath {
            ImageView(path: path, pointSize: 1024, contentMode: .fit)
        }
        else {
            RemoteGalleryImage(fileId: item.id, loadImage: loadImage)
        }
    }
}

/// A gallery image whose file isn't on disk here: fetch its bytes (downloaded
/// from the release's cloud home and decrypted by core), decode off the main
/// thread, and render it — a spinner while loading, a warning glyph on failure.
private struct RemoteGalleryImage: View {
    let fileId: String
    let loadImage: @Sendable (_ fileId: String) async throws -> Data

    @State
    private var image: UIImage?
    @State
    private var failed = false

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
            }
            else if failed {
                Image(systemName: "exclamationmark.triangle")
                    .font(.largeTitle)
                    .foregroundStyle(.white.opacity(0.7))
            }
            else {
                ProgressView()
                    .tint(.white)
            }
        }
        .task(id: fileId) {
            do {
                let data = try await loadImage(fileId)
                let decoded = await Task.detached(priority: .userInitiated) {
                    UIImage(data: data)
                }.value
                if Task.isCancelled {
                    logger.debug(
                        "gallery image decode cancelled: \(fileId, privacy: .public)"
                    )
                    return
                }
                if let decoded {
                    image = decoded
                }
                else {
                    logger.warning(
                        "Couldn't decode gallery image \(fileId, privacy: .public) (\(data.count) bytes)"
                    )
                    failed = true
                }
            }
            catch is CancellationError {
                // The viewer was dismissed mid-fetch; leave state as-is.
                logger.debug(
                    "gallery image fetch cancelled: \(fileId, privacy: .public)"
                )
            }
            catch {
                logger.warning(
                    "Failed to load gallery image \(fileId, privacy: .public): \(error)"
                )
                failed = true
            }
        }
    }
}
