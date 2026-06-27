import AppKit
import OSLog
import SwiftUI
import VisionKit

private let logger = Logger.bae("LightboxView")

struct LightboxItem: Identifiable, Equatable {
    let id: String
    let label: String
    /// Where the lightbox reads this item's bytes.
    let source: Source

    /// The byte source for a lightbox item: a release's gallery slot (its cover
    /// or an image file) read via the bridge from the whole `BridgeGallerySource`,
    /// or an import-candidate file already on disk (the folder-import gallery).
    enum Source: Equatable {
        case gallery(releaseId: String, source: BridgeGallerySource)
        case local(path: String)
    }

    /// What the thumbnail strip renders for this item (decoded small and cached).
    var thumbnailContent: ImageContent {
        switch source {
        case .gallery(let releaseId, let gallerySource):
            return .library(
                .gallery(
                    releaseId: releaseId,
                    source: gallerySource,
                    cacheId: id
                )
            )
        case .local(let path):
            return .source(.local(path: path))
        }
    }
}

struct LightboxView: View {
    let cursor: Cursor<LightboxItem>
    let onUpdate: (Cursor<LightboxItem>) -> Void
    let onDismiss: () -> Void

    @Environment(MediaPaths.self)
    private var mediaPaths
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var magnification: CGFloat = 1.0
    @State
    private var magnifyAnchor: UnitPoint = .center
    @State
    private var loadedImage: NSImage?
    @State
    private var fullResImage: NSImage?
    /// The resolved decode source for the current item — `.local` for an
    /// import candidate, `.data` for fetched library bytes. Set once when the
    /// fit-to-screen image loads and reused by the on-zoom full-res decode, so
    /// zooming never re-crosses the bridge.
    @State
    private var decodeSource: ImageLoader.Source?
    @State
    private var fullResImageUpgradeTask: Task<Void, Never>?
    @State
    private var imageAnalysis: ImageAnalysis?
    @State
    private var loadFailed = false
    @FocusState
    private var focused: Bool

    private static let analyzer = ImageAnalyzer()

    var body: some View {
        ZStack {
            Color.black.opacity(0.85)
                .ignoresSafeArea()
                .contentShape(Rectangle())
                .onTapGesture { onDismiss() }

            VStack(spacing: 0) {
                GeometryReader { geo in
                    ZStack {
                        imageContent
                        if cursor.canCycle {
                            cycleNavButtons
                        }
                        closeButtonOverlay
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .task(id: cursor.current.id) {
                        await loadCurrentImage(containerSize: geo.size)
                    }
                }

                labelView

                if cursor.canCycle {
                    thumbnailStrip
                }
            }
            .allowsHitTesting(true)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .focusable()
        .focusEffectDisabled()
        .focused($focused)
        .onKeyPress(.escape) {
            onDismiss()
            return .handled
        }
        .onKeyPress(.leftArrow) {
            goPrevious()
            return .handled
        }
        .onKeyPress(.rightArrow) {
            goNext()
            return .handled
        }
        .onAppear { focused = true }
        .onChange(of: cursor.current.id) { _, _ in
            magnification = 1.0
            fullResImageUpgradeTask?.cancel()
            fullResImageUpgradeTask = nil
        }
    }

    /// Resolves the current item to a decodable source, fetching the gallery
    /// bytes from the bridge when the item is a library slot.
    private func resolveDecodeSource(
        _ item: LightboxItem
    ) async throws -> ImageLoader.Source {
        switch item.source {
        case .local(let path):
            return .local(path: path)
        case .gallery(let releaseId, let gallerySource):
            return .data(
                try await mediaPaths.fetchGalleryBytes(releaseId, gallerySource)
            )
        }
    }

    private func loadCurrentImage(containerSize: CGSize) async {
        loadedImage = nil
        fullResImage = nil
        imageAnalysis = nil
        loadFailed = false
        decodeSource = nil

        let source: ImageLoader.Source
        do {
            source = try await resolveDecodeSource(cursor.current)
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "Failed to fetch lightbox image \(cursor.current.id): \(error)"
            )
            loadFailed = true
            return
        }
        decodeSource = source

        let loaded: NSImage
        do {
            loaded = try await ImageLoader.load(
                source: source,
                size: .fitTo(
                    points: max(containerSize.width, containerSize.height)
                ),
                displayScale: displayScale,
                fetchRemoteBytes: mediaPaths.fetchCoverBytes
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning(
                "Failed to decode lightbox image \(cursor.current.id): \(error)"
            )
            loadFailed = true
            return
        }

        loadedImage = loaded

        let configuration = ImageAnalyzer.Configuration([.text])
        do {
            let analysis = try await Self.analyzer.analyze(
                loaded,
                orientation: .up,
                configuration: configuration
            )
            if !Task.isCancelled {
                imageAnalysis = analysis
            }
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.debug("Live Text analysis failed: \(error)")
        }
    }

    private func loadFullResImage() async {
        defer { fullResImageUpgradeTask = nil }
        guard let source = decodeSource else {
            return
        }
        let loaded: NSImage
        do {
            loaded = try await ImageLoader.load(
                source: source,
                size: .native,
                displayScale: displayScale,
                fetchRemoteBytes: mediaPaths.fetchCoverBytes
            )
        }
        catch is CancellationError {
            return
        }
        catch {
            logger.warning("Failed to load full-res image: \(error)")
            return
        }
        guard !Task.isCancelled else {
            return
        }
        fullResImage = loaded
    }

    @ViewBuilder
    private var imageContent: some View {
        if let nsImage = fullResImage ?? loadedImage {
            loadedImageView(nsImage)
        }
        else if loadFailed {
            loadFailedView
        }
        else {
            ProgressView()
                .controlSize(.large)
        }
    }

    private func loadedImageView(_ nsImage: NSImage) -> some View {
        Image(nsImage: nsImage)
            .resizable()
            .aspectRatio(contentMode: .fit)
            .overlay {
                if let analysis = imageAnalysis {
                    LiveTextOverlay(analysis: analysis)
                }
            }
            .scaleEffect(magnification, anchor: magnifyAnchor)
            .gesture(
                MagnifyGesture()
                    .onChanged { value in
                        magnifyAnchor = value.startAnchor
                        magnification = max(value.magnification, 1.0)
                        if value.magnification > 1.01,
                            fullResImage == nil,
                            fullResImageUpgradeTask == nil
                        {
                            fullResImageUpgradeTask = Task {
                                await loadFullResImage()
                            }
                        }
                    }
                    .onEnded { _ in
                        withAnimation(.easeOut(duration: 0.25)) {
                            magnification = 1.0
                        }
                    },
            )
            .padding(.horizontal, 40)
            .padding(.top, 40)
            .padding(.bottom, 16)
            .shadow(color: .black.opacity(0.5), radius: 20)
    }

    private func goPrevious() {
        var next = cursor
        next.goToPrevious()
        onUpdate(next)
    }

    private func goNext() {
        var next = cursor
        next.goToNext()
        onUpdate(next)
    }
}

// MARK: - Overlay chrome

extension LightboxView {
    fileprivate var thumbnailStrip: some View {
        GeometryReader { geo in
            ScrollViewReader { scrollProxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(cursor.items) { thumbnailItem in
                            thumbnailView(
                                for: thumbnailItem,
                                isActive: cursor.isCurrent(thumbnailItem)
                            )
                            .id(thumbnailItem.id)
                        }
                    }
                    .padding(.horizontal, 8)
                    .frame(minWidth: geo.size.width)
                }
                .onAppear {
                    scrollProxy.scrollTo(cursor.current.id, anchor: .center)
                }
                .onChange(of: cursor.current.id) { _, newId in
                    withAnimation(.easeInOut(duration: 0.2)) {
                        scrollProxy.scrollTo(newId, anchor: .center)
                    }
                }
            }
        }
        .frame(height: 64)
        .padding(.bottom, 12)
        .fadesWhenZoomed(at: magnification)
    }

