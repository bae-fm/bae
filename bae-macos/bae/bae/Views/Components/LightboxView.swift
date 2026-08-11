import AppKit
import BaeKit
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

    /// What this item shows, in the strip and full-screen alike.
    var content: ImageContent {
        switch source {
        case .gallery(let releaseId, let gallerySource):
            return .releaseImage(releaseId: releaseId, source: gallerySource)
        case .local(let path):
            return .localFile(path: path)
        }
    }
}

struct LightboxView: View {
    let cursor: Cursor<LightboxItem>
    let onUpdate: (Cursor<LightboxItem>) -> Void
    let onDismiss: () -> Void

    @Environment(ImageStore.self)
    private var imageStore
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
                    ThumbnailStrip(
                        cursor: cursor,
                        centered: true,
                        onSelect: { id in
                            var next = cursor
                            next.select(id: id)
                            onUpdate(next)
                        },
                        stroke: { _, isActive in
                            (
                                isActive
                                    ? Color.white : Color.gray.opacity(0.4),
                                isActive ? 2 : 1
                            )
                        }
                    ) { item in
                        ImageView(content: item.content, pointSize: 56)
                    }
                    .padding(.bottom, 12)
                    .fadesWhenZoomed(at: magnification)
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

    /// Where the current item's bytes come from, or nil once the failure has
    /// been logged and put on screen. A cancellation resolves to nil too, with
    /// nothing shown — the item is on its way out.
    private func resolveDecodeSource() async -> ImageLoader.Source? {
        do {
            guard
                let source = try await imageStore.decodeSource(
                    for: cursor.current.content
                )
            else {
                logger.warning(
                    "No bytes for lightbox image \(cursor.current.id)"
                )
                loadFailed = true
                return nil
            }
            return source
        }
        catch is CancellationError {
            return nil
        }
        catch {
            logger.warning(
                "Failed to fetch lightbox image \(cursor.current.id): \(error)"
            )
            loadFailed = true
            return nil
        }
    }

    private func loadCurrentImage(containerSize: CGSize) async {
        loadedImage = nil
        fullResImage = nil
        imageAnalysis = nil
        loadFailed = false
        decodeSource = nil

        guard let source = await resolveDecodeSource() else {
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
                displayScale: displayScale
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
                displayScale: displayScale
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

}

// MARK: - Overlay chrome

extension LightboxView {
    fileprivate func goPrevious() {
        var next = cursor
        next.goToPrevious()
        onUpdate(next)
    }

    fileprivate func goNext() {
        var next = cursor
        next.goToNext()
        onUpdate(next)
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

#if DEBUG
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
                .environment(ImageStore.stub())
        }
    }
#endif
