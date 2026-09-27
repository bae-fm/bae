import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("GalleryView")

/// Full-screen, swipeable viewer over a release's gallery items, each loaded on
/// demand and pinch-zoomable.
struct GalleryView: View {
    let items: [BridgeGalleryItem]
    /// Resolves an item to its image source; nil means it has no bytes.
    let loadImage:
        @Sendable (_ item: BridgeGalleryItem) async throws ->
            ImageLoader.Source?

    @Environment(\.dismiss)
    private var dismiss
    @State
    private var selection = 0
    /// How far the viewer follows a downward swipe-to-dismiss.
    @State
    private var dragOffset: CGFloat = 0

    private static let dismissThreshold: CGFloat = 150

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Theme.backdrop.ignoresSafeArea()
            TabView(selection: $selection) {
                ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                    GalleryPage(item: item, loadImage: loadImage)
                        .tag(index)
                }
            }
            // The page dots would sit under the home indicator, so the counter
            // below replaces them.
            .tabViewStyle(.page(indexDisplayMode: .never))
            .ignoresSafeArea()
            // The current item's label and, with several items, its position.
            VStack(spacing: 2) {
                if let selectedItem {
                    Text(selectedItem.label)
                        .font(.caption)
                        .foregroundStyle(Theme.onFill)
                    if items.count > 1 {
                        Text(
                            verbatim:
                                "\((selection + 1).formatted()) / \(items.count.formatted())"
                        )
                        .font(.caption2)
                        .foregroundStyle(Theme.onFillSecondary)
                    }
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
                    .foregroundStyle(Theme.onFill)
                    .padding()
            }
        }
        .offset(y: dragOffset)
        .simultaneousGesture(dismissDrag)
        .onChange(of: items.count) { _, count in
            clampSelection(toItemCount: count)
        }
    }

    private var selectedItem: BridgeGalleryItem? {
        guard items.indices.contains(selection) else { return nil }
        return items[selection]
    }

    private func clampSelection(toItemCount count: Int) {
        guard !items.indices.contains(selection) else { return }
        selection = min(selection, max(0, count - 1))
    }

    // Runs alongside the pager so a downward swipe dismisses without stealing a
    // page turn.
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

/// One gallery page, a single view per element so the pager's element type
/// stays stable, that fetches the item's bytes on demand.
private struct GalleryPage: View {
    let item: BridgeGalleryItem
    let loadImage:
        @Sendable (_ item: BridgeGalleryItem) async throws ->
            ImageLoader.Source?

    @State
    private var source: ImageLoader.Source?
    @State
    private var failed = false

    var body: some View {
        // Placeholders toggle by opacity because swapping children would churn
        // the pager's layout.
        ZStack {
            if let source {
                ZoomableGalleryImage(source: source)
            }
            GalleryFailedView()
                .opacity(failed ? 1 : 0)
                .allowsHitTesting(failed)
            ProgressView()
                .tint(Theme.onFill)
                .opacity(source == nil && !failed ? 1 : 0)
                .allowsHitTesting(false)
        }
        .task(id: item.id) {
            do {
                guard let resolved = try await loadImage(item) else {
                    logger.warning("No bytes for gallery image \(item.id)")
                    failed = true
                    return
                }
                source = resolved
            }
            catch is CancellationError {
                // The viewer was dismissed mid-fetch; leave state as-is.
                logger.debug("gallery image fetch cancelled: \(item.id)")
            }
            catch {
                logger.warning(
                    "Failed to load gallery image \(item.id): \(error)"
                )
                failed = true
            }
        }
    }
}

/// A pinch-zoomable image that decodes the full resolution only once a pinch
/// starts, so huge JPEGs stay responsive.
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
                        .scaledToFit()
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
                    ProgressView().tint(Theme.onFill)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            // Decode the screen-fit thumbnail once the page has its size.
            .task {
                await loadThumbnail(containerSize: geo.size)
            }
        }
        // The full-resolution decode starts from a gesture, not a `.task`, so
        // it is cancelled here by hand.
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

/// Shown when a gallery page's fetch or decode failed.
private struct GalleryFailedView: View {
    var body: some View {
        Image(systemName: "exclamationmark.triangle")
            .font(.largeTitle)
            .foregroundStyle(Theme.onFillSecondary)
    }
}

#if DEBUG
#Preview {
    GalleryView(items: [], loadImage: { _ in nil })
}
#endif