    fileprivate var labelView: some View {
        Text(cursor.current.label)
            .font(.callout)
            .foregroundStyle(.white.opacity(0.7))
            .lineLimit(1)
            .padding(.bottom, 8)
            .fadesWhenZoomed(at: magnification)
    }

    fileprivate var cycleNavButtons: some View {
        HStack {
            circleIconButton(
                systemName: "chevron.left",
                diameter: 48,
                iconFont: .title2.weight(.medium),
                action: goPrevious
            )
            Spacer()
            circleIconButton(
                systemName: "chevron.right",
                diameter: 48,
                iconFont: .title2.weight(.medium),
                action: goNext
            )
        }
        .padding(.horizontal, 16)
        .fadesWhenZoomed(at: magnification)
    }

    fileprivate var closeButtonOverlay: some View {
        VStack {
            HStack {
                Spacer()
                circleIconButton(
                    systemName: "xmark",
                    diameter: 36,
                    iconFont: .body.weight(.semibold)
                ) {
                    onDismiss()
                }
                .padding(12)
            }
            Spacer()
        }
        .fadesWhenZoomed(at: magnification)
    }

    fileprivate func circleIconButton(
        systemName: String,
        diameter: CGFloat,
        iconFont: Font,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            ZStack {
                Circle()
                    .fill(.black.opacity(0.4))
                    .frame(width: diameter, height: diameter)
                Image(systemName: systemName)
                    .font(iconFont)
                    .foregroundStyle(.white.opacity(0.8))
            }
        }
        .buttonStyle(.plain)
    }

    fileprivate var loadFailedView: some View {
        VStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle")
                .font(.largeTitle)
                .foregroundStyle(.gray)
            Text("Couldn't load image")
                .font(.callout)
                .foregroundStyle(.gray)
        }
        .allowsHitTesting(false)
    }

    @ViewBuilder
    fileprivate func thumbnailView(for item: LightboxItem, isActive: Bool)
        -> some View
    {
        ImageView(content: item.thumbnailContent, pointSize: 56)
            .frame(width: 56, height: 56)
            .clipped()
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(
                        isActive ? .white : .gray.opacity(0.4),
                        lineWidth: isActive ? 2 : 1,
                    ),
            )
            .contentShape(Rectangle())
            .onTapGesture {
                var next = cursor
                next.select(id: item.id)
                onUpdate(next)
            }
    }
}

extension View {
    fileprivate func fadesWhenZoomed(at magnification: CGFloat) -> some View {
        opacity(magnification > 1.01 ? 0 : 1)
    }
}

private struct LiveTextOverlay: NSViewRepresentable {
    let analysis: ImageAnalysis

    func makeNSView(context _: Context) -> ImageAnalysisOverlayView {
        let overlayView = ImageAnalysisOverlayView()
        overlayView.preferredInteractionTypes = .textSelection
        overlayView.analysis = analysis
        return overlayView
    }

    func updateNSView(
        _ overlayView: ImageAnalysisOverlayView,
        context _: Context
    ) {
        overlayView.analysis = analysis
    }
}

#Preview {
    if let cursor = Cursor(items: [
        LightboxItem(
            id: "1",
            label: "Front.jpg",
            source: .local(path: "/tmp/fake/Front.jpg")
        ),
        LightboxItem(
            id: "2",
            label: "Back.jpg",
            source: .local(path: "/tmp/fake/Back.jpg")
        ),
    ]) {
        LightboxView(cursor: cursor, onUpdate: { _ in }, onDismiss: {})
            .environment(MediaPaths.stub)
    }
}
