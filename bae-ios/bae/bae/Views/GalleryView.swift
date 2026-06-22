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
    /// Downward drag for swipe-to-dismiss: the viewer follows the finger and, on
    /// release, dismisses once it's past the threshold (or a fast flick) and
    /// otherwise springs back. Only downward — an upward drag stays at rest, and
    /// horizontal drags belong to the pager.
    @State
    private var dragOffset: CGFloat = 0

    private static let dismissThreshold: CGFloat = 150

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black.ignoresSafeArea()
            TabView(selection: $selection) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    GalleryPage(item: item, loadImage: loadImage)
                        .tag(index)
                }
            }
            // Edge-to-edge so the photo fills the whole screen; the built-in page
            // dots are dropped (they'd land under the home indicator) in favor of
            // the safe-area "n / N" counter below.
            .tabViewStyle(.page(indexDisplayMode: .never))
            .ignoresSafeArea()
            // The current item's label (e.g. "Cover", "Back.jpg") plus, for a
            // multi-image gallery, its position — so it isn't a blind
            // swipe-through. Sits in the safe area and doesn't intercept swipes.
            // `selection` is TabView-bounded and the gallery is never shown empty,
            // so the subscript is safe; core always sets a non-empty label.
            VStack(spacing: 2) {
                Text(items[selection].label)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.85))
                if items.count > 1 {
                    Text("\(selection + 1) / \(items.count)")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(0.6))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
            .padding(.bottom, 24)
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
        .offset(y: dragOffset)
        .simultaneousGesture(dismissDrag)
    }

    // Vertical-dominant downward swipe to dismiss, run alongside the pager's
    // horizontal swipe (simultaneousGesture) so it never steals a page turn. The
    // offset tracks the finger; release past the threshold — or on a fast flick —
    // dismisses, otherwise it springs back.
    private var dismissDrag: some Gesture {
        DragGesture(minimumDistance: 10)
            .onChanged { value in
                let translation = value.translation
                guard translation.height > 0,
                    translation.height > abs(translation.width)
                else { return }
                dragOffset = translation.height
            }
            .onEnded { value in
                let flickedAway =
                    value.predictedEndTranslation.height > Self.dismissThreshold * 2
                if value.translation.height > Self.dismissThreshold || flickedAway {
                    dismiss()
                }
                else {
                    withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
                        dragOffset = 0
                    }
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
