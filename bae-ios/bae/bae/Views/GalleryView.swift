import SwiftUI
import os.log

private let logger = Logger.bae("GalleryView")

/// Full-screen artwork viewer over a release's gallery items (cover first, then
/// every image file the release has). Swipeable when there's more than one. An
/// item already on disk renders from its `localPath`; a cloud-only item (no
/// local path) has its bytes fetched on demand via `loadImage`, keyed by the
/// item's id. Each page pinch-zooms and snaps back on release.
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
/// path and fetches a cloud-only one on demand. Both kinds land in
/// `ZoomableGalleryImage`, which handles the screen-fit-then-full-res decode and
/// the pinch-to-zoom.
private struct GalleryPage: View {
    let item: BridgeGalleryItem
    let loadImage: @Sendable (_ fileId: String) async throws -> Data

    var body: some View {
        if let path = item.localPath {
            ZoomableGalleryImage(source: .local(path: path))
        }
        else {
            RemoteGalleryImage(fileId: item.id, loadImage: loadImage)
        }
    }
}

/// A zoomable gallery image with a two-stage decode that keeps huge JPEGs (e.g.
/// 35 MB) responsive: a screen-fit thumbnail loads first, and the full-res
/// decode runs only once the user starts pinching in. Pinch scales the image
/// around the gesture anchor and releases back to fit, mirroring the macOS
/// lightbox.
private struct ZoomableGalleryImage: View {
    let source: ImageLoader.Source

    @Environment(\.displayScale)
    private var displayScale
    @State
    private var thumbnail: UIImage?
    @State
    private var fullRes: UIImage?
    @State
    private var fullResTask: Task<Void, Never>?
    @State
    private var scale: CGFloat = 1
    @State
    private var anchor: UnitPoint = .center
    @State
    private var failed = false

    var body: some View {
        GeometryReader { geo in
            Group {
                if let image = fullRes ?? thumbnail {
                    Image(uiImage: image)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(
                            maxWidth: .infinity,
                            maxHeight: .infinity
                        )
                        .scaleEffect(scale, anchor: anchor)
                        .gesture(magnifyGesture)
                }
                else if failed {
                    GalleryFailedView()
                }
                else {
                    ProgressView().tint(.white)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            // Decode the screen-fit thumbnail once the page has its size. Each
            // page is a distinct image, so this runs once per page.
            .task {
                await loadThumbnail(containerSize: geo.size)
            }
        }
        // The full-res decode is gesture-driven, so it's an explicitly-managed
        // task rather than a `.task`: cancel it if the page leaves while a big
        // decode is still running.
        .onDisappear {
            fullResTask?.cancel()
            fullResTask = nil
        }
    }

    private var magnifyGesture: some Gesture {
        MagnifyGesture()
            .onChanged { value in
                anchor = value.startAnchor
                scale = max(value.magnification, 1)
                if value.magnification > 1.01,
                    fullRes == nil,
                    fullResTask == nil
                {
                    fullResTask = Task { await loadFullRes() }
                }
            }
            .onEnded { _ in
                withAnimation(.easeOut(duration: 0.25)) {
                    scale = 1
                }
            }
    }

    private func loadThumbnail(containerSize: CGSize) async {
        do {
            thumbnail = try await ImageLoader.load(
                source: source,
                size: .fitTo(
                    points: max(containerSize.width, containerSize.height)
                ),
                displayScale: displayScale
            )
        }
        catch is CancellationError {
            logger.debug(
                "gallery thumbnail load cancelled: \(source.description)"
            )
            return
        }
        catch {
            logger.warning(
                "Failed to decode gallery image (\(source.description)): \(error)"
            )
            failed = true
        }
    }

    private func loadFullRes() async {
        defer { fullResTask = nil }
        do {
            let loaded = try await ImageLoader.load(
                source: source,
                size: .native,
                displayScale: displayScale
            )
            guard !Task.isCancelled else {
                logger.debug(
                    "full-res gallery load cancelled after decode: \(source.description)"
                )
                return
            }
            fullRes = loaded
        }
        catch is CancellationError {
            logger.debug(
                "full-res gallery load cancelled: \(source.description)"
            )
            return
        }
        catch {
            logger.warning(
                "Failed to decode full-res gallery image (\(source.description)): \(error)"
            )
        }
    }
}

/// A gallery image whose file isn't on disk here: fetch its bytes (downloaded
/// from the release's cloud home and decrypted by core), then hand them to
/// `ZoomableGalleryImage` to decode and display. A spinner shows while
/// fetching, a warning glyph if the fetch fails.
private struct RemoteGalleryImage: View {
    let fileId: String
    let loadImage: @Sendable (_ fileId: String) async throws -> Data

    @State
    private var bytes: Data?
    @State
    private var failed = false

    var body: some View {
        Group {
            if let bytes {
                ZoomableGalleryImage(source: .data(bytes))
            }
            else if failed {
                GalleryFailedView()
            }
            else {
                ProgressView().tint(.white)
            }
        }
        .task(id: fileId) {
            do {
                bytes = try await loadImage(fileId)
            }
            catch is CancellationError {
                // The viewer was dismissed mid-fetch; leave state as-is.
                logger.debug(
                    "gallery image fetch cancelled: \(fileId)"
                )
            }
            catch {
                logger.warning(
                    "Failed to load gallery image \(fileId): \(error)"
                )
                failed = true
            }
        }
    }
}

/// The shared failure placeholder for a gallery page (fetch or decode failed).
private struct GalleryFailedView: View {
    var body: some View {
        Image(systemName: "exclamationmark.triangle")
            .font(.largeTitle)
            .foregroundStyle(.white.opacity(0.7))
    }
}
